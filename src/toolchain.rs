/*!
Toolchain orchestration module (v7: Phases 2–5, 8).

This module owns the toolchain sidecars, proxy, shims and notification helpers.
The crate root re-exports these symbols with `pub use toolchain::*;`.
*/

use std::collections::HashMap;
use std::env as std_env;
use std::fs;
use std::io;
use std::io::{Read, Write};
use std::net::TcpListener;
#[cfg(target_os = "linux")]
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime};

#[cfg(unix)]
use nix::unistd::{getgid, getuid};

use crate::{
    container_runtime_path, create_session_id, find_header_end, shell_join, shell_like_split_args,
    strip_outer_quotes, url_decode,
};

mod images;
pub use images::{
    default_toolchain_image, default_toolchain_image_for_version, is_official_rust_image,
    normalize_toolchain_kind, official_rust_image_for_version,
};

mod routing;
pub use routing::{
    container_exists, route_tool_to_sidecar, select_kind_for_tool, sidecar_allowlist,
};

mod env;
mod mounts;

mod sidecar;
pub use sidecar::{
    build_sidecar_exec_preview, build_sidecar_run_preview, toolchain_bootstrap_typescript_global,
    toolchain_cleanup_session, toolchain_purge_caches, toolchain_run, toolchain_start_session,
};

mod proxy;
pub use proxy::toolexec_start_proxy;

mod shim;
pub use shim::toolchain_write_shims;

/// Proxy/cargo-related environment variables to pass through to sidecars.
const PROXY_ENV_NAMES: &[&str] = &[
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "NO_PROXY",
    "http_proxy",
    "https_proxy",
    "no_proxy",
    "CARGO_NET_GIT_FETCH_WITH_CLI",
    "CARGO_REGISTRIES_CRATES_IO_PROTOCOL",
];

/// Return true when an Authorization header value authorizes the given token
/// using the standard Bearer scheme (RFC 6750).
/// Accepts:
/// - "Bearer <token>" (scheme case-insensitive; at least one ASCII whitespace
///   separating scheme and credentials)
fn authorization_value_matches(value: &str, token: &str) -> bool {
    let v = value.trim();
    // Split at the first ASCII whitespace to separate scheme and credentials
    if let Some(idx) = v.find(|c: char| c.is_ascii_whitespace()) {
        let (scheme, rest) = v.split_at(idx);
        if scheme.eq_ignore_ascii_case("bearer") {
            let cred = rest.trim();
            return !cred.is_empty() && cred == token;
        }
    }
    false
}

fn random_token() -> String {
    // Prefer strong randomness on Unix via /dev/urandom
    #[cfg(target_family = "unix")]
    {
        use std::io::Read;
        if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
            let mut buf = [0u8; 16];
            if f.read_exact(&mut buf).is_ok() {
                let mut s = String::with_capacity(buf.len() * 2);
                for b in buf {
                    use std::fmt::Write as _;
                    let _ = write!(&mut s, "{:02x}", b);
                }
                return s;
            }
        }
    }
    // Prefer strong randomness on Windows via PowerShell .NET RNG
    #[cfg(target_os = "windows")]
    {
        let script = "[byte[]]$b=New-Object byte[] 16; [System.Security.Cryptography.RandomNumberGenerator]::Create().GetBytes($b); [System.BitConverter]::ToString($b).Replace('-', '').ToLower()";
        if let Ok(out) = Command::new("powershell.exe")
            .args(&["-NoProfile", "-NonInteractive", "-Command", script])
            .output()
        {
            if out.status.success() {
                let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if s.len() >= 32 {
                    return s;
                }
            }
        }
        // Try pwsh as an alternative
        if let Ok(out) = Command::new("pwsh")
            .args(&["-NoProfile", "-NonInteractive", "-Command", script])
            .output()
        {
            if out.status.success() {
                let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if s.len() >= 32 {
                    return s;
                }
            }
        }
    }
    // Fallback: time^pid base36 (sufficient for short-lived local dev)
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_nanos();
    let pid = std::process::id() as u128;
    let v = now ^ pid;
    let alphabet = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut n = v;
    let mut s = String::new();
    if n == 0 {
        s.push('0');
    } else {
        while n > 0 {
            s.push(alphabet[(n % 36) as usize] as char);
            n /= 36;
        }
    }
    s.chars().rev().collect()
}

/// Parse minimal application/x-www-form-urlencoded body; supports repeated keys.
pub fn parse_form_urlencoded(body: &str) -> Vec<(String, String)> {
    let mut res = Vec::new();
    for pair in body.split('&') {
        if pair.is_empty() {
            continue;
        }
        let mut it = pair.splitn(2, '=');
        let k = it.next().unwrap_or_default();
        let v = it.next().unwrap_or_default();
        res.push((url_decode(k), url_decode(v)));
    }
    res
}

