#[cfg(target_os = "linux")]
use nix::sys::signal::{self, SaFlags, SigAction, SigHandler, SigSet, Signal};
#[cfg(target_os = "linux")]
use nix::unistd::Pid;
use std::env;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
#[cfg(target_os = "linux")]
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{self, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const PROTO_VERSION: &str = "2";

#[cfg(unix)]
static SIGINT_COUNT: AtomicU32 = AtomicU32::new(0);
#[cfg(unix)]
static GOT_TERM: AtomicBool = AtomicBool::new(false);
#[cfg(unix)]
static GOT_HUP: AtomicBool = AtomicBool::new(false);

#[cfg(target_os = "linux")]
extern "C" fn handle_sigint(_sig: i32) {
    SIGINT_COUNT.fetch_add(1, Ordering::SeqCst);
}
#[cfg(target_os = "linux")]
extern "C" fn handle_term(_sig: i32) {
    GOT_TERM.store(true, Ordering::SeqCst);
}
#[cfg(target_os = "linux")]
extern "C" fn handle_hup(_sig: i32) {
    GOT_HUP.store(true, Ordering::SeqCst);
}

#[cfg(target_os = "linux")]
fn install_signal_handlers() {
    let act_int = SigAction::new(
        SigHandler::Handler(handle_sigint),
        SaFlags::SA_RESTART,
        SigSet::empty(),
    );
    let act_term = SigAction::new(
        SigHandler::Handler(handle_term),
        SaFlags::SA_RESTART,
        SigSet::empty(),
    );
    let act_hup = SigAction::new(
        SigHandler::Handler(handle_hup),
        SaFlags::SA_RESTART,
        SigSet::empty(),
    );
    unsafe {
        let _ = signal::sigaction(Signal::SIGINT, &act_int);
        let _ = signal::sigaction(Signal::SIGTERM, &act_term);
        let _ = signal::sigaction(Signal::SIGHUP, &act_hup);
    }
}

#[cfg(target_os = "linux")]
fn kill_parent_shell_if_interactive() {
    let interactive = atty::is(atty::Stream::Stdin) || atty::is(atty::Stream::Stdout);
    if std::env::var("AIFO_SHIM_KILL_PARENT_SHELL_ON_SIGINT")
        .ok()
        .as_deref()
        .unwrap_or("1")
        != "1"
        || !interactive
    {
        return;
    }
    // Read PPID and possible PGID
    let mut ppid: i32 = 0;
    let mut pgid: i32 = 0;
    if let Ok(stat) = fs::read_to_string("/proc/self/stat") {
        if let Some(rp) = stat.rfind(')') {
            let rest = &stat[rp + 1..];
            let parts: Vec<&str> = rest.split_whitespace().collect();
            if parts.len() >= 6 {
                ppid = parts[1].parse::<i32>().unwrap_or(0);
                pgid = parts[2].parse::<i32>().unwrap_or(0);
            }
        }
    }
    if ppid <= 1 {
        return;
    }
    // Try to identify if parent is a shell
    let mut is_shell = false;
    if let Ok(comm) = fs::read_to_string(format!("/proc/{}/comm", ppid)) {
        let c = comm.trim();
        is_shell = matches!(
            c,
            "sh" | "bash" | "dash" | "zsh" | "ksh" | "ash" | "busybox" | "busybox-sh"
        );
    }
    if !is_shell {
        return;
    }
    let _ = signal::kill(Pid::from_raw(ppid), Signal::SIGHUP);
    std::thread::sleep(std::time::Duration::from_millis(50));
    let _ = signal::kill(Pid::from_raw(ppid), Signal::SIGTERM);
    std::thread::sleep(std::time::Duration::from_millis(300));
    if pgid > 0 && pgid == ppid {
        let _ = signal::kill(Pid::from_raw(-pgid), Signal::SIGHUP);
        std::thread::sleep(std::time::Duration::from_millis(50));
        let _ = signal::kill(Pid::from_raw(-pgid), Signal::SIGTERM);
        std::thread::sleep(std::time::Duration::from_millis(300));
    }
    let _ = signal::kill(Pid::from_raw(ppid), Signal::SIGKILL);
}

fn post_signal(url: &str, token: &str, exec_id: &str, signal_name: &str, verbose: bool) {
    let mut args: Vec<String> = vec![
        "-sS".into(),
        "-X".into(),
        "POST".into(),
        "-H".into(),
        format!("Authorization: Bearer {}", token),
        "-H".into(),
        "X-Aifo-Proto: 2".into(),
        "-H".into(),
        "Content-Type: application/x-www-form-urlencoded".into(),
        "--data-urlencode".into(),
        format!("exec_id={}", exec_id),
        "--data-urlencode".into(),
        format!("signal={}", signal_name),
    ];
    let mut final_url = url.to_string();
    if url.starts_with("unix://") {
        let sock = url.trim_start_matches("unix://").to_string();
        args.push("--unix-socket".into());
        args.push(sock);
        final_url = "http://localhost/signal".to_string();
    } else {
        if let Some(idx) = final_url.rfind("/exec") {
            final_url.truncate(idx);
        }
        final_url.push_str("/signal");
    }
    args.push(final_url);
    let mut cmd = Command::new("curl");
    cmd.args(&args);
    if verbose {
        let joined = args.join(" ");
        eprintln!(
            "\raifo-shim: posting signal {} via curl {}",
            signal_name, joined
        );
    }
    let _ = cmd.status();
}

// Native HTTP/1.1 client (Phase 3): TCP + Linux UDS, chunked request, trailer parsing.
// Returns Some(exit_code) when native path is taken; None to fall back to curl.
fn try_run_native(
    url: &str,
    token: &str,
    exec_id: &str,
    form_parts: &[(String, String)],
    verbose: bool,
) -> Option<i32> {
    // Default enabled; set AIFO_SHIM_NATIVE_HTTP=0 to force curl fallback
    if std::env::var("AIFO_SHIM_NATIVE_HTTP").ok().as_deref() == Some("0") {
        return None;
    }

    // Percent-encode a single component for application/x-www-form-urlencoded
    fn urlencode_component(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        for b in s.bytes() {
            match b {
                b' ' => out.push('+'),
                b'-' | b'_' | b'.' | b'~' => out.push(b as char),
                b'0'..=b'9' | b'a'..=b'z' | b'A'..=b'Z' => out.push(b as char),
                _ => {
                    out.push('%');
                    out.push_str(&format!("{:02X}", b));
                }
            }
        }
        out
    }

    // Build the urlencoded body from provided form parts (tool, cwd, arg=...)
    let mut body = String::new();
    for (i, (k, v)) in form_parts.iter().enumerate() {
        if i > 0 {
            body.push('&');
        }
        body.push_str(&urlencode_component(k));
        body.push('=');
        body.push_str(&urlencode_component(v));
    }

    // Connection abstraction over TCP/UDS
    enum Conn {
        Tcp(TcpStream, String, String), // stream, host header, path
        #[cfg(target_os = "linux")]
        Uds(UnixStream, String), // stream, path (Host: localhost)
    }

    // Parse URL and connect
    let mut conn: Conn = if url.starts_with("unix://") {
        #[cfg(target_os = "linux")]
        {
            let sock = url.trim_start_matches("unix://");
            let path = "/exec".to_string();
            match UnixStream::connect(sock) {
                Ok(stream) => {
                    let _ = stream.set_read_timeout(Some(std::time::Duration::from_millis(150)));
                    let _ = stream.set_write_timeout(Some(std::time::Duration::from_millis(150)));
                    Conn::Uds(stream, path)
                }
                Err(_) => return None, // fall back to curl on connect error
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            return None;
        }
    } else {
        // Expect http://host:port/path
        let rest = url.trim_start_matches("http://").to_string();
        let path_idx = rest.find('/').unwrap_or(rest.len());
        let (host_port, path) = rest.split_at(path_idx);
        let path = if path.is_empty() {
            "/exec".to_string()
        } else {
            path.to_string()
        };
        let (host, port) = if let Some((h, p)) = host_port.split_once(':') {
            let pn = p.parse::<u16>().unwrap_or(80);
            (h.to_string(), pn)
        } else {
            (host_port.to_string(), 80u16)
        };
        let addr = format!("{}:{}", host, port);
        match TcpStream::connect(&addr) {
            Ok(stream) => {
                let _ = stream.set_read_timeout(Some(std::time::Duration::from_millis(150)));
                let _ = stream.set_write_timeout(Some(std::time::Duration::from_millis(150)));
                Conn::Tcp(stream, host, path)
            }
            Err(_) => return None, // fall back to curl on connect error
        }
    };

    // Compose request headers
    let (mut stream_box, host_header, path) = match &mut conn {
        Conn::Tcp(s, host, path) => (s as &mut dyn Write, host.clone(), path.clone()),
        #[cfg(target_os = "linux")]
        Conn::Uds(s, path) => (s as &mut dyn Write, "localhost".to_string(), path.clone()),
    };

    if verbose {
        eprintln!("aifo-shim: variant=rust transport=native");
    }

    let req_line = format!("POST {} HTTP/1.1\r\n", path);
    let headers = format!(
        concat!(
            "Host: {host}\r\n",
            "Authorization: Bearer {tok}\r\n",
            "X-Aifo-Proto: 2\r\n",
            "TE: trailers\r\n",
            "Content-Type: application/x-www-form-urlencoded\r\n",
            "Transfer-Encoding: chunked\r\n",
            "X-Aifo-Exec-Id: {eid}\r\n",
            "Connection: close\r\n",
            "\r\n"
        ),
        host = host_header,
        tok = token,
        eid = exec_id
    );

    // Write request line + headers
    if stream_box.write_all(req_line.as_bytes()).is_err()
        || stream_box.write_all(headers.as_bytes()).is_err()
    {
        return None;
    }

    // Chunk writer
    fn write_chunk<W: Write>(w: &mut W, data: &[u8]) -> std::io::Result<()> {
        write!(w, "{:X}\r\n", data.len())?;
        w.write_all(data)?;
        w.write_all(b"\r\n")?;
        Ok(())
    }

    // Send body as chunks (8 KiB pieces)
    let bytes = body.as_bytes();
    let mut ofs = 0usize;
    while ofs < bytes.len() {
        let end = (ofs + 8192).min(bytes.len());
        if write_chunk(&mut stream_box, &bytes[ofs..end]).is_err() {
            return None;
        }
        ofs = end;
    }
    if stream_box.write_all(b"0\r\n\r\n").is_err() {
        return None;
    }
    let _ = stream_box.flush();

    // Now read response and stream stdout
    // Helper to read until headers end
    fn find_header_end(buf: &[u8]) -> Option<usize> {
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            Some(i + 4)
        } else {
            buf.windows(2).position(|w| w == b"\n\n").map(|i| i + 2)
        }
    }

    // Reader abstraction
    let mut reader_box: Box<dyn Read> = match conn {
        Conn::Tcp(s, _, _) => Box::new(s),
        #[cfg(target_os = "linux")]
        Conn::Uds(s, _) => Box::new(s),
    };

    let mut hdr_buf: Vec<u8> = Vec::with_capacity(4096);
    let mut tmp = [0u8; 1024];
    let mut header_end_idx: Option<usize> = None;

    // Poll headers with timeouts to allow SIG handling
    loop {
        match reader_box.read(&mut tmp) {
            Ok(0) => break,
            Ok(n) => {
                hdr_buf.extend_from_slice(&tmp[..n]);
                if let Some(idx) = find_header_end(&hdr_buf) {
                    header_end_idx = Some(idx);
                    break;
                }
            }
            Err(ref e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                // Check signals
                #[cfg(unix)]
                {
                    let cnt = SIGINT_COUNT.load(Ordering::SeqCst);
                    if cnt >= 1 {
                        let sig = if cnt == 1 {
                            "INT"
                        } else if cnt == 2 {
                            "TERM"
                        } else {
                            "KILL"
                        };
                        post_signal(url, token, exec_id, sig, verbose);
                        #[cfg(target_os = "linux")]
                        {
                            if sig != "KILL" {
                                kill_parent_shell_if_interactive();
                            }
                        }
                        let code = if std::env::var("AIFO_SHIM_EXIT_ZERO_ON_SIGINT")
                            .ok()
                            .as_deref()
                            .unwrap_or("1")
                            == "1"
                        {
                            0
                        } else {
                            match sig {
                                "INT" => 130,
                                "TERM" => 143,
                                _ => 137,
                            }
                        };
                        return Some(code);
                    }
                    if GOT_TERM.load(Ordering::SeqCst) {
                        post_signal(url, token, exec_id, "TERM", verbose);
                        #[cfg(target_os = "linux")]
                        {
                            kill_parent_shell_if_interactive();
                        }
                        let code = if std::env::var("AIFO_SHIM_EXIT_ZERO_ON_SIGINT")
                            .ok()
                            .as_deref()
                            .unwrap_or("1")
                            == "1"
                        {
                            0
                        } else {
                            143
                        };
                        return Some(code);
                    }
                    if GOT_HUP.load(Ordering::SeqCst) {
                        post_signal(url, token, exec_id, "HUP", verbose);
                        #[cfg(target_os = "linux")]
                        {
                            kill_parent_shell_if_interactive();
                        }
                        let code = if std::env::var("AIFO_SHIM_EXIT_ZERO_ON_SIGINT")
                            .ok()
                            .as_deref()
                            .unwrap_or("1")
                            == "1"
                        {
                            0
                        } else {
                            129
                        };
                        return Some(code);
                    }
                }
                continue;
            }
            Err(_) => break,
        }
    }

    let idx = match header_end_idx {
        Some(i) => i,
        None => {
            // No headers; treat as disconnect
            if verbose {
                eprintln!("aifo-coder: disconnect, waiting for process termination...");
                let wait_secs: u64 = std::env::var("AIFO_SHIM_DISCONNECT_WAIT_SECS")
                    .ok()
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(1);
                if wait_secs > 0 {
                    std::thread::sleep(std::time::Duration::from_secs(wait_secs));
                }
                eprintln!("aifo-coder: terminating now");
                eprintln!();
            }
            let ec = if std::env::var("AIFO_SHIM_EXIT_ZERO_ON_DISCONNECT")
                .ok()
                .as_deref()
                != Some("0")
            {
                0
            } else {
                1
            };
            // Keep markers for proxy cleanup; best-effort tmp cleanup
            let tmp_base = std::env::var("TMPDIR")
                .ok()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "/tmp".to_string());
            let tmp_dir = format!("{}/aifo-shim.{}", tmp_base, std::process::id());
            let _ = fs::remove_dir_all(&tmp_dir);
            return Some(ec);
        }
    };

    // Parse headers
    let header_bytes = &hdr_buf[..idx];
    let body_after = &hdr_buf[idx..];
    let header_text = String::from_utf8_lossy(header_bytes);
    let mut is_chunked = false;
    let mut header_exit_code: Option<i32> = None;
    for line in header_text.lines() {
        let l = line.trim();
        let ll = l.to_ascii_lowercase();
        if ll.starts_with("transfer-encoding:") && ll.contains("chunked") {
            is_chunked = true;
        } else if let Some(v) = l.strip_prefix("X-Exit-Code:") {
            if let Ok(n) = v.trim().parse::<i32>() {
                header_exit_code = Some(n);
            }
        } else if ll.starts_with("x-exit-code:") {
            if let Some(idx) = l.find(':') {
                let v = &l[idx + 1..];
                if let Ok(n) = v.trim().parse::<i32>() {
                    header_exit_code = Some(n);
                }
            }
        }
    }

    let mut stdout = std::io::stdout();

    // Stream body
    let mut exit_code: i32 = 1;
    let mut had_trailer = false;

    if is_chunked {
        // Initialize a buffer that already contains any bytes after headers
        let mut buf: Vec<u8> = body_after.to_vec();
        // Helper to read a single line ending in CRLF or LF
        let read_line = |reader: &mut dyn Read, buf: &mut Vec<u8>| -> Option<String> {
            loop {
                if let Some(pos) = buf
                    .windows(2)
                    .position(|w| w == b"\r\n")
                    .or_else(|| buf.iter().position(|&b| b == b'\n'))
                {
                    let (line, rest) = if pos + 1 < buf.len() && buf[pos] == b'\r' {
                        let line = buf[..pos].to_vec();
                        let rest = buf[pos + 2..].to_vec();
                        (line, rest)
                    } else {
                        let line = buf[..pos].to_vec();
                        let rest = buf[pos + 1..].to_vec();
                        (line, rest)
                    };
                    *buf = rest;
                    return String::from_utf8(line).ok();
                }
                let mut tmp2 = [0u8; 1024];
                match reader.read(&mut tmp2) {
                    Ok(0) => return None,
                    Ok(n) => buf.extend_from_slice(&tmp2[..n]),
                    Err(ref e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            || e.kind() == std::io::ErrorKind::TimedOut => {}
                    Err(_) => return None,
                }
                // Signal checks during blocking reads
                #[cfg(unix)]
                {
                    let cnt = SIGINT_COUNT.load(Ordering::SeqCst);
                    if cnt >= 1 {
                        let sig = if cnt == 1 {
                            "INT"
                        } else if cnt == 2 {
                            "TERM"
                        } else {
                            "KILL"
                        };
                        post_signal(url, token, exec_id, sig, verbose);
                        #[cfg(target_os = "linux")]
                        {
                            if sig != "KILL" {
                                kill_parent_shell_if_interactive();
                            }
                        }
                        return Some("__exit__".to_string());
                    }
                    if GOT_TERM.load(Ordering::SeqCst) {
                        post_signal(url, token, exec_id, "TERM", verbose);
                        #[cfg(target_os = "linux")]
                        {
                            kill_parent_shell_if_interactive();
                        }
                        return Some("__exit_term__".to_string());
                    }
                    if GOT_HUP.load(Ordering::SeqCst) {
                        post_signal(url, token, exec_id, "HUP", verbose);
                        #[cfg(target_os = "linux")]
                        {
                            kill_parent_shell_if_interactive();
                        }
                        return Some("__exit_hup__".to_string());
                    }
                }
            }
        };

        while let Some(s) = read_line(&mut *reader_box, &mut buf) {
            let ln = s;
            if ln == "__exit__" || ln == "__exit_term__" || ln == "__exit_hup__" {
                // Signal exit mapping handled by caller of try_run_native (we returned Some(code) earlier)
                // Here, break and treat as disconnect to be safe.
                break;
            }
            let ln_trim = ln.trim();
            if ln_trim.is_empty() {
                continue;
            }
            // Parse chunk size (hex), tolerate extensions
            let mut size_hex = ln_trim;
            if let Some(idx) = ln_trim.find(';') {
                size_hex = &ln_trim[..idx];
            }
            let size = match usize::from_str_radix(size_hex, 16) {
                Ok(v) => v,
                Err(_) => break,
            };
            if size == 0 {
                // Read and parse trailers until blank line
                while let Some(tr) = read_line(&mut *reader_box, &mut buf) {
                    let t = tr.trim();
                    if t.is_empty() {
                        break;
                    }
                    if let Some(v) = t.strip_prefix("X-Exit-Code:") {
                        if let Ok(n) = v.trim().parse::<i32>() {
                            exit_code = n;
                            had_trailer = true;
                        }
                    } else if t.to_ascii_lowercase().starts_with("x-exit-code:") {
                        if let Some(idx) = t.find(':') {
                            let v = &t[idx + 1..];
                            if let Ok(n) = v.trim().parse::<i32>() {
                                exit_code = n;
                                had_trailer = true;
                            }
                        }
                    }
                }
                break;
            }
            // Read size bytes + CRLF
            let mut remaining = size;
            while remaining > 0 {
                if !buf.is_empty() {
                    let take = remaining.min(buf.len());
                    if stdout.write_all(&buf[..take]).is_err() {
                        // client write error: treat as disconnect
                        break;
                    }
                    buf.drain(..take);
                    remaining -= take;
                } else {
                    let mut tmp3 = [0u8; 8192];
                    match reader_box.read(&mut tmp3) {
                        Ok(0) => break, // disconnect
                        Ok(n) => buf.extend_from_slice(&tmp3[..n]),
                        Err(ref e)
                            if e.kind() == std::io::ErrorKind::WouldBlock
                                || e.kind() == std::io::ErrorKind::TimedOut => {}
                        Err(_) => break,
                    }
                }
            }
            // Consume trailing CRLF after chunk
            // Ensure we have at least 2 bytes to drop CRLF
            while buf.len() < 2 {
                let mut tmp4 = [0u8; 64];
                match reader_box.read(&mut tmp4) {
                    Ok(0) => break,
                    Ok(n) => buf.extend_from_slice(&tmp4[..n]),
                    Err(_) => break,
                }
            }
            if buf.starts_with(b"\r\n") {
                buf.drain(..2);
            } else if buf.starts_with(b"\n") {
                buf.drain(..1);
            }
            let _ = stdout.flush();
        }
    } else {
        // Not chunked: write remaining body bytes and drain to EOF
        if !body_after.is_empty() {
            let _ = stdout.write_all(body_after);
        }
        let mut tmpb = [0u8; 8192];
        loop {
            match reader_box.read(&mut tmpb) {
                Ok(0) => break,
                Ok(n) => {
                    let _ = stdout.write_all(&tmpb[..n]);
                }
                Err(ref e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut => {}
                Err(_) => break,
            }
        }
        if let Some(hc) = header_exit_code {
            exit_code = hc;
            had_trailer = true; // treat as we have an exit header
        }
    }

    // Cleanup markers and tmp dir
    if had_trailer {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/home/coder".to_string());
        let d = PathBuf::from(&home).join(".aifo-exec").join(exec_id);
        let _ = fs::remove_dir_all(&d);
    } else {
        if verbose {
            eprintln!("aifo-coder: disconnect, waiting for process termination...");
            let wait_secs: u64 = std::env::var("AIFO_SHIM_DISCONNECT_WAIT_SECS")
                .ok()
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(1);
            if wait_secs > 0 {
                std::thread::sleep(std::time::Duration::from_secs(wait_secs));
            }
            eprintln!("aifo-coder: terminating now");
            eprintln!();
        }
        if std::env::var("AIFO_SHIM_EXIT_ZERO_ON_DISCONNECT")
            .ok()
            .as_deref()
            != Some("0")
        {
            exit_code = 0;
        } else if exit_code == 0 {
            // keep 0 if server told us explicitly
        } else if exit_code == 1 {
            // default fallback on disconnect when not zeroed by env
            exit_code = 1;
        }
    }
    // Best-effort tmp dir cleanup created by caller naming scheme
    let tmp_base = std::env::var("TMPDIR")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "/tmp".to_string());
    let tmp_dir = format!("{}/aifo-shim.{}", tmp_base, std::process::id());
    let _ = fs::remove_dir_all(&tmp_dir);

    Some(exit_code)
}

fn main() {
    let url = match env::var("AIFO_TOOLEEXEC_URL") {
        Ok(v) if !v.trim().is_empty() => v,
        _ => {
            eprintln!("aifo-shim: proxy not configured. Please launch agent with --toolchain.");
            process::exit(86);
        }
    };
    let token = match env::var("AIFO_TOOLEEXEC_TOKEN") {
        Ok(v) if !v.trim().is_empty() => v,
        _ => {
            eprintln!("aifo-shim: proxy token missing. Please launch agent with --toolchain.");
            process::exit(86);
        }
    };
    let verbose = env::var("AIFO_TOOLCHAIN_VERBOSE").ok().as_deref() == Some("1");

    let tool = std::env::args_os()
        .next()
        .and_then(|p| {
            let pb = PathBuf::from(p);
            pb.file_name().map(|s| s.to_string_lossy().to_string())
        })
        .unwrap_or_else(|| "unknown".to_string());

    let cwd = env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| ".".to_string());

    // Generate ExecId (prefer env AIFO_EXEC_ID) and record agent markers for proxy disconnect handling
    let exec_id: String = match env::var("AIFO_EXEC_ID") {
        Ok(v) if !v.trim().is_empty() => v,
        _ => {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_else(|_| std::time::Duration::from_secs(0))
                .as_nanos();
            let pid = std::process::id() as u128;
            format!("{:032x}", now ^ pid)
        }
    };
    let home = env::var("HOME").unwrap_or_else(|_| "/home/coder".to_string());
    let base_dir = PathBuf::from(&home).join(".aifo-exec").join(&exec_id);
    let _ = fs::create_dir_all(&base_dir);
    // Record agent_ppid, agent_tpgid, controlling tty, and no_shell_on_tty marker
    if let Ok(stat) = fs::read_to_string("/proc/self/stat") {
        if let Some(rp) = stat.rfind(')') {
            let rest = &stat[rp + 1..];
            let parts: Vec<&str> = rest.split_whitespace().collect();
            if parts.len() >= 6 {
                let _ = fs::write(base_dir.join("agent_ppid"), parts[1]);
                let _ = fs::write(base_dir.join("agent_tpgid"), parts[5]);
            }
        }
    }
    let tty_path = std::fs::read_link("/proc/self/fd/0")
        .ok()
        .or_else(|| std::fs::read_link("/proc/self/fd/1").ok());
    if let Some(tp) = tty_path {
        let _ = fs::write(base_dir.join("tty"), tp.to_string_lossy().as_bytes());
    }
    let _ = fs::write(base_dir.join("no_shell_on_tty"), b"");

    if verbose {
        eprintln!("aifo-shim: tool={} cwd={} exec_id={}", tool, cwd, exec_id);
        eprintln!(
            "aifo-shim: preparing request to {} (proto={})",
            url, PROTO_VERSION
        );
    }

    // Form parts to be encoded with --data-urlencode
    let mut form_parts: Vec<(String, String)> = Vec::new();
    form_parts.push(("tool".to_string(), tool.clone()));
    form_parts.push(("cwd".to_string(), cwd.clone()));
    for a in std::env::args().skip(1) {
        form_parts.push(("arg".to_string(), a));
    }

    // Install Linux signal handlers before entering native path so Ctrl-C is trapped properly
    #[cfg(target_os = "linux")]
    install_signal_handlers();

    // Try native HTTP client (Phase 3); fall back to curl when disabled or on error
    if let Some(code) = try_run_native(&url, &token, &exec_id, &form_parts, verbose) {
        process::exit(code);
    }

    // Prepare temp directory for header dump
    let tmp_base = env::var("TMPDIR")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "/tmp".to_string());
    let tmp_dir = format!("{}/aifo-shim.{}", tmp_base, std::process::id());
    let _ = fs::create_dir_all(&tmp_dir);
    let header_path = format!("{}/h", tmp_dir);

    // Build curl command
    let mut args: Vec<String> = Vec::new();
    args.push("-sS".to_string());
    args.push("--no-buffer".to_string());
    args.push("-D".to_string());
    args.push(header_path.clone());
    args.push("-X".to_string());
    args.push("POST".to_string());
    args.push("-H".to_string());
    args.push(format!("Authorization: Bearer {}", token));
    args.push("-H".to_string());
    args.push(format!("X-Aifo-Proto: {}", PROTO_VERSION));
    args.push("-H".to_string());
    args.push("TE: trailers".to_string());
    args.push("-H".to_string());
    args.push("Content-Type: application/x-www-form-urlencoded".to_string());
    // Provide ExecId header so proxy can correlate; also exported via env for docker exec path
    args.push("-H".to_string());
    args.push(format!("X-Aifo-Exec-Id: {}", exec_id));

    for (k, v) in &form_parts {
        args.push("--data-urlencode".to_string());
        args.push(format!("{}={}", k, v));
    }

    let mut final_url = url.clone();
    if url.starts_with("unix://") {
        // unix socket mode
        let sock_path = url.trim_start_matches("unix://").to_string();
        args.push("--unix-socket".to_string());
        args.push(sock_path);
        final_url = "http://localhost/exec".to_string();
    }
    args.push(final_url);

    #[cfg(target_os = "linux")]
    install_signal_handlers();

    if verbose {
        eprintln!("aifo-shim: variant=rust transport=curl");
    }

    let mut cmd = Command::new("curl");
    cmd.args(&args);
    cmd.stdout(Stdio::inherit());
    cmd.stderr(Stdio::inherit());
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("aifo-shim: failed to spawn curl: {}", e);
            let _ = fs::remove_dir_all(&tmp_dir);
            process::exit(86);
        }
    };

    // Poll for signals while streaming
    loop {
        // Check if child exited
        if let Ok(Some(_st)) = child.try_wait() {
            break;
        }
        // Handle signals (Unix)
        #[cfg(unix)]
        {
            let cnt = SIGINT_COUNT.load(Ordering::SeqCst);
            if cnt >= 1 {
                let sig = if cnt == 1 {
                    "INT"
                } else if cnt == 2 {
                    "TERM"
                } else {
                    "KILL"
                };
                post_signal(&url, &token, &exec_id, sig, verbose);
                #[cfg(target_os = "linux")]
                {
                    if sig != "KILL" {
                        kill_parent_shell_if_interactive();
                    }
                }
                let _ = child.kill();
                // Determine exit code mapping
                let code = if std::env::var("AIFO_SHIM_EXIT_ZERO_ON_SIGINT")
                    .ok()
                    .as_deref()
                    .unwrap_or("1")
                    == "1"
                {
                    0
                } else {
                    match sig {
                        "INT" => 130,
                        "TERM" => 143,
                        _ => 137,
                    }
                };
                // Keep markers for proxy cleanup
                let _ = child.wait();
                let _ = fs::remove_dir_all(&tmp_dir);
                process::exit(code);
            }
            if GOT_TERM.load(Ordering::SeqCst) {
                post_signal(&url, &token, &exec_id, "TERM", verbose);
                #[cfg(target_os = "linux")]
                {
                    kill_parent_shell_if_interactive();
                }
                let _ = child.kill();
                let code = if std::env::var("AIFO_SHIM_EXIT_ZERO_ON_SIGINT")
                    .ok()
                    .as_deref()
                    .unwrap_or("1")
                    == "1"
                {
                    0
                } else {
                    143
                };
                let _ = child.wait();
                let _ = fs::remove_dir_all(&tmp_dir);
                process::exit(code);
            }
            if GOT_HUP.load(Ordering::SeqCst) {
                post_signal(&url, &token, &exec_id, "HUP", verbose);
                #[cfg(target_os = "linux")]
                {
                    kill_parent_shell_if_interactive();
                }
                let _ = child.kill();
                let code = if std::env::var("AIFO_SHIM_EXIT_ZERO_ON_SIGINT")
                    .ok()
                    .as_deref()
                    .unwrap_or("1")
                    == "1"
                {
                    0
                } else {
                    129
                };
                let _ = child.wait();
                let _ = fs::remove_dir_all(&tmp_dir);
                process::exit(code);
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    let status_success = child.wait().map(|s| s.success()).unwrap_or(false);

    // Parse X-Exit-Code from headers/trailers
    let mut exit_code: i32 = 1;
    let mut had_header = false;
    if let Ok(hdr) = fs::read_to_string(&header_path) {
        for line in hdr.lines() {
            if let Some(v) = line.strip_prefix("X-Exit-Code:") {
                if let Ok(code) = v.trim().parse::<i32>() {
                    exit_code = code;
                    had_header = true;
                }
            }
        }
    } else if status_success {
        // If curl succeeded but header file missing, assume success
        exit_code = 0;
        had_header = true;
    }

    // Cleanup tmp; keep agent markers on disconnect (no header) so proxy can close lingering shell
    let _ = fs::remove_dir_all(&tmp_dir);
    if had_header {
        let _ = fs::remove_dir_all(&base_dir);
    } else {
        // Optional default: zero exit on disconnect unless opted out
        if env::var("AIFO_SHIM_EXIT_ZERO_ON_DISCONNECT")
            .ok()
            .as_deref()
            != Some("0")
        {
            exit_code = 0;
        }
        // In verbose mode, inform user and give proxy logs a brief moment to flush before exiting.
        if env::var("AIFO_TOOLCHAIN_VERBOSE").ok().as_deref() == Some("1") {
            eprintln!("aifo-coder: disconnect, waiting for process termination...");
            let wait_secs: u64 = env::var("AIFO_SHIM_DISCONNECT_WAIT_SECS")
                .ok()
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(1);
            if wait_secs > 0 {
                std::thread::sleep(std::time::Duration::from_secs(wait_secs));
            }
            eprintln!("aifo-coder: terminating now");
            // Ensure the agent prompt appears on a fresh, clean line
            eprintln!();
        }
    }
    process::exit(exit_code);
}
