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

mod http;
mod auth;
mod notifications;

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

fn log_parsed_request(verbose: bool, tool: &str, argv: &[String], cwd: &str) {
    if verbose {
        let _ = std::io::stdout().flush();
        let _ = std::io::stderr().flush();
        eprintln!(
            "\r\x1b[2Kaifo-coder: proxy parsed: tool={} argv={:?} cwd={}",
            tool, argv, cwd
        );
        eprintln!("\r");
    }
}

fn log_request_result(
    verbose: bool,
    tool: &str,
    kind: &str,
    code: i32,
    started: &std::time::Instant,
) {
    if verbose {
        let _ = std::io::stdout().flush();
        let _ = std::io::stderr().flush();
        eprintln!(
            "\r\x1b[2Kaifo-coder: proxy result tool={} kind={} code={} dur_ms={}",
            tool,
            kind,
            code,
            started.elapsed().as_millis()
        );
        eprintln!("\r");
    }
}

/// Return true when an Authorization header value authorizes the given token
/// using the standard Bearer scheme (RFC 6750).
/// Accepts:
/// - "Bearer <token>" (scheme case-insensitive; at least one ASCII whitespace
///   separating scheme and credentials)

fn random_token() -> String {
    // Cross-platform secure RNG using getrandom
    let mut buf = [0u8; 16]; // 128-bit token
    match getrandom::getrandom(&mut buf) {
        Ok(_) => {
            let mut s = String::with_capacity(buf.len() * 2);
            for b in buf {
                use std::fmt::Write as _;
                let _ = write!(&mut s, "{:02x}", b);
            }
            s
        }
        Err(e) => {
            // Very rare fallback: deterministic-ish token with warning
            eprintln!(
                "aifo-coder: warning: secure RNG failed ({}); falling back to time^pid",
                e
            );
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
    }
}

/// Parse minimal application/x-www-form-urlencoded body; supports repeated keys.



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
                query_pairs.extend(http::parse_form_urlencoded(q));
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
                if auth::authorization_value_matches(v, token) {
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
    const BODY_CAP: usize = 1024 * 1024;
    if content_len > BODY_CAP {
        respond_plain(stream, "400 Bad Request", 86, ERR_BAD_REQUEST);
        let _ = stream.flush();
        return;
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
    // Use endpoint classification to gate method/path handling.
    let endpoint = http::classify_endpoint(&request_path_lc);
    if !matches!(endpoint, Some(http::Endpoint::Notifications)) {
        if method_up != "POST" {
            respond_plain(stream, "405 Method Not Allowed", 86, ERR_METHOD_NOT_ALLOWED);
            let _ = stream.flush();
            return;
        }
        if !matches!(endpoint, Some(http::Endpoint::Exec)) {
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
        .chain(http::parse_form_urlencoded(&form).into_iter())
    {
        let kl = k.to_ascii_lowercase();
        match kl.as_str() {
            "tool" => tool = v,
            "cwd" => cwd = v,
            "arg" => argv.push(v),
            _ => {}
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

    // Log the request
    log_parsed_request(verbose, &tool, &argv, &cwd);

    // Notifications endpoint: allow Authorization-bypass with strict exact-args guard.
    // If Authorization is valid, still require protocol header (1 or 2).
    if matches!(endpoint, Some(http::Endpoint::Notifications)) && auth_ok {
        if !proto_present || !proto_ok {
            respond_plain(stream, "426 Upgrade Required", 86, ERR_UNSUPPORTED_PROTO);
            let _ = stream.flush();
            return;
        }
        match notifications::notifications_handle_request(&argv, verbose, timeout_secs) {
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
        // If Authorization header is present but protocol is missing/invalid => 426 (align with tests expecting 426 on bad proto).
        // Otherwise, 401 for missing/invalid auth; else 400 for malformed body.
        if saw_auth && (!proto_present || !proto_ok) {
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
    if !auth_ok && matches!(endpoint, Some(http::Endpoint::Notifications)) {
        match notifications::notifications_handle_request(&argv, verbose, timeout_secs) {
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
                // Emit a small diagnostic chunk before the trailer to aid UX
                respond_chunked_write_chunk(stream, b"aifo-coder proxy spawn error\n");
                // Log the failure for observability
                log_request_result(verbose, &tool, kind, 1, &started);
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

            log_request_result(verbose, &tool, kind, 124, &started);
            respond_chunked_trailer(stream, 124);
            return;
        }

        let code = child.wait().ok().and_then(|s| s.code()).unwrap_or(1);
        eprintln!("\r");
        log_request_result(verbose, &tool, kind, code, &started);

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
                // Log timeout consistently with streaming path
                log_request_result(verbose, &tool, kind, 124, &started);
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
    eprintln!("\r");
    log_request_result(verbose, &tool, kind, status_code, &started);

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