/// Parse ~/.aider.conf.yml and extract notifications-command as argv tokens.
pub fn parse_notifications_command_config() -> Result<Vec<String>, String> {
    // Allow tests (and power users) to override config path explicitly
    let path = if let Ok(p) = std_env::var("AIFO_NOTIFICATIONS_CONFIG") {
        let p = p.trim().to_string();
        if !p.is_empty() {
            PathBuf::from(p)
        } else {
            home::home_dir()
                .ok_or_else(|| "home directory not found".to_string())?
                .join(".aider.conf.yml")
        }
    } else {
        home::home_dir()
            .ok_or_else(|| "home directory not found".to_string())?
            .join(".aider.conf.yml")
    };
    let content =
        fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {}", path.display(), e))?;

    // Pre-split lines to allow simple multi-line parsing
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0usize;
    while i < lines.len() {
        let line = lines[i];
        let l = line.trim_start();
        if l.starts_with('#') || l.is_empty() {
            i += 1;
            continue;
        }
        if let Some(rest) = l.strip_prefix("notifications-command:") {
            let mut val = rest.trim().to_string();
            // Tolerate configs/tests that append a literal "\n" at end of line
            if val.ends_with("\\n") {
                val.truncate(val.len() - 2);
            }

            // Helper: parse inline JSON/YAML-like array ["say","--title","AIFO"]
            let parse_inline_array = |val: &str| -> Result<Vec<String>, String> {
                let inner = &val[1..val.len() - 1];
                let mut argv: Vec<String> = Vec::new();
                let mut cur = String::new();
                let mut in_single = false;
                let mut in_double = false;
                let mut esc = false;
                for ch in inner.chars() {
                    if esc {
                        let c = match ch {
                            'n' => '\n',
                            'r' => '\r',
                            't' => '\t',
                            other => other,
                        };
                        cur.push(c);
                        esc = false;
                        continue;
                    }
                    match ch {
                        '\\' if in_double || in_single => esc = true,
                        '"' if !in_single => {
                            if in_double {
                                in_double = false;
                                argv.push(cur.clone());
                                cur.clear();
                            } else {
                                in_double = true;
                            }
                        }
                        '\'' if !in_double => {
                            if in_single {
                                in_single = false;
                                argv.push(cur.clone());
                                cur.clear();
                            } else {
                                in_single = true;
                            }
                        }
                        ',' if !in_single && !in_double => { /* separator */ }
                        c => {
                            if in_single || in_double {
                                cur.push(c);
                            }
                        }
                    }
                }
                if !cur.is_empty() && !in_single && !in_double {
                    argv.push(cur);
                }
                if argv.is_empty() {
                    Err("notifications-command parsed to an empty command".to_string())
                } else {
                    Ok(argv)
                }
            };

            // Case 1: inline array
            if val.starts_with('[') && val.ends_with(']') {
                return parse_inline_array(&val);
            }

            // Case 2: explicit block scalars '|' or '>'
            if val == "|" || val == ">" || val.is_empty() {
                // Collect subsequent indented lines; also support YAML list items beginning with '-'
                let mut j = i + 1;
                // Skip blank/comment lines until first candidate
                while j < lines.len()
                    && (lines[j].trim().is_empty() || lines[j].trim_start().starts_with('#'))
                {
                    j += 1;
                }
                if j >= lines.len() {
                    return Err("notifications-command is empty or malformed".to_string());
                }
                let first = lines[j];
                let is_list = first.trim_start().starts_with('-');
                if is_list {
                    let mut argv: Vec<String> = Vec::new();
                    while j < lines.len() {
                        let ln = lines[j];
                        let t = ln.trim_start();
                        if !t.starts_with('-') {
                            break;
                        }
                        let item = t.trim_start_matches('-').trim();
                        if !item.is_empty() {
                            argv.push(strip_outer_quotes(item));
                        }
                        j += 1;
                    }
                    if argv.is_empty() {
                        return Err("notifications-command list is empty".to_string());
                    }
                    return Ok(argv);
                } else {
                    // Block scalar: concatenate trimmed lines with spaces into a single command string
                    let mut parts: Vec<String> = Vec::new();
                    while j < lines.len() {
                        let ln = lines[j];
                        let t = ln.trim_start();
                        if t.is_empty() || t.starts_with('#') {
                            j += 1;
                            continue;
                        }
                        // Stop if de-indented to column 0 and looks like a new key
                        if !ln.starts_with(' ') && t.contains(':') {
                            break;
                        }
                        parts.push(t.to_string());
                        j += 1;
                    }
                    let joined = parts.join(" ");
                    let argv = shell_like_split_args(&strip_outer_quotes(&joined));
                    if argv.is_empty() {
                        return Err("notifications-command parsed to an empty command".to_string());
                    }
                    return Ok(argv);
                }
            }

            // Case 3: single-line scalar
            let unquoted = strip_outer_quotes(&val);
            let argv = shell_like_split_args(&unquoted);
            if argv.is_empty() {
                return Err("notifications-command parsed to an empty command".to_string());
            }
            return Ok(argv);
        }
        i += 1;
    }
    Err("notifications-command not found in ~/.aider.conf.yml".to_string())
}

