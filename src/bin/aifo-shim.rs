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

// Notification tools handled via /notify (extendable)
const NOTIFY_TOOLS: &[&str] = &["say"];

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

fn compute_wait_secs(verbose: bool) -> u64 {
    if let Ok(v) = std::env::var("AIFO_SHIM_DISCONNECT_WAIT_SECS") {
        if let Ok(n) = v.trim().parse::<u64>() {
            return n;
        }
    }
    if verbose {
        3
    } else {
        1
    }
}

fn disconnect_wait(verbose: bool) {
    let secs = compute_wait_secs(verbose);
    eprintln!(
        "\raifo-coder: disconnect, waiting for process termination (~{}s)...",
        secs
    );
    // Add an extra blank line after the line to end on a clean new line
    eprintln!();
    if secs > 0 {
        std::thread::sleep(std::time::Duration::from_secs(secs));
    }
    if verbose {
        eprintln!("aifo-coder: terminating now");
        // Ensure the agent prompt appears on a fresh, clean line
        eprintln!();
    }
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

// Best-effort native POST /signal over TCP or Linux UDS; silent on errors.
fn send_signal_native(url: &str, token: &str, exec_id: &str, signal_name: &str) {
    let body = format!("exec_id={}&signal={}", exec_id, signal_name);
    // UDS on Linux
    if url.starts_with("unix://") {
        #[cfg(target_os = "linux")]
        {
            let sock = url.trim_start_matches("unix://");
            let mut stream = match UnixStream::connect(sock) {
                Ok(s) => s,
                Err(_) => return,
            };
            let _ = stream.set_read_timeout(Some(std::time::Duration::from_millis(300)));
            let _ = stream.set_write_timeout(Some(std::time::Duration::from_millis(300)));
            let req = format!(
                concat!(
                    "POST /signal HTTP/1.1\r\n",
                    "Host: localhost\r\n",
                    "Authorization: Bearer {tok}\r\n",
                    "X-Aifo-Proto: 2\r\n",
                    "Content-Type: application/x-www-form-urlencoded\r\n",
                    "Content-Length: {len}\r\n",
                    "Connection: close\r\n",
                    "\r\n"
                ),
                tok = token,
                len = body.len()
            );
            let _ = stream.write_all(req.as_bytes());
            let _ = stream.write_all(body.as_bytes());
            let _ = stream.flush();
            let _ = stream.shutdown(std::net::Shutdown::Both);
        }
        return;
    }

    // Default TCP http://host:port/…
    let rest = url.trim_start_matches("http://").to_string();
    let path_idx = rest.find('/').unwrap_or(rest.len());
    let (host_port, _path) = rest.split_at(path_idx);
    let (host, port) = if let Some((h, p)) = host_port.split_once(':') {
        let pn = p.parse::<u16>().unwrap_or(80);
        (h.to_string(), pn)
    } else {
        (host_port.to_string(), 80u16)
    };
    let addr = format!("{}:{}", host, port);
    let mut stream = match TcpStream::connect(&addr) {
        Ok(s) => s,
        Err(_) => return,
    };
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_millis(300)));
    let _ = stream.set_write_timeout(Some(std::time::Duration::from_millis(300)));
    let req = format!(
        concat!(
            "POST /signal HTTP/1.1\r\n",
            "Host: {host}\r\n",
            "Authorization: Bearer {tok}\r\n",
            "X-Aifo-Proto: 2\r\n",
            "Content-Type: application/x-www-form-urlencoded\r\n",
            "Content-Length: {len}\r\n",
            "Connection: close\r\n",
            "\r\n"
        ),
        host = host,
        tok = token,
        len = body.len()
    );
    let _ = stream.write_all(req.as_bytes());
    let _ = stream.write_all(body.as_bytes());
    let _ = stream.flush();
    let _ = stream.shutdown(std::net::Shutdown::Both);
}