/// Validate and, if allowed, execute the host 'say' command with provided args.
/// Returns (exit_code, output_bytes) on success, or Err(reason) if rejected.
pub fn notifications_handle_request(
    argv: &[String],
    _verbose: bool,
    timeout_secs: u64,
) -> Result<(i32, Vec<u8>), String> {
    let cfg_argv = parse_notifications_command_config()?;
    if cfg_argv.is_empty() {
        return Err("notifications-command is empty".to_string());
    }
    if cfg_argv[0] != "say" {
        return Err("only 'say' is allowed as notifications-command executable".to_string());
    }
    let cfg_args = &cfg_argv[1..];
    if cfg_args != argv {
        return Err(format!(
            "arguments mismatch: configured {:?} vs requested {:?}",
            cfg_args, argv
        ));
    }

    // Execute 'say' on the host with a timeout.
    let (tx, rx) = std::sync::mpsc::channel();
    let args_vec: Vec<String> = argv.to_vec();
    std::thread::spawn(move || {
        let mut cmd = Command::new("say");
        for a in &args_vec {
            cmd.arg(a);
        }
        let out = cmd.output();
        let _ = tx.send(out);
    });
    match rx.recv_timeout(std::time::Duration::from_secs(timeout_secs)) {
        Ok(Ok(o)) => {
            let mut b = o.stdout;
            if !o.stderr.is_empty() {
                b.extend_from_slice(&o.stderr);
            }
            Ok((o.status.code().unwrap_or(1), b))
        }
        Ok(Err(e)) => Err(format!("failed to execute host 'say': {}", e)),
        Err(_timeout) => Err("host 'say' execution timed out".to_string()),
    }
}

const ERR_UNAUTHORIZED: &[u8] = b"unauthorized\n";
const ERR_FORBIDDEN: &[u8] = b"forbidden\n";
const ERR_BAD_REQUEST: &[u8] = b"bad request\n";
const ERR_METHOD_NOT_ALLOWED: &[u8] = b"method not allowed\n";
const ERR_NOT_FOUND: &[u8] = b"not found\n";
const ERR_UNSUPPORTED_PROTO: &[u8] = b"Unsupported shim protocol; expected 1 or 2\n";

/// Response helpers (common).
fn respond_plain<W: Write>(w: &mut W, status: &str, exit_code: i32, body: &[u8]) {
    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/plain; charset=utf-8\r\nX-Exit-Code: {exit_code}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = w.write_all(header.as_bytes());
    let _ = w.write_all(body);
    let _ = w.flush();
}

fn respond_chunked_prelude<W: Write>(w: &mut W) {
    let hdr = b"HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nTransfer-Encoding: chunked\r\nTrailer: X-Exit-Code\r\nConnection: close\r\n\r\n";
    let _ = w.write_all(hdr);
    let _ = w.flush();
}

fn respond_chunked_write_chunk<W: Write>(w: &mut W, chunk: &[u8]) {
    if !chunk.is_empty() {
        let _ = write!(w, "{:X}\r\n", chunk.len());
        let _ = w.write_all(chunk);
        let _ = w.write_all(b"\r\n");
        let _ = w.flush();
    }
}

fn respond_chunked_trailer<W: Write>(w: &mut W, code: i32) {
    let _ = w.write_all(b"0\r\n");
    let trailer = format!("X-Exit-Code: {code}\r\n\r\n");
    let _ = w.write_all(trailer.as_bytes());
    let _ = w.flush();
}

/// Build streaming docker exec spawn args: add -t and wrap with sh -c "... 2>&1".
fn build_streaming_exec_args(container_name: &str, exec_preview_args: &[String]) -> Vec<String> {
    let mut spawn_args: Vec<String> = Vec::new();
    let mut idx = None;
    for (i, a) in exec_preview_args.iter().enumerate().skip(1) {
        if a == container_name {
            idx = Some(i);
            break;
        }
    }
    let idx = idx.unwrap_or(exec_preview_args.len().saturating_sub(1));
    // Up to and including container name
    spawn_args.extend(exec_preview_args[1..=idx].iter().cloned());
    // Allocate a TTY for streaming to improve interactive flushing.
    // Set AIFO_TOOLEEXEC_TTY=0 to disable TTY allocation if it interferes with tooling.
    let use_tty = std_env::var("AIFO_TOOLEEXEC_TTY").ok().as_deref() != Some("0");
    if use_tty {
        spawn_args.insert(1, "-t".to_string());
    }
    // User command slice after container name
    let user_slice: Vec<String> = exec_preview_args[idx + 1..].to_vec();
    let script = {
        let s = shell_join(&user_slice);
        format!("{} 2>&1", s)
    };
    spawn_args.push("sh".to_string());
    spawn_args.push("-c".to_string());
    spawn_args.push(script);
    spawn_args
}

fn is_tool_allowed_any_sidecar(tool: &str) -> bool {
    let tl = tool.to_ascii_lowercase();
    ["rust", "node", "python", "c-cpp", "go"]
        .iter()
        .any(|k| sidecar_allowlist(k).contains(&tl.as_str()))
}

struct ProxyCtx {
    runtime: PathBuf,
    token: String,
    session: String,
    timeout_secs: u64,
    verbose: bool,
    uidgid: Option<(u32, u32)>,
}

/// Start a minimal proxy to execute tools via shims inside sidecars.
/// Returns (url, token, running_flag, thread_handle).
pub(crate) fn toolexec_start_proxy_impl(
    session_id: &str,
    verbose: bool,
) -> io::Result<(
    String,
    String,
    std::sync::Arc<std::sync::atomic::AtomicBool>,
    std::thread::JoinHandle<()>,
)> {
    let runtime = container_runtime_path()?;

    #[cfg(unix)]
    let uid: u32 = u32::from(getuid());
    #[cfg(unix)]
    let gid: u32 = u32::from(getgid());
    #[cfg(not(unix))]
    let (uid, gid) = (0u32, 0u32);

    // Prepare shared proxy state (token, timeout, running flag, session id)
    let token = random_token();
    let token_for_thread = token.clone();
    // Per-request timeout (seconds); default 60
    let timeout_secs: u64 = std_env::var("AIFO_TOOLEEXEC_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|&v| v > 0)
        .unwrap_or(60);
    let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let session = session_id.to_string();

    // Optional unix socket transport on Linux, gated by AIFO_TOOLEEXEC_USE_UNIX=1
    let use_unix = cfg!(target_os = "linux")
        && std_env::var("AIFO_TOOLEEXEC_USE_UNIX").ok().as_deref() == Some("1");
    if use_unix {
        #[cfg(target_os = "linux")]
        {
            // Create host socket directory and bind UnixListener
            let base = "/run/aifo";
            let _ = fs::create_dir_all(base);
            let host_dir = format!("{}/aifo-{}", base, session);
            let _ = fs::create_dir_all(&host_dir);
            let sock_path = format!("{}/toolexec.sock", host_dir);
            let _ = fs::remove_file(&sock_path);
            let listener = UnixListener::bind(&sock_path)
                .map_err(|e| io::Error::new(e.kind(), format!("proxy unix bind failed: {e}")))?;
            let _ = listener.set_nonblocking(true);
            // Expose directory for agent mount
            std_env::set_var("AIFO_TOOLEEXEC_UNIX_DIR", &host_dir);
            let running_cl2 = running.clone();
            let token_for_thread2 = token_for_thread.clone();
            let handle = std::thread::spawn(move || {
                if verbose {
                    eprintln!("aifo-coder: toolexec proxy listening on unix socket");
                }
                let mut tool_cache: HashMap<(String, String), bool> = HashMap::new();
                loop {
                    if !running_cl2.load(std::sync::atomic::Ordering::SeqCst) {
                        break;
                    }
                    let (mut stream, _addr) = match listener.accept() {
                        Ok(pair) => pair,
                        Err(e) => {
                            if e.kind() == io::ErrorKind::WouldBlock {
                                std::thread::sleep(Duration::from_millis(50));
                                continue;
                            } else {
                                continue;
                            }
                        }
                    };
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(timeout_secs)));
                    let _ = stream.set_write_timeout(None);

                    let ctx = ProxyCtx {
                        runtime: runtime.clone(),
                        token: token_for_thread2.clone(),
                        session: session.clone(),
                        timeout_secs,
                        verbose,
                        uidgid: if cfg!(unix) { Some((uid, gid)) } else { None },
                    };
                    handle_connection(&ctx, &mut stream, &mut tool_cache);
                }
                if verbose {
                    eprintln!("aifo-coder: toolexec proxy stopped");
                }
            });
            let url = format!("unix://{}/toolexec.sock", host_dir);
            return Ok((url, token, running, handle));
        }
    }
    // Bind address by OS: 0.0.0.0 on Linux (containers connect), 127.0.0.1 on macOS/Windows
    let bind_host: &str = if cfg!(target_os = "linux") {
        "0.0.0.0"
    } else {
        "127.0.0.1"
    };
    let listener = TcpListener::bind((bind_host, 0))
        .map_err(|e| io::Error::new(e.kind(), format!("proxy bind failed: {e}")))?;
    let addr = listener
        .local_addr()
        .map_err(|e| io::Error::new(e.kind(), format!("proxy addr failed: {e}")))?;
    let port = addr.port();
    let _ = listener.set_nonblocking(true);
    let running_cl = running.clone();

    let handle = std::thread::spawn(move || {
        if verbose {
            eprintln!(
                "aifo-coder: toolexec proxy listening on {}:{port}",
                bind_host
            );
        }
        let mut tool_cache: HashMap<(String, String), bool> = HashMap::new();
        loop {
            if !running_cl.load(std::sync::atomic::Ordering::SeqCst) {
                break;
            }
            let (mut stream, _addr) = match listener.accept() {
                Ok(pair) => pair,
                Err(e) => {
                    if e.kind() == io::ErrorKind::WouldBlock {
                        std::thread::sleep(Duration::from_millis(50));
                        continue;
                    } else {
                        continue;
                    }
                }
            };
            let _ = stream.set_read_timeout(Some(Duration::from_secs(timeout_secs)));
            let _ = stream.set_write_timeout(None);

            let ctx = ProxyCtx {
                runtime: runtime.clone(),
                token: token_for_thread.clone(),
                session: session.clone(),
                timeout_secs,
                verbose,
                uidgid: if cfg!(unix) { Some((uid, gid)) } else { None },
            };
            handle_connection(&ctx, &mut stream, &mut tool_cache);
        }
        if verbose {
            eprintln!("aifo-coder: toolexec proxy stopped");
        }
    });
    // On macOS/Windows, host.docker.internal resolves; on Linux we add host-gateway and still use host.docker.internal
    let url = format!("http://host.docker.internal:{}/exec", port);
    Ok((url, token, running, handle))
}