// Best-effort proactive escalation during disconnect wait: INT -> TERM -> KILL.
fn proactive_disconnect_escalation(url: &str, token: &str, exec_id: &str) {
    if std::env::var("AIFO_SHIM_PROACTIVE_DISCONNECT_ESCALATION")
        .ok()
        .as_deref()
        == Some("0")
    {
        return;
    }
    // INT, brief grace, then TERM, longer grace, then KILL.
    send_signal_native(url, token, exec_id, "INT");
    std::thread::sleep(std::time::Duration::from_millis(500));
    send_signal_native(url, token, exec_id, "TERM");
    std::thread::sleep(std::time::Duration::from_millis(1500));
    send_signal_native(url, token, exec_id, "KILL");
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
                    let _ = stream.set_read_timeout(Some(std::time::Duration::from_millis(1000)));
                    let _ = stream.set_write_timeout(Some(std::time::Duration::from_millis(1000)));
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
                let _ = stream.set_read_timeout(Some(std::time::Duration::from_millis(1000)));
                let _ = stream.set_write_timeout(Some(std::time::Duration::from_millis(1000)));
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

    // Write request line + headers (best-effort; tolerate early write errors)
    let _ = stream_box.write_all(req_line.as_bytes());
    let _ = stream_box.write_all(headers.as_bytes());

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
            break;
        }
        ofs = end;
    }
    let _ = stream_box.write_all(b"0\r\n\r\n");
    let _ = stream_box.flush();
    // Do not half-close the write side here: on some stacks (e.g., macOS+Colima) an early shutdown(Write)
    // can race with the server’s chunked writes and cause a broken pipe/RST on the next write.

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
                    || e.kind() == std::io::ErrorKind::TimedOut
                    || e.kind() == std::io::ErrorKind::Interrupted =>
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
                        disconnect_wait(verbose);
                        eprint!("\n\r");
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
                        eprint!("\n\r");
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
                        disconnect_wait(verbose);
                        eprint!("\n\r");
                        return Some(code);
                    }
                }
                // Idle/Interrupted: wait briefly to avoid tight loop and allow server to respond
                std::thread::sleep(std::time::Duration::from_millis(25));
                continue;
            }
            Err(_) => break,
        }
    }

    // If headers not yet found, wait up to header_wait_ms for them to arrive (idle tolerant)
    if header_end_idx.is_none() {
        let wait_ms: u64 = std::env::var("AIFO_SHIM_HEADER_WAIT_MS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(2000);
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(wait_ms);
        let mut tmp_wait = [0u8; 1024];
        while std::time::Instant::now() < deadline && header_end_idx.is_none() {
            match reader_box.read(&mut tmp_wait) {
                Ok(0) => break,
                Ok(n) => {
                    hdr_buf.extend_from_slice(&tmp_wait[..n]);
                    if let Some(idx2) = find_header_end(&hdr_buf) {
                        header_end_idx = Some(idx2);
                        break;
                    }
                }
                Err(ref e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut
                        || e.kind() == std::io::ErrorKind::Interrupted =>
                {
                    std::thread::sleep(std::time::Duration::from_millis(25));
                    continue;
                }
                Err(_) => break,
            }
        }
    }
    let idx = match header_end_idx {
        Some(i) => i,
        None => {
            // No headers observed even after a short grace; finish benignly without escalation.
            let home_rm = std::env::var("HOME").unwrap_or_else(|_| "/home/coder".to_string());
            let d_rm = PathBuf::from(&home_rm).join(".aifo-exec").join(exec_id);
            let _ = fs::remove_dir_all(&d_rm);
            // Best-effort tmp cleanup created by caller naming scheme
            let tmp_base = std::env::var("TMPDIR")
                .ok()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "/tmp".to_string());
            let tmp_dir = format!("{}/aifo-shim.{}", tmp_base, std::process::id());
            let _ = fs::remove_dir_all(&tmp_dir);
            eprint!("\n\r");
            return Some(0);
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
        // Track immediate signal-handling exit code (when we wait inside the reader loop)
        let mut signal_exit: Option<i32> = None;
        // Helper to read a single line ending in CRLF or LF
        let mut read_line = |reader: &mut dyn Read, buf: &mut Vec<u8>| -> Option<String> {
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
                            || e.kind() == std::io::ErrorKind::TimedOut
                            || e.kind() == std::io::ErrorKind::Interrupted =>
                    {
                        // Timeout/idle/interrupted: wait briefly and continue to avoid premature disconnect
                        std::thread::sleep(std::time::Duration::from_millis(25));
                        continue;
                    }
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
                        disconnect_wait(verbose);
                        signal_exit = Some(code);
                        return Some("__signal__".to_string());
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
                        disconnect_wait(verbose);
                        signal_exit = Some(code);
                        return Some("__signal__".to_string());
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
                        disconnect_wait(verbose);
                        signal_exit = Some(code);
                        return Some("__signal__".to_string());
                    }
                }
            }
        };

        while let Some(s) = read_line(&mut *reader_box, &mut buf) {
            let ln = s;
            if ln == "__signal__" {
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
                                || e.kind() == std::io::ErrorKind::TimedOut
                                || e.kind() == std::io::ErrorKind::Interrupted => {
                            // Avoid busy-spin; keep waiting for more data to arrive
                            std::thread::sleep(std::time::Duration::from_millis(25));
                            continue;
                        }
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
                    Err(ref e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            || e.kind() == std::io::ErrorKind::TimedOut
                            || e.kind() == std::io::ErrorKind::Interrupted =>
                    {
                        // Idle gap before CRLF: wait briefly and continue to avoid premature disconnect
                        std::thread::sleep(std::time::Duration::from_millis(25));
                        continue;
                    }
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
        // If a signal was handled, close stream, cleanup markers, and exit now
        if let Some(code) = signal_exit {
            // Close the stream so the proxy can continue cleanup/logging
            std::mem::drop(reader_box);
            let home = std::env::var("HOME").unwrap_or_else(|_| "/home/coder".to_string());
            let d = PathBuf::from(&home).join(".aifo-exec").join(exec_id);
            let _ = fs::remove_dir_all(&d);
            eprint!("\n\r");
            return Some(code);
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
                        || e.kind() == std::io::ErrorKind::TimedOut
                        || e.kind() == std::io::ErrorKind::Interrupted => {
                    // Idle period or interrupted: wait briefly and continue to avoid premature disconnect
                    std::thread::sleep(std::time::Duration::from_millis(25));
                    continue;
                }
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
        // Benign finish: do not treat missing trailers as disconnect; avoid sending signals or waiting.
        let home_rm = std::env::var("HOME").unwrap_or_else(|_| "/home/coder".to_string());
        let d_rm = PathBuf::from(&home_rm).join(".aifo-exec").join(exec_id);
        let _ = fs::remove_dir_all(&d_rm);
        // Default to success unless server provided a non-zero exit code earlier.
        if exit_code != 0 {
            exit_code = 0;
        }
    }
    // Best-effort tmp dir cleanup created by caller naming scheme
    let tmp_base = std::env::var("TMPDIR")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "/tmp".to_string());
    let tmp_dir = format!("{}/aifo-shim.{}", tmp_base, std::process::id());
    let _ = fs::remove_dir_all(&tmp_dir);

    eprint!("\n\r");

    Some(exit_code)
}

fn try_notify_native(
    url: &str,
    token: &str,
    cmd: &str,
    args: &[String],
    _verbose: bool,
) -> Option<i32> {
    // Default enabled; set AIFO_SHIM_NATIVE_HTTP=0 to force curl fallback
    if std::env::var("AIFO_SHIM_NATIVE_HTTP").ok().as_deref() == Some("0") {
        return None;
    }

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

    let mut body = String::new();
    body.push_str("cmd=");
    body.push_str(&urlencode_component(cmd));
    for a in args {
        body.push('&');
        body.push_str("arg=");
        body.push_str(&urlencode_component(a));
    }

    enum Conn {
        Tcp(TcpStream, String), // stream, host header
        #[cfg(target_os = "linux")]
        Uds(UnixStream),
    }

    // Connect
    let mut conn: Conn = if url.starts_with("unix://") {
        #[cfg(target_os = "linux")]
        {
            let sock = url.trim_start_matches("unix://");
            match UnixStream::connect(sock) {
                Ok(stream) => {
                    let _ = stream.set_read_timeout(Some(std::time::Duration::from_millis(1000)));
                    let _ = stream.set_write_timeout(Some(std::time::Duration::from_millis(1000)));
                    Conn::Uds(stream)
                }
                Err(_) => return None,
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            return None;
        }
    } else {
        let rest = url.trim_start_matches("http://").to_string();
        let path_idx = rest.find('/').unwrap_or(rest.len());
        let (host_port, _path) = rest.split_at(path_idx);
        let (host, port) = if let Some((h, p)) = host_port.split_once(':') {
            let pn = p.parse::<u16>().unwrap_or(80);
            (h.to_string(), pn)
        } else {
            (host_port.to_string(), 80u16)
        };
        let addr = format!("{}:{}", host, port);
        match TcpStream::connect(&addr) {
            Ok(stream) => {
                let _ = stream.set_read_timeout(Some(std::time::Duration::from_millis(1000)));
                let _ = stream.set_write_timeout(Some(std::time::Duration::from_millis(1000)));
                Conn::Tcp(stream, host)
            }
            Err(_) => return None,
        }
    };

    // Write request
    let (w, host_header) = match &mut conn {
        Conn::Tcp(s, host) => (s as &mut dyn Write, host.clone()),
        #[cfg(target_os = "linux")]
        Conn::Uds(s) => (s as &mut dyn Write, "localhost".to_string()),
    };
    let req = format!(
        concat!(
            "POST /notify HTTP/1.1\r\n",
            "Host: {host}\r\n",
            "Authorization: Bearer {tok}\r\n",
            "X-Aifo-Proto: 2\r\n",
            "X-Aifo-Client: rust-shim-native\r\n",
            "Content-Type: application/x-www-form-urlencoded\r\n",
            "Content-Length: {len}\r\n",
            "Connection: close\r\n",
            "\r\n"
        ),
        host = host_header,
        tok = token,
        len = body.len()
    );
    // Best-effort writes: proceed to read response even if peer closed early.
    let _ = w.write_all(req.as_bytes());
    let _ = w.write_all(body.as_bytes());
    let _ = w.flush();

    // Read response, print body, parse X-Exit-Code
    let mut buf = Vec::new();
    let mut tmp = [0u8; 2048];
    let mut reader: Box<dyn Read> = match conn {
        Conn::Tcp(s, _) => Box::new(s),
        #[cfg(target_os = "linux")]
        Conn::Uds(s) => Box::new(s),
    };
    loop {
        match reader.read(&mut tmp) {
            Ok(0) => break,
            Ok(n) => buf.extend_from_slice(&tmp[..n]),
            Err(ref e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                continue;
            }
            Err(_) => break,
        }
    }
    // Find header end
    let header_end = if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
        i + 4
    } else if let Some(i) = buf.windows(2).position(|w| w == b"\n\n") {
        i + 2
    } else {
        buf.len()
    };
    let header_bytes = &buf[..header_end];
    let mut exit_code: i32 = 1;
    for line in String::from_utf8_lossy(header_bytes).lines() {
        let ll = line.to_ascii_lowercase();
        if ll.starts_with("x-exit-code:") {
            if let Some(idx) = line.find(':') {
                if let Ok(n) = line[idx + 1..].trim().parse::<i32>() {
                    exit_code = n;
                }
            }
        }
    }
    let mut stdout = std::io::stdout();
    if header_end < buf.len() {
        let _ = stdout.write_all(&buf[header_end..]);
    }
    loop {
        match reader.read(&mut tmp) {
            Ok(0) => break,
            Ok(n) => {
                let _ = stdout.write_all(&tmp[..n]);
            }
            Err(ref e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                continue;
            }
            Err(_) => break,
        }
    }
    let _ = stdout.flush();
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
    if verbose {
        eprintln!(
            "aifo-shim: build={} target={} profile={} rust={} ver={}",
            env!("AIFO_SHIM_BUILD_DATE"),
            env!("AIFO_SHIM_BUILD_TARGET"),
            env!("AIFO_SHIM_BUILD_PROFILE"),
            env!("AIFO_SHIM_BUILD_RUSTC"),
            env!("CARGO_PKG_VERSION")
        );
    }

    let tool = std::env::args_os()
        .next()
        .and_then(|p| {
            let pb = PathBuf::from(p);
            pb.file_name().map(|s| s.to_string_lossy().to_string())
        })
        .unwrap_or_else(|| "unknown".to_string());

    // Notification tools early path (native + curl)
    if NOTIFY_TOOLS.contains(&tool.as_str()) {
        let start = std::time::Instant::now();
        if verbose {
            let prefer_native = std::env::var("AIFO_SHIM_NATIVE_HTTP").ok().as_deref() != Some("0");
            let client = if prefer_native {
                "rust-shim-native"
            } else {
                "rust-shim-curl"
            };
            // Emit aifo-coder-style parsed line on agent stdout to avoid cross-stream races
            let cwd_verbose = env::current_dir()
                .ok()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| ".".to_string());
            let argv_joined_verbose = std::env::args().skip(1).collect::<Vec<_>>().join(" ");
            println!(
                "aifo-coder: proxy notify parsed cmd={} argv='{}' cwd={} client={}",
                tool, argv_joined_verbose, cwd_verbose, client
            );
            if std::env::var("AIFO_SHIM_LOG_VARIANT").ok().as_deref() == Some("1") {
                println!(
                    "aifo-shim: variant=rust transport={}",
                    if prefer_native { "native" } else { "curl" }
                );
            }
            println!(
                "aifo-shim: notify cmd={} argv={} client={}",
                tool,
                std::env::args().skip(1).collect::<Vec<_>>().join(" "),
                client
            );
            println!(
                "aifo-shim: preparing request to /notify (proto={}) client={}",
                PROTO_VERSION, client
            );
        }
        let args_vec: Vec<String> = std::env::args().skip(1).collect();
        let async_mode =
            !verbose && std::env::var("AIFO_SHIM_NOTIFY_ASYNC").ok().as_deref() != Some("0");
        if async_mode {
            // Fire-and-forget notify via curl without waiting for response
            let mut final_url = url.clone();
            let mut curl_args: Vec<String> = Vec::new();
            curl_args.push("-sS".to_string());
            curl_args.push("-X".to_string());
            curl_args.push("POST".to_string());
            curl_args.push("-H".to_string());
            curl_args.push(format!("Authorization: Bearer {}", token));
            curl_args.push("-H".to_string());
            curl_args.push("X-Aifo-Proto: 2".to_string());
            curl_args.push("-H".to_string());
            curl_args.push("X-Aifo-Client: rust-shim-curl".to_string());
            curl_args.push("-H".to_string());
            curl_args.push("Content-Type: application/x-www-form-urlencoded".to_string());
            curl_args.push("--data-urlencode".to_string());
            curl_args.push(format!("cmd={}", tool));
            for a in &args_vec {
                curl_args.push("--data-urlencode".to_string());
                curl_args.push(format!("arg={}", a));
            }
            if final_url.starts_with("unix://") {
                let sock_path = final_url.trim_start_matches("unix://").to_string();
                curl_args.push("--unix-socket".to_string());
                curl_args.push(sock_path);
                final_url = "http://localhost/notify".to_string();
            } else {
                if let Some(idx) = final_url.rfind("/exec") {
                    final_url.truncate(idx);
                }
                final_url.push_str("/notify");
            }
            curl_args.push(final_url);
            let mut cmd = Command::new("curl");
            cmd.args(&curl_args)
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            if cmd.spawn().is_ok() {
                process::exit(0);
            }
            // If spawning curl failed, fall back to synchronous behavior below
        }
        if let Some(code) = try_notify_native(&url, &token, &tool, &args_vec, verbose) {
            if verbose {
                let dur_ms = start.elapsed().as_millis();
                println!(
                    "aifo-coder: proxy result tool={} kind=notify code={} dur_ms={}",
                    tool, code, dur_ms
                );
                let delay = std::env::var("AIFO_NOTIFY_EXIT_DELAY_SECS")
                    .ok()
                    .and_then(|s| s.parse::<f64>().ok())
                    .unwrap_or(0.5);
                let ms = (delay * 1000.0) as u64;
                std::thread::sleep(std::time::Duration::from_millis(ms));
            }
            process::exit(code);
        }
        if verbose {
            println!("aifo-shim: native HTTP failed, falling back to curl");
        }

        // Prepare temp headers file
        let tmp_base = env::var("TMPDIR")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "/tmp".to_string());
        let tmp_dir = format!("{}/aifo-shim.{}", tmp_base, std::process::id());
        let _ = fs::create_dir_all(&tmp_dir);
        let header_path = format!("{}/h", tmp_dir);

        // Build curl args
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
        args.push("X-Aifo-Client: rust-shim-curl".to_string());
        args.push("-H".to_string());
        args.push("Content-Type: application/x-www-form-urlencoded".to_string());

        args.push("--data-urlencode".to_string());
        args.push(format!("cmd={}", tool));
        for a in &args_vec {
            args.push("--data-urlencode".to_string());
            args.push(format!("arg={}", a));
        }

        let mut final_url = url.clone();
        if url.starts_with("unix://") {
            let sock_path = url.trim_start_matches("unix://").to_string();
            args.push("--unix-socket".to_string());
            args.push(sock_path);
            final_url = "http://localhost/notify".to_string();
        } else {
            if let Some(idx) = final_url.rfind("/exec") {
                final_url.truncate(idx);
            }
            final_url.push_str("/notify");
        }
        args.push(final_url);

        let status_success = Command::new("curl")
            .args(&args)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        let mut exit_code: i32 = 1;
        if let Ok(hdr) = fs::read_to_string(&header_path) {
            for line in hdr.lines() {
                if line.to_ascii_lowercase().starts_with("x-exit-code:") {
                    if let Some(idx) = line.find(':') {
                        if let Ok(n) = line[idx + 1..].trim().parse::<i32>() {
                            exit_code = n;
                            break;
                        }
                    }
                }
            }
        } else if status_success {
            exit_code = 1;
        }
        let _ = fs::remove_dir_all(&tmp_dir);
        if verbose {
            let dur_ms = start.elapsed().as_millis();
            println!(
                "aifo-coder: proxy result tool={} kind=notify code={} dur_ms={}",
                tool, exit_code, dur_ms
            );
            let delay = std::env::var("AIFO_NOTIFY_EXIT_DELAY_SECS")
                .ok()
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(0.5);
            let ms = (delay * 1000.0) as u64;
            std::thread::sleep(std::time::Duration::from_millis(ms));
        }
        process::exit(exit_code);
    }

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
        let prefer_native = std::env::var("AIFO_SHIM_NATIVE_HTTP").ok().as_deref() != Some("0");
        eprintln!(
            "aifo-shim: variant=rust transport={}",
            if prefer_native { "native" } else { "curl" }
        );
        eprintln!("aifo-shim: tool={} cwd={} exec_id={}", tool, cwd, exec_id);
        eprintln!(
            "aifo-shim: preparing request to /exec (proto={})",
            PROTO_VERSION
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
    if verbose {
        eprintln!("aifo-shim: native HTTP failed, falling back to curl");
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
                // Inform user and wait briefly so proxy logs can flush cleanly
                disconnect_wait(verbose);
                // Keep markers for proxy cleanup
                let _ = child.wait();
                let _ = fs::remove_dir_all(&tmp_dir);
                eprint!("\n\r");
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
                eprint!("\n\r");
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
                eprint!("\n\r");
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
        // Proactively drive escalation on the proxy while we wait.
        proactive_disconnect_escalation(&url, &token, &exec_id);
        // Inform user and wait so proxy logs can flush before returning control
        disconnect_wait(verbose);
        // Remove exec markers so the shell wrapper won't auto-exit on the next /run
        let _ = fs::remove_dir_all(&base_dir);
    }
    eprint!("\n\r");
    process::exit(exit_code);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};

    fn find_header_end(buf: &[u8]) -> Option<usize> {
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            Some(i + 4)
        } else {
            buf.windows(2).position(|w| w == b"\n\n").map(|i| i + 2)
        }
    }

    #[allow(dead_code)]
    fn read_all(stream: &mut TcpStream) -> Vec<u8> {
        let mut buf = Vec::new();
        let mut tmp = [0u8; 2048];
        loop {
            match stream.read(&mut tmp) {
                Ok(0) => break,
                Ok(n) => buf.extend_from_slice(&tmp[..n]),
                Err(_) => break,
            }
        }
        buf
    }

    // Tiny helper: read only until end-of-headers (CRLFCRLF or LFLF) with a small timeout.
    fn read_until_header_end(stream: &mut TcpStream, max_ms: u64) -> Vec<u8> {
        let _ = stream.set_read_timeout(Some(std::time::Duration::from_millis(max_ms)));
        let mut buf = Vec::<u8>::new();
        let mut tmp = [0u8; 1024];
        loop {
            if find_header_end(&buf).is_some() {
                break;
            }
            match stream.read(&mut tmp) {
                Ok(0) => break,
                Ok(n) => buf.extend_from_slice(&tmp[..n]),
                Err(ref e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut =>
                {
                    break;
                }
                Err(_) => break,
            }
        }
        buf
    }

    // Tiny helper: read up to cap_bytes more, bounded by max_ms timeout.
    fn read_some_with_timeout(stream: &mut TcpStream, cap_bytes: usize, max_ms: u64) -> Vec<u8> {
        let _ = stream.set_read_timeout(Some(std::time::Duration::from_millis(max_ms)));
        let mut out = Vec::<u8>::new();
        let mut tmp = [0u8; 1024];
        loop {
            if out.len() >= cap_bytes {
                break;
            }
            match stream.read(&mut tmp) {
                Ok(0) => break,
                Ok(n) => {
                    let take = n.min(cap_bytes.saturating_sub(out.len()));
                    out.extend_from_slice(&tmp[..take]);
                    if take < n {
                        break;
                    }
                }
                Err(ref e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut =>
                {
                    break;
                }
                Err(_) => break,
            }
        }
        out
    }

    fn decode_chunked_body(body: Vec<u8>) -> Vec<u8> {
        let mut out = Vec::new();
        let mut cursor = 0usize;
        // Simple decoder tolerant to LF or CRLF
        loop {
            // read size line
            let mut line = Vec::new();
            while cursor < body.len() {
                let b = body[cursor];
                cursor += 1;
                if b == b'\r' {
                    if cursor < body.len() && body[cursor] == b'\n' {
                        cursor += 1;
                    }
                    break;
                } else if b == b'\n' {
                    break;
                } else {
                    line.push(b);
                }
            }
            if line.is_empty() {
                // tolerate empty lines
                continue;
            }
            let size_str = String::from_utf8_lossy(&line);
            let size_hex = size_str.split(';').next().unwrap_or(&size_str);
            let size = usize::from_str_radix(size_hex.trim(), 16).unwrap_or(0);
            if size == 0 {
                // drain any trailers until blank
                loop {
                    let mut tr = Vec::new();
                    while cursor < body.len() {
                        let b = body[cursor];
                        cursor += 1;
                        if b == b'\r' {
                            if cursor < body.len() && body[cursor] == b'\n' {
                                cursor += 1;
                            }
                            break;
                        } else if b == b'\n' {
                            break;
                        } else {
                            tr.push(b);
                        }
                    }
                    if tr.is_empty() {
                        break;
                    }
                }
                break;
            }
            // copy payload
            let end = cursor.saturating_add(size).min(body.len());
            out.extend_from_slice(&body[cursor..end]);
            cursor = end;
            // consume trailing CRLF/LF
            if cursor < body.len() && body[cursor] == b'\r' {
                cursor += 1;
                if cursor < body.len() && body[cursor] == b'\n' {
                    cursor += 1;
                }
            } else if cursor < body.len() && body[cursor] == b'\n' {
                cursor += 1;
            }
        }
        out
    }

    #[test]
    fn test_notify_urlencode_component_covers_star_and_space() {
        // Spin up local server to capture request body
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind");
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            if let Ok((mut s, _a)) = listener.accept() {
                // Read only until header end, then a small slice of body to avoid deadlock.
                let mut buf = read_until_header_end(&mut s, 200);
                // Try to capture a bit more body if already available.
                let more = read_some_with_timeout(&mut s, 4096, 200);
                buf.extend_from_slice(&more);
                let idx = find_header_end(&buf).unwrap_or(buf.len());
                let body = String::from_utf8_lossy(&buf[idx..]).to_string();
                // Respond OK immediately
                let resp = "HTTP/1.1 200 OK\r\nX-Exit-Code: 0\r\nContent-Length: 2\r\nConnection: close\r\n\r\nOK";
                let _ = s.write_all(resp.as_bytes());
                assert!(
                    body.contains("cmd=say%2A") || body.contains("cmd=say%2a"),
                    "expected '*' encoded, got: {}",
                    body
                );
                assert!(
                    body.contains("arg=a+b") || body.contains("arg=a%20b"),
                    "expected space encoded (+ or %20), got: {}",
                    body
                );
            }
        });
        let url = format!("http://127.0.0.1:{}/notify", port);
        let token = "t";
        let cmd = "say*";
        let args = vec!["a b".to_string()];
        let code = try_notify_native(&url, token, cmd, &args, false).expect("native");
        assert_eq!(code, 0);
    }

    #[test]
    fn test_exec_chunked_trailer_exit_code_124() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind");
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            if let Ok((mut s, _a)) = listener.accept() {
                // Read headers quickly, then a bounded slice of the body.
                let mut buf = read_until_header_end(&mut s, 200);
                let more = read_some_with_timeout(&mut s, 8192, 200);
                buf.extend_from_slice(&more);
                let idx = find_header_end(&buf).unwrap_or(buf.len());
                let body = decode_chunked_body(buf[idx..].to_vec());
                let body_s = String::from_utf8_lossy(&body);
                assert!(
                    body_s.contains("tool=") && body_s.contains("cwd="),
                    "expected form parts, got: {}",
                    body_s
                );
                // Chunked response with trailer
                let resp = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n4\r\nok\r\n0\r\nX-Exit-Code: 124\r\n\r\n";
                let _ = s.write_all(resp.as_bytes());
            }
        });
        let url = format!("http://127.0.0.1:{}/exec", port);
        let token = "t";
        let exec_id = "e1";
        let parts = vec![
            ("tool".to_string(), "cargo".to_string()),
            ("cwd".to_string(), ".".to_string()),
            ("arg".to_string(), "--help".to_string()),
        ];
        let code = try_run_native(&url, token, exec_id, &parts, false).expect("native");
        assert_eq!(code, 124);
    }

    #[test]
    fn test_header_end_lf_only_parsing_on_notify() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind");
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            if let Ok((mut s, _a)) = listener.accept() {
                // LF-only header terminator
                let resp =
                    "HTTP/1.1 200 OK\nX-Exit-Code: 86\nContent-Length: 0\nConnection: close\n\n";
                let _ = s.write_all(resp.as_bytes());
            }
        });
        let url = format!("http://127.0.0.1:{}/notify", port);
        let token = "t";
        let cmd = "say";
        let args = vec!["hi".to_string()];
        let code = try_notify_native(&url, token, cmd, &args, false).expect("native");
        assert_eq!(code, 86);
    }

    #[test]
    fn test_disconnect_exit_code_default_and_override() {
        // Use two independent listeners to avoid race between sequential runs.
        std::env::set_var("AIFO_SHIM_DISCONNECT_WAIT_SECS", "0");

        // First run: default zero-on-disconnect
        let listener1 = TcpListener::bind(("127.0.0.1", 0)).expect("bind");
        let port1 = listener1.local_addr().unwrap().port();
        let (tx1, rx1) = std::sync::mpsc::channel::<()>();
        let handle1 = std::thread::spawn(move || {
            let _ = tx1.send(());
            if let Ok((mut s, _a)) = listener1.accept() {
                // Read a bit of the request to allow client to finish writes before we reply.
                let _ = read_until_header_end(&mut s, 200);
                let _ = read_some_with_timeout(&mut s, 4096, 200);
                let resp =
                    "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n";
                let _ = s.write_all(resp.as_bytes());
                // Keep socket alive briefly to avoid immediate RST and let client parse headers.
                std::thread::sleep(std::time::Duration::from_millis(75));
            }
        });
        let _ = rx1.recv_timeout(std::time::Duration::from_millis(200));
        let url1 = format!("http://127.0.0.1:{}/exec", port1);
        let token = "t";
        let exec_id = "e2";
        let parts = vec![
            ("tool".to_string(), "node".to_string()),
            ("cwd".to_string(), ".".to_string()),
        ];
        std::env::remove_var("AIFO_SHIM_EXIT_ZERO_ON_DISCONNECT");
        let code1 = try_run_native(&url1, token, exec_id, &parts, false).expect("native");
        assert_eq!(code1, 0, "default should be zero on disconnect");

        // Second run: force non-zero on disconnect
        let listener2 = TcpListener::bind(("127.0.0.1", 0)).expect("bind");
        let port2 = listener2.local_addr().unwrap().port();
        let (tx2, rx2) = std::sync::mpsc::channel::<()>();
        let handle2 = std::thread::spawn(move || {
            let _ = tx2.send(());
            if let Ok((mut s, _a)) = listener2.accept() {
                let _ = read_until_header_end(&mut s, 200);
                let _ = read_some_with_timeout(&mut s, 4096, 200);
                let resp =
                    "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n";
                let _ = s.write_all(resp.as_bytes());
                std::thread::sleep(std::time::Duration::from_millis(75));
            }
        });
        let _ = rx2.recv_timeout(std::time::Duration::from_millis(200));
        let url2 = format!("http://127.0.0.1:{}/exec", port2);
        std::env::set_var("AIFO_SHIM_EXIT_ZERO_ON_DISCONNECT", "0");
        let code2 = try_run_native(&url2, token, exec_id, &parts, false).expect("native");
        assert_eq!(code2, 1, "override should yield non-zero on disconnect");

        // Cleanup
        std::env::remove_var("AIFO_SHIM_EXIT_ZERO_ON_DISCONNECT");
        std::env::remove_var("AIFO_SHIM_DISCONNECT_WAIT_SECS");
        let _ = handle1.join();
        let _ = handle2.join();
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_send_signal_native_uds_basic() {
        use std::os::unix::net::UnixListener;
        use std::time::Duration;

        // Prepare a temporary unix socket
        let td = tempfile::tempdir().expect("tmpdir");
        let sock_path = td.path().join("shim-signal.sock");
        let listener = UnixListener::bind(&sock_path).expect("bind uds");

        // Spawn server to capture request and assert headers/body
        let handle = std::thread::spawn(move || {
            let (mut s, _addr) = listener.accept().expect("accept");
            let _ = s.set_read_timeout(Some(Duration::from_millis(300)));
            let _ = s.set_write_timeout(Some(Duration::from_millis(300)));

            let mut buf = Vec::new();
            let mut tmp = [0u8; 1024];

            // Read until end of headers
            loop {
                match s.read(&mut tmp) {
                    Ok(0) => break,
                    Ok(n) => {
                        buf.extend_from_slice(&tmp[..n]);
                        if let Some(_idx) = find_header_end(&buf) {
                            break;
                        }
                    }
                    Err(ref e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            || e.kind() == std::io::ErrorKind::TimedOut =>
                    {
                        break;
                    }
                    Err(_) => break,
                }
            }
            // Attempt to read a bit more (body) with a tiny timeout window
            let start = std::time::Instant::now();
            loop {
                match s.read(&mut tmp) {
                    Ok(0) => break,
                    Ok(n) => {
                        buf.extend_from_slice(&tmp[..n]);
                        if buf.len() > 4096 {
                            break;
                        }
                    }
                    Err(ref e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            || e.kind() == std::io::ErrorKind::TimedOut =>
                    {
                        break;
                    }
                    Err(_) => break,
                }
                if start.elapsed() > Duration::from_millis(250) {
                    break;
                }
            }

            let req = String::from_utf8_lossy(&buf).to_string();
            let req_lc = req.to_ascii_lowercase();

            assert!(
                req.contains("POST /signal"),
                "expected POST /signal line, got:\n{}",
                req
            );
            assert!(
                req.contains("Authorization: Bearer t"),
                "expected Authorization header, got:\n{}",
                req
            );
            assert!(
                req_lc.contains("x-aifo-proto: 2"),
                "expected X-Aifo-Proto: 2 header, got:\n{}",
                req
            );
            assert!(
                req.contains("exec_id=e1"),
                "expected exec_id in body, got:\n{}",
                req
            );
            assert!(
                req.contains("signal=INT"),
                "expected signal=INT in body, got:\n{}",
                req
            );

            // Minimal 204 response
            let resp = b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
            let _ = s.write_all(resp);
        });

        // Call native signal sender against unix:// socket
        let url = format!("unix://{}", sock_path.display());
        send_signal_native(&url, "t", "e1", "INT");
        let _ = handle.join();
    }
}