fn parse_request_line_and_query(header_str: &str) -> (String, String, Vec<(String, String)>) {
    let mut method_up = String::new();
    let mut request_path_lc = String::new();
    let mut query_pairs: Vec<(String, String)> = Vec::new();
    if let Some(first_line) = header_str.lines().next() {
        let mut parts = first_line.split_whitespace();
        if let Some(m) = parts.next() {
            method_up = m.to_ascii_uppercase();
        }
        if let Some(target) = parts.next() {
            let path_only = target.split('?').next().unwrap_or(target);
            request_path_lc = path_only.to_ascii_lowercase();
            if let Some(idx) = target.find('?') {
                let q = &target[idx + 1..];
                query_pairs.extend(parse_form_urlencoded(q));
            }
        }
    }
    (method_up, request_path_lc, query_pairs)
}

/// Handle a single proxy connection: parse request, route, exec, and respond.
fn handle_connection<S: Read + Write>(
    ctx: &ProxyCtx,
    stream: &mut S,
    tool_cache: &mut HashMap<(String, String), bool>,
) {
    let runtime: &Path = &ctx.runtime;
    let token: &str = &ctx.token;
    let session: &str = &ctx.session;
    let timeout_secs: u64 = ctx.timeout_secs;
    let verbose: bool = ctx.verbose;
    let uidgid = ctx.uidgid;
    // Read request (simple HTTP)
    let mut buf = Vec::new();
    let mut hdr = Vec::new();
    let mut tmp = [0u8; 1024];
    // Read until end of headers
    let mut header_end = None;
    while header_end.is_none() {
        match stream.read(&mut tmp) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&tmp[..n]);
                if let Some(end) = find_header_end(&buf) {
                    header_end = Some(end);
                } else if let Some(pos) = buf.windows(2).position(|w| w == b"\n\n") {
                    // Be tolerant to LF-only header termination used by some simple clients/tests
                    header_end = Some(pos);
                }
                // avoid overly large header
                if buf.len() > 64 * 1024 {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    let hend = if let Some(h) = header_end {
        h
    } else if !buf.is_empty() {
        // Tolerate missing CRLFCRLF for simple clients: treat entire buffer as headers
        buf.len()
    } else {
        let body = b"unauthorized\n";
        respond_plain(stream, "401 Unauthorized", 86, body);
        let _ = stream.flush();
        return;
    };
    hdr.extend_from_slice(&buf[..hend]);
    let header_str = String::from_utf8_lossy(&hdr);
    let mut auth_ok = false;
    let mut content_len: usize = 0;
    let mut proto_ok = false;
    let mut proto_present = false;
    let mut proto_ver: u8 = 0;
    let mut saw_auth = false;
    for line in header_str.lines() {
        let l = line.trim();
        let lower = l.to_ascii_lowercase();
        if lower.starts_with("authorization:") {
            saw_auth = true;
            if let Some((_, v)) = l.split_once(':') {
                if authorization_value_matches(v, token) {
                    auth_ok = true;
                }
            }
        } else if lower.starts_with("content-length:") {
            if let Some((_, v)) = l.split_once(':') {
                content_len = v.trim().parse().unwrap_or(0);
            }
        } else if lower.starts_with("x-aifo-proto:") {
            if let Some((_, v)) = l.split_once(':') {
                proto_present = true;
                let vt = v.trim();
                if vt == "1" || vt == "2" {
                    proto_ok = true;
                    proto_ver = if vt == "2" { 2 } else { 1 };
                }
            }
        }
    }
    if verbose {
        eprintln!(
            "\r\x1b[2Kaifo-coder: proxy headers: auth_seen={} auth_ok={} proto_v={}",
            saw_auth,
            auth_ok,
            if proto_present { proto_ver as i32 } else { 0 }
        );
    }
    // Extract query parameters and validate method/target early
    let (method_up, request_path_lc, query_pairs) = parse_request_line_and_query(&header_str);
    // Tighten: Only allow POST to /exec for normal exec requests; notifications paths are exempt.
    let is_notifications_path_hint = request_path_lc.contains("/notifications")
        || request_path_lc.contains("/notifications-cmd")
        || request_path_lc.contains("/notify");
    if !is_notifications_path_hint {
        if method_up != "POST" {
            respond_plain(stream, "405 Method Not Allowed", 86, ERR_METHOD_NOT_ALLOWED);
            let _ = stream.flush();
            return;
        }
        if !request_path_lc.ends_with("/exec") {
            respond_plain(stream, "404 Not Found", 86, ERR_NOT_FOUND);
            let _ = stream.flush();
            return;
        }
    }
    // Read body (skip header terminator if present)
    let mut body_start = hend;
    if buf.len() >= hend + 4 && &buf[hend..hend + 4] == b"\r\n\r\n" {
        body_start = hend + 4;
    } else if buf.len() >= hend + 2 && &buf[hend..hend + 2] == b"\n\n" {
        body_start = hend + 2;
    }
    let mut body = buf[body_start..].to_vec();
    while body.len() < content_len {
        match stream.read(&mut tmp) {
            Ok(0) => break,
            Ok(n) => body.extend_from_slice(&tmp[..n]),
            Err(_) => break,
        }
    }
    let form = String::from_utf8_lossy(&body).to_string();
    let mut tool = String::new();
    let mut cwd = "/workspace".to_string();
    let mut argv: Vec<String> = Vec::new();
    for (k, v) in query_pairs
        .into_iter()
        .chain(parse_form_urlencoded(&form).into_iter())
    {
        let kl = k.to_ascii_lowercase();
        match kl.as_str() {
            "tool" => tool = v,
            "cwd" => cwd = v,
            "arg" => argv.push(v),
            _ => {}
        }
    }
    if tool.is_empty() {
        let rp = request_path_lc.as_str();
        if rp.ends_with("/notifications")
            || rp.ends_with("/notifications-cmd")
            || rp.ends_with("/notify")
            || rp.contains("/notifications")
            || rp.contains("/notifications-cmd")
            || rp.contains("/notify")
        {
            tool = "notifications-cmd".to_string();
        }
    }
    // Fallback: if tool is still empty, attempt to parse from Request-Target query (?tool=...)
    // This helps when clients don't send a body or Content-Length is missing.
    if tool.is_empty() {
        if let Some(first_line) = header_str.lines().next() {
            if let Some(idx) = first_line.find("?tool=") {
                let rest = &first_line[idx + 6..];
                let end = rest
                    .find(|c: char| c == '&' || c.is_ascii_whitespace() || c == '\r')
                    .unwrap_or(rest.len());
                let val = &rest[..end];
                tool = url_decode(val);
            }
        }
    }
    // Notifications endpoint: allow Authorization-bypass with strict exact-args guard.
    // If Authorization is valid, still require protocol header (1 or 2).
    if (tool.eq_ignore_ascii_case("notifications-cmd")
        || form.contains("tool=notifications-cmd")
        || request_path_lc.contains("/notifications")
        || request_path_lc.contains("/notifications-cmd")
        || request_path_lc.contains("/notify"))
        && auth_ok
    {
        if !proto_present || !proto_ok {
            respond_plain(stream, "426 Upgrade Required", 86, ERR_UNSUPPORTED_PROTO);
            let _ = stream.flush();
            return;
        }
        match notifications_handle_request(&argv, verbose, timeout_secs) {
            Ok((status_code, body_out)) => {
                let header = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nX-Exit-Code: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    status_code,
                    body_out.len()
                );
                let _ = stream.write_all(header.as_bytes());
                let _ = stream.write_all(&body_out);
                let _ = stream.flush();
                return;
            }
            Err(reason) => {
                let mut body = reason.into_bytes();
                body.push(b'\n');
                respond_plain(stream, "403 Forbidden", 86, &body);
                let _ = stream.flush();
                return;
            }
        }
    }
    // If not authorized, fall through to the no-auth bypass block below.
    // Fast-path: if tool provided and not permitted by any sidecar allowlist, reject early
    if !tool.is_empty() && !is_tool_allowed_any_sidecar(&tool) {
        respond_plain(stream, "403 Forbidden", 86, ERR_FORBIDDEN);
        let _ = stream.flush();
        return;
    }
    if tool.is_empty() {
        // If Authorization is valid, require protocol header X-Aifo-Proto: 1 (426 on missing or wrong). Otherwise, 401 for missing/invalid auth; else 400 for malformed body
        if auth_ok && (!proto_present || !proto_ok) {
            respond_plain(stream, "426 Upgrade Required", 86, ERR_UNSUPPORTED_PROTO);
            let _ = stream.flush();
            return;
        } else if !auth_ok {
            respond_plain(stream, "401 Unauthorized", 86, ERR_UNAUTHORIZED);
            let _ = stream.flush();
            return;
        } else {
            respond_plain(stream, "400 Bad Request", 86, ERR_BAD_REQUEST);
            let _ = stream.flush();
            return;
        }
    }
    // Secondary notifications block for no-auth bypass (special case)
    if !auth_ok
        && (tool.eq_ignore_ascii_case("notifications-cmd")
            || form.contains("tool=notifications-cmd")
            || request_path_lc.contains("/notifications")
            || request_path_lc.contains("/notifications-cmd")
            || request_path_lc.contains("/notify"))
    {
        match notifications_handle_request(&argv, verbose, timeout_secs) {
            Ok((status_code, body_out)) => {
                let header = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nX-Exit-Code: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    status_code,
                    body_out.len()
                );
                let _ = stream.write_all(header.as_bytes());
                let _ = stream.write_all(&body_out);
                let _ = stream.flush();
                return;
            }
            Err(reason) => {
                let mut body = reason.into_bytes();
                body.push(b'\n');
                respond_plain(stream, "403 Forbidden", 86, &body);
                let _ = stream.flush();
                return;
            }
        }
    }
    let selected_kind = select_kind_for_tool(session, &tool, timeout_secs, tool_cache);
    let kind = selected_kind.as_str();
    let allow = sidecar_allowlist(kind);
    if !allow.contains(&tool.as_str()) {
        respond_plain(stream, "403 Forbidden", 86, ERR_FORBIDDEN);
        let _ = stream.flush();
        return;
    }
    // When Authorization is valid, require X-Aifo-Proto: 1 (426 on missing or wrong). Otherwise, 401 when missing/invalid auth.
    if auth_ok && (!proto_present || !proto_ok) {
        respond_plain(stream, "426 Upgrade Required", 86, ERR_UNSUPPORTED_PROTO);
        let _ = stream.flush();
        return;
    }
    if !auth_ok {
        respond_plain(stream, "401 Unauthorized", 86, ERR_UNAUTHORIZED);
        let _ = stream.flush();
        return;
    }

    let name = sidecar::sidecar_container_name(kind, session);
    // If selected sidecar isn't running and no alternative was available, return a helpful error
    if !container_exists(&name) {
        let msg = format!(
            "tool '{}' not available in running sidecars; start an appropriate toolchain (e.g., --toolchain c-cpp or --toolchain rust)\n",
            tool
        );
        respond_plain(stream, "409 Conflict", 86, msg.as_bytes());
        let _ = stream.flush();
        return;
    }

    let pwd = PathBuf::from(cwd);
    if verbose {
        let _ = std::io::stdout().flush();
        let _ = std::io::stderr().flush();
        eprintln!(
            "\r\x1b[2Kaifo-coder: proxy exec: tool={} args={:?} cwd={}",
            tool,
            argv,
            pwd.display()
        );
    }

    let mut full_args: Vec<String>;
    if tool == "tsc" {
        let nm_tsc = pwd.join("node_modules").join(".bin").join("tsc");
        if nm_tsc.exists() {
            full_args = vec!["./node_modules/.bin/tsc".to_string()];
            full_args.extend(argv.clone());
            if verbose {
                let _ = std::io::stdout().flush();
                let _ = std::io::stderr().flush();
                eprintln!("\r\x1b[2Kaifo-coder: proxy exec: tsc via local node_modules");
                eprintln!("\r");
            }
        } else {
            full_args = vec!["npx".to_string(), "tsc".to_string()];
            full_args.extend(argv.clone());
            if verbose {
                let _ = std::io::stdout().flush();
                let _ = std::io::stderr().flush();
                eprintln!("\r\x1b[2Kaifo-coder: proxy exec: tsc via npx");
                eprintln!("\r");
            }
        }
    } else {
        full_args = vec![tool.clone()];
        full_args.extend(argv.clone());
    }

    let exec_preview_args = build_sidecar_exec_preview(
        &name,
        if cfg!(unix) { uidgid } else { None },
        &pwd,
        kind,
        &full_args,
    );

    if verbose {
        let _ = std::io::stdout().flush();
        let _ = std::io::stderr().flush();
        eprintln!("\r\x1b[2Kaifo-coder: proxy docker:");
        eprintln!("\r\x1b[2K  {}", shell_join(&exec_preview_args));
    }

    // If client requested streaming (protocol v2), stream chunked output with exit code as trailer
    if proto_present && proto_ok && proto_ver == 2 {
        respond_chunked_prelude(stream);
        if verbose {
            let _ = std::io::stdout().flush();
            let _ = std::io::stderr().flush();
            eprintln!("\r\x1b[2Kaifo-coder: proxy exec: proto=v2 (streaming)");
        }
        eprintln!("\r");
        let started = std::time::Instant::now();

        let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
        let runtime_cl = runtime.to_path_buf();

        let spawn_args = build_streaming_exec_args(&name, &exec_preview_args);

        let mut cmd = Command::new(&runtime_cl);
        for a in &spawn_args {
            cmd.arg(a);
        }
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                respond_chunked_trailer(stream, 1);
                eprintln!("aifo-coder: proxy spawn error: {}", e);
                return;
            }
        };

        // Drain docker exec stderr to prevent pipe backpressure from stalling long outputs
        if let Some(mut se) = child.stderr.take() {
            std::thread::spawn(move || {
                let mut buf = [0u8; 8192];
                loop {
                    match se.read(&mut buf) {
                        Ok(0) => break,
                        Ok(_n) => {}
                        Err(_) => break,
                    }
                }
            });
        }

        if let Some(mut so) = child.stdout.take() {
            let txo = tx.clone();
            std::thread::spawn(move || {
                let mut buf = [0u8; 8192];
                loop {
                    match so.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            let _ = txo.send(buf[..n].to_vec());
                        }
                        Err(_) => break,
                    }
                }
            });
        }
        // stderr merged into stdout via '2>&1'; no separate reader

        // Drain chunks and forward to client with timeout support
        let (tox, tor) = std::sync::mpsc::channel::<()>();
        let timeout_secs_cl = timeout_secs;
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(timeout_secs_cl));
            let _ = tox.send(());
        });
        drop(tx);
        let mut timed_out = false;
        loop {
            // Check timeout signal
            if tor.try_recv().is_ok() {
                let _ = child.kill();
                timed_out = true;
                break;
            }
            match rx.recv_timeout(Duration::from_millis(200)) {
                Ok(chunk) => {
                    respond_chunked_write_chunk(stream, &chunk);
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    continue;
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }

        if timed_out {
            let _ = child.wait();
            respond_chunked_write_chunk(stream, b"aifo-coder proxy timeout\n");
            if verbose {
                let _ = std::io::stdout().flush();
                let _ = std::io::stderr().flush();
                eprintln!(
                    "\r\x1b[2Kaifo-coder: proxy result tool={} kind={} code={} dur_ms={}",
                    tool,
                    kind,
                    124,
                    started.elapsed().as_millis()
                );
                eprintln!("\r");
            }
            respond_chunked_trailer(stream, 124);
            return;
        }

        let code = child.wait().ok().and_then(|s| s.code()).unwrap_or(1);
        let dur_ms = started.elapsed().as_millis();
        eprintln!("\r");
        if verbose {
            let _ = std::io::stdout().flush();
            let _ = std::io::stderr().flush();
            eprintln!(
                "\r\x1b[2Kaifo-coder: proxy result tool={} kind={} code={} dur_ms={}",
                tool, kind, code, dur_ms
            );
            eprintln!("\r");
        }
        // Final chunk + trailer with exit code
        respond_chunked_trailer(stream, code);
        return;
    }

    if verbose {
        let _ = std::io::stdout().flush();
        let _ = std::io::stderr().flush();
        eprintln!("\r\x1b[2Kaifo-coder: proxy exec: proto=v1 (buffered)");
    }
    let started = std::time::Instant::now();
    let (status_code, mut body_bytes) = {
        let (tx, rx) = std::sync::mpsc::channel();
        let runtime_cl = runtime.to_path_buf();
        let args_clone: Vec<String> = exec_preview_args[1..].to_vec();
        std::thread::spawn(move || {
            let mut cmd = Command::new(&runtime_cl);
            for a in &args_clone {
                cmd.arg(a);
            }
            let out = cmd.output();
            let _ = tx.send(out);
        });
        match rx.recv_timeout(Duration::from_secs(timeout_secs)) {
            Ok(Ok(o)) => {
                let code = o.status.code().unwrap_or(1);
                let mut b = o.stdout;
                if !o.stderr.is_empty() {
                    b.extend_from_slice(&o.stderr);
                }
                (code, b)
            }
            Ok(Err(e)) => {
                let mut b = format!("aifo-coder proxy error: {}", e).into_bytes();
                b.push(b'\n');
                (1, b)
            }
            Err(_timeout) => {
                respond_plain(
                    stream,
                    "504 Gateway Timeout",
                    124,
                    b"aifo-coder proxy timeout\n",
                );
                let _ = stream.flush();
                return;
            }
        }
    };
    let dur_ms = started.elapsed().as_millis();
    eprintln!("\r");
    if verbose {
        let _ = std::io::stdout().flush();
        let _ = std::io::stderr().flush();
        eprintln!(
            "\r\x1b[2Kaifo-coder: proxy result tool={} kind={} code={} dur_ms={}",
            tool, kind, status_code, dur_ms
        );
        eprintln!("\r");
    }
    if verbose {
        if !body_bytes.starts_with(b"\n") && !body_bytes.starts_with(b"\r") {
            let mut pref = Vec::with_capacity(body_bytes.len() + 1);
            pref.push(b'\n');
            pref.extend_from_slice(&body_bytes);
            body_bytes = pref;
        }
        if !body_bytes.ends_with(b"\n") && !body_bytes.ends_with(b"\r") {
            body_bytes.push(b'\n');
        }
    }
    let header = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nX-Exit-Code: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        status_code,
        body_bytes.len()
    );
    let _ = stream.write_all(header.as_bytes());
    let _ = stream.write_all(&body_bytes);
    let _ = stream.flush();
}

#[cfg(test)]
mod auth_tests {
    use super::authorization_value_matches;

    #[test]
    fn auth_bearer_basic() {
        assert!(authorization_value_matches("Bearer tok", "tok"));
    }
    #[test]
    fn auth_bearer_case_whitespace() {
        assert!(authorization_value_matches("bearer    tok", "tok"));
        assert!(authorization_value_matches("BEARER tok", "tok"));
    }
    #[test]
    fn auth_bearer_punct_rejected() {
        assert!(!authorization_value_matches("Bearer \"tok\"", "tok"));
        assert!(!authorization_value_matches("Bearer tok,", "tok"));
        assert!(!authorization_value_matches("'Bearer tok';", "tok"));
    }
    #[test]
    fn auth_bare_token_rejected() {
        assert!(!authorization_value_matches("tok", "tok"));
    }
    #[test]
    fn auth_wrong() {
        assert!(!authorization_value_matches("Bearer nope", "tok"));
        assert!(!authorization_value_matches("Basic tok", "tok"));
        assert!(!authorization_value_matches("nearlytok", "tok"));
    }
}
