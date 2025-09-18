/*!
Proxy module: dispatcher, accept loop, and public toolexec_start_proxy API.

Implements v3 signal propagation and timeout model:
- Listener setup (TCP/unix), accept loop with backoff.
- Per-connection dispatcher using http::read_http_request + http::classify_endpoint.
- Centralized auth/proto via auth::validate_auth_and_proto.
- /signal endpoint: authenticated signal forwarding by ExecId.
- ExecId registry and streaming prelude includes X-Exec-Id (v2).
- Setsid+PGID wrapper applied to v1 and v2 execs; PGID file at $HOME/.aifo-exec/<ExecId>/pgid.
- Disconnect-triggered termination for v2 (INT -> TERM -> KILL).
- No default proxy-imposed timeout for tool execs; optional max-runtime escalation (INT at T, TERM at T+5s, KILL at T+10s).
- Notifications policy per spec with independent short timeout.
- Streaming prelude only after successful spawn; plain 500 on spawn error.
*/
use std::collections::HashMap;
use std::env as std_env;
#[cfg(target_os = "linux")]
use std::fs;
use std::io;
use std::io::{Read, Write};
use std::net::TcpListener;
#[cfg(target_os = "linux")]
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

#[cfg(unix)]
use nix::unistd::{getgid, getuid};

use crate::container_runtime_path;
use crate::shell_join;

use super::sidecar;
use super::{auth, http, notifications};
use super::{container_exists, select_kind_for_tool, sidecar_allowlist};

use super::{
    log_parsed_request, log_request_result, random_token, ERR_BAD_REQUEST, ERR_FORBIDDEN,
    ERR_METHOD_NOT_ALLOWED, ERR_NOT_FOUND, ERR_UNAUTHORIZED, ERR_UNSUPPORTED_PROTO,
};

struct ProxyCtx {
    runtime: PathBuf,
    token: String,
    session: String,
    timeout_secs: u64,
    verbose: bool,
    agent_container: Option<String>,
    uidgid: Option<(u32, u32)>,
}

// Response helpers (moved from toolchain.rs)
fn respond_plain<W: Write>(w: &mut W, status: &str, exit_code: i32, body: &[u8]) {
    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/plain; charset=utf-8\r\nX-Exit-Code: {exit_code}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = w.write_all(header.as_bytes());
    let _ = w.write_all(body);
    let _ = w.flush();
}

fn respond_chunked_prelude<W: Write>(w: &mut W, exec_id: Option<&str>) {
    let mut hdr = String::from("HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nTransfer-Encoding: chunked\r\nTrailer: X-Exit-Code\r\nConnection: close\r\n");
    if let Some(id) = exec_id {
        hdr.push_str(&format!("X-Exec-Id: {}\r\n", id));
    }
    hdr.push_str("\r\n");
    let _ = w.write_all(hdr.as_bytes());
    let _ = w.flush();
}

fn respond_chunked_write_chunk<W: Write>(w: &mut W, chunk: &[u8]) -> io::Result<()> {
    if !chunk.is_empty() {
        write!(w, "{:X}\r\n", chunk.len())?;
        w.write_all(chunk)?;
        w.write_all(b"\r\n")?;
        w.flush()?;
    }
    Ok(())
}

fn respond_chunked_trailer<W: Write>(w: &mut W, code: i32) {
    let _ = w.write_all(b"0\r\n");
    let trailer = format!("X-Exit-Code: {code}\r\n\r\n");
    let _ = w.write_all(trailer.as_bytes());
    let _ = w.flush();
}

/// Test helper: tee important proxy log lines to stderr and optionally to a file
/// when AIFO_TEST_LOG_PATH is set (used by acceptance tests to avoid dup2 tricks).
fn log_stderr_and_file(s: &str) {
    eprintln!("{}", s);
    if let Ok(p) = std_env::var("AIFO_TEST_LOG_PATH") {
        if !p.trim().is_empty() {
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&p)
            {
                use std::io::Write as _;
                let _ = writeln!(f, "{}", s);
            }
        }
    }
}

/// Best-effort: send a signal to the process group inside container for given exec id.
fn kill_in_container(
    runtime: &PathBuf,
    container: &str,
    exec_id: &str,
    signal: &str,
    verbose: bool,
) {
    let sig = signal.to_ascii_uppercase();
    let script = format!(
        "pg=\"/home/coder/.aifo-exec/{id}/pgid\"; if [ -f \"$pg\" ]; then n=$(cat \"$pg\" 2>/dev/null); if [ -n \"$n\" ]; then kill -s {sig} -\"$n\" || true; fi; fi",
        id = exec_id,
        sig = sig
    );

    let args: Vec<String> = vec![
        "docker".into(),
        "exec".into(),
        container.into(),
        "sh".into(),
        "-lc".into(),
        script,
    ];
    if verbose {
        eprintln!("\raifo-coder: docker: {}", shell_join(&args));
    }
    // First attempt
    let mut cmd = Command::new(runtime);
    for a in &args[1..] {
        cmd.arg(a);
    }
    cmd.stdout(Stdio::null()).stderr(Stdio::null());
    let _ = cmd.status();
    // Brief retry to mitigate transient docker/kill issues
    std::thread::sleep(Duration::from_millis(100));
    let mut cmd2 = Command::new(runtime);
    for a in &args[1..] {
        cmd2.arg(a);
    }
    cmd2.stdout(Stdio::null()).stderr(Stdio::null());
    let _ = cmd2.status();
}

/// Best-effort: kill the interactive /run shell inside the agent container using recorded tpgid.
fn kill_agent_shell_in_agent_container(
    runtime: &PathBuf,
    agent_container: &str,
    exec_id: &str,
    verbose: bool,
) {
    // Read recorded terminal foreground PGID and signal that group within the agent container.
    let script = format!(
        "pp=\"/home/coder/.aifo-exec/{id}/agent_ppid\"; tp=\"/home/coder/.aifo-exec/{id}/agent_tpgid\"; tt=\"/home/coder/.aifo-exec/{id}/tty\"; \
         kill_shells_by_tty() {{ t=\"$1\"; [ -z \"$t\" ] && return; bn=\"$(basename \"$t\" 2>/dev/null)\"; \
           if command -v ps >/dev/null 2>&1; then \
             ps -eo pid=,tty=,comm= | awk -v T=\"$bn\" '($2==T){{print $1\" \"$3}}' | while read -r pid comm; do \
               case \"$comm\" in sh|bash|dash|zsh|ksh|ash|busybox|busybox-sh) \
                 kill -HUP \"$pid\" >/dev/null 2>&1 || true; sleep 0.1; \
                 kill -TERM \"$pid\" >/dev/null 2>&1 || true; sleep 0.3; \
                 kill -KILL \"$pid\" >/dev/null 2>&1 || true; \
               ;; esac; \
             done; \
           fi; \
         }}; \
         if [ -f \"$pp\" ]; then p=$(cat \"$pp\" 2>/dev/null); if [ -n \"$p\" ]; then pg=\"\"; if [ -r \"/proc/$p/stat\" ]; then pg=\"$(awk '{{print $5}}' \"/proc/$p/stat\" 2>/dev/null | tr -d ' \\r\\n')\"; fi; \
           kill -s HUP \"$p\" >/dev/null 2>&1 || true; sleep 0.1; \
           kill -s TERM \"$p\" >/dev/null 2>&1 || true; sleep 0.3; \
           if [ -n \"$pg\" ]; then \
             kill -s HUP -\"$pg\" >/dev/null 2>&1 || true; sleep 0.1; \
             kill -s TERM -\"$pg\" >/dev/null 2>&1 || true; sleep 0.3; \
           fi; \
           kill -s KILL \"$p\" >/dev/null 2>&1 || true; \
         fi; fi; \
         if [ -f \"$tt\" ]; then \
           t=$(cat \"$tt\" 2>/dev/null); \
           kill_shells_by_tty \"$t\"; \
           # Also inject an 'exit' and Ctrl-D to the controlling TTY (best-effort) \
           if [ -n \"$t\" ]; then \
             printf \"exit\\r\\n\" > \"$t\" 2>/dev/null || true; sleep 0.1; \
             printf \"\\004\" > \"$t\" 2>/dev/null || true; \
           fi; \
         fi; \
         if [ ! -f \"$pp\" ] && [ -f \"$tp\" ]; then n=$(cat \"$tp\" 2>/dev/null); if [ -n \"$n\" ]; then \
           kill -s HUP -\"$n\" || true; sleep 0.1; \
           kill -s TERM -\"$n\" || true; sleep 0.3; \
           kill -s KILL -\"$n\" || true; \
         fi; fi",
        id = exec_id
    );
    let args: Vec<String> = vec![
        "docker".into(),
        "exec".into(),
        agent_container.into(),
        "sh".into(),
        "-lc".into(),
        script,
    ];
    if verbose {
        eprintln!("\raifo-coder: docker: {}", shell_join(&args));
    }
    let mut cmd = Command::new(runtime);
    for a in &args[1..] {
        cmd.arg(a);
    }
    cmd.stdout(Stdio::null()).stderr(Stdio::null());
    let _ = cmd.status();
}

/** Disconnect-triggered termination: INT then KILL with ~2s grace.
Adds a short pre-INT delay to let the shim post /signal first. */
fn disconnect_terminate_exec_in_container(
    runtime: &PathBuf,
    container: &str,
    exec_id: &str,
    verbose: bool,
    agent_container: Option<&str>,
) {
    // Always print a single disconnect line so the user sees it before returning to the agent
    log_stderr_and_file("\raifo-coder: disconnect");
    // Small grace to allow shim's trap to POST /signal.
    std::thread::sleep(Duration::from_millis(150));
    kill_in_container(runtime, container, exec_id, "INT", verbose);
    // In parallel, try to close the transient /run shell in the agent container, if known.
    if let Some(ac) = agent_container {
        kill_agent_shell_in_agent_container(runtime, ac, exec_id, verbose);
    }
    std::thread::sleep(Duration::from_millis(500));
    kill_in_container(runtime, container, exec_id, "TERM", verbose);
    std::thread::sleep(Duration::from_millis(1500));
    kill_in_container(runtime, container, exec_id, "KILL", verbose);
}

/// Build docker exec spawn args with setsid+PGID wrapper (use_tty controls -t).
fn build_exec_args_with_wrapper(
    container_name: &str,
    exec_preview_args: &[String],
    use_tty: bool,
) -> Vec<String> {
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
    // Allocate a TTY for streaming to improve interactive flushing when requested.
    if use_tty {
        spawn_args.insert(1, "-t".to_string());
    }
    // User command slice after container name
    let user_slice: Vec<String> = exec_preview_args[idx + 1..].to_vec();
    let inner = shell_join(&user_slice);
    let script = format!(
        "set -e; export PATH=\"/home/coder/.cargo/bin:/usr/local/cargo/bin:$PATH\"; eid=\"${{AIFO_EXEC_ID:-}}\"; if [ -z \"$eid\" ]; then exec {inner} 2>&1; fi; d=\"${{HOME:-/home/coder}}/.aifo-exec/${{AIFO_EXEC_ID:-}}\"; mkdir -p \"$d\" 2>/dev/null || {{ d=\"/tmp/.aifo-exec/${{AIFO_EXEC_ID:-}}\"; mkdir -p \"$d\" || true; }}; ( setsid sh -lc \"export PATH=\\\"/home/coder/.cargo/bin:/usr/local/cargo/bin:\\$PATH\\\"; exec {inner} 2>&1\" ) & pg=$!; printf \"%s\\n\" \"$pg\" > \"$d/pgid\" 2>/dev/null || true; wait \"$pg\"; rm -rf \"$d\" || true",
        inner = inner
    );
    spawn_args.push("sh".to_string());
    spawn_args.push("-c".to_string());
    spawn_args.push(script);
    spawn_args
}

pub fn toolexec_start_proxy(
    session_id: &str,
    verbose: bool,
) -> io::Result<(String, String, Arc<AtomicBool>, JoinHandle<()>)> {
    let runtime = container_runtime_path()?;

    #[cfg(unix)]
    let uid: u32 = u32::from(getuid());
    #[cfg(unix)]
    let gid: u32 = u32::from(getgid());
    #[cfg(not(unix))]
    let (uid, gid) = (0u32, 0u32);

    let token = random_token();
    let token_for_thread = token.clone();
    let timeout_secs: u64 = std_env::var("AIFO_TOOLEEXEC_MAX_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|&v| v > 0)
        .or_else(|| {
            std_env::var("AIFO_TOOLEEXEC_TIMEOUT_SECS")
                .ok()
                .and_then(|s| s.parse::<u64>().ok())
                .filter(|&v| v > 0)
        })
        .unwrap_or(0);
    let running = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let session = session_id.to_string();

    // Optional unix socket (Linux)
    let use_unix = cfg!(target_os = "linux")
        && std_env::var("AIFO_TOOLEEXEC_USE_UNIX").ok().as_deref() == Some("1");
    if use_unix {
        #[cfg(target_os = "linux")]
        {
            let base = "/run/aifo";
            let _ = fs::create_dir_all(base);
            let host_dir = format!("{}/aifo-{}", base, session);
            let _ = fs::create_dir_all(&host_dir);
            #[cfg(target_os = "linux")]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(&host_dir, fs::Permissions::from_mode(0o700));
            }
            let sock_path = format!("{}/toolexec.sock", host_dir);
            let _ = fs::remove_file(&sock_path);
            let listener = UnixListener::bind(&sock_path)
                .map_err(|e| io::Error::new(e.kind(), format!("proxy unix bind failed: {e}")))?;
            let _ = listener.set_nonblocking(true);
            std_env::set_var("AIFO_TOOLEEXEC_UNIX_DIR", &host_dir);
            let running_cl2 = running.clone();
            let token_for_thread2 = token_for_thread.clone();
            let host_dir_cl = host_dir.clone();
            let sock_path_cl = sock_path.clone();
            let handle = std::thread::spawn(move || {
                if verbose {
                    eprintln!("aifo-coder: toolexec proxy listening on unix socket");
                }
                let tool_cache = std::sync::Arc::new(std::sync::Mutex::new(HashMap::<(String, String), bool>::new()));
                let exec_registry = std::sync::Arc::new(std::sync::Mutex::new(HashMap::<String, String>::new()));
                let recent_signals = std::sync::Arc::new(std::sync::Mutex::new(HashMap::<String, std::time::Instant>::new()));
                loop {
                    if !running_cl2.load(std::sync::atomic::Ordering::SeqCst) {
                        break;
                    }
                    let (stream, _addr) = match listener.accept() {
                        Ok(pair) => pair,
                        Err(e) => {
                            if e.kind() == io::ErrorKind::WouldBlock {
                                std::thread::sleep(Duration::from_millis(50));
                                continue;
                            } else {
                                if verbose {
                                    eprintln!("aifo-coder: accept error: {}", e);
                                }
                                std::thread::sleep(Duration::from_millis(50));
                                continue;
                            }
                        }
                    };
                    let _ = stream.set_nonblocking(false);
                    if timeout_secs > 0 {
                        let _ = stream.set_read_timeout(Some(Duration::from_secs(timeout_secs)));
                    } else {
                        let _ = stream.set_read_timeout(None);
                    }
                    let _ = stream.set_write_timeout(None);

                    let tc = tool_cache.clone();
                    let er = exec_registry.clone();
                    let rs = recent_signals.clone();
                    let runtime_cl = runtime.clone();
                    let token_cl = token_for_thread2.clone();
                    let session_cl = session.clone();
                    std::thread::spawn(move || {
                        let ctx2 = ProxyCtx {
                            runtime: runtime_cl,
                            token: token_cl,
                            session: session_cl,
                            timeout_secs,
                            verbose,
                            agent_container: std_env::var("AIFO_CODER_CONTAINER_NAME").ok(),
                            uidgid: if cfg!(unix) { Some((uid, gid)) } else { None },
                        };
                        let mut s = stream;
                        handle_connection(&ctx2, &mut s, &tc, &er, &rs);
                    });
                }
                let _ = fs::remove_file(&sock_path_cl);
                let _ = fs::remove_dir(&host_dir_cl);
                if verbose {
                    eprintln!("aifo-coder: toolexec proxy stopped");
                }
            });
            let url = format!("unix://{}/toolexec.sock", host_dir);
            return Ok((url, token, running, handle));
        }
    }

    // TCP listener (default)
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
        let tool_cache = std::sync::Arc::new(std::sync::Mutex::new(HashMap::<(String, String), bool>::new()));
        let exec_registry = std::sync::Arc::new(std::sync::Mutex::new(HashMap::<String, String>::new()));
        let recent_signals = std::sync::Arc::new(std::sync::Mutex::new(HashMap::<String, std::time::Instant>::new()));
        loop {
            if !running_cl.load(std::sync::atomic::Ordering::SeqCst) {
                break;
            }
            let (stream, _addr) = match listener.accept() {
                Ok(pair) => pair,
                Err(e) => {
                    if e.kind() == io::ErrorKind::WouldBlock {
                        std::thread::sleep(Duration::from_millis(50));
                        continue;
                    } else {
                        if verbose {
                            eprintln!("aifo-coder: accept error: {}", e);
                        }
                        std::thread::sleep(Duration::from_millis(50));
                        continue;
                    }
                }
            };
            let _ = stream.set_nonblocking(false);
            if timeout_secs > 0 {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(timeout_secs)));
            } else {
                let _ = stream.set_read_timeout(None);
            }
            let _ = stream.set_write_timeout(None);

            let tc = tool_cache.clone();
            let er = exec_registry.clone();
            let rs = recent_signals.clone();
            let runtime_cl = runtime.clone();
            let token_cl = token_for_thread.clone();
            let session_cl = session.clone();
            std::thread::spawn(move || {
                let ctx2 = ProxyCtx {
                    runtime: runtime_cl,
                    token: token_cl,
                    session: session_cl,
                    timeout_secs,
                    verbose,
                    agent_container: std_env::var("AIFO_CODER_CONTAINER_NAME").ok(),
                    uidgid: if cfg!(unix) { Some((uid, gid)) } else { None },
                };
                let mut s = stream;
                handle_connection(&ctx2, &mut s, &tc, &er, &rs);
            });
        }
        if verbose {
            eprintln!("aifo-coder: toolexec proxy stopped");
        }
    });
    let url = format!("http://127.0.0.1:{}/exec", port);
    Ok((url, token, running, handle))
}

// Handle a single proxy connection
fn handle_connection<S: Read + Write>(
    ctx: &ProxyCtx,
    stream: &mut S,
    tool_cache: &Arc<Mutex<HashMap<(String, String), bool>>>,
    exec_registry: &Arc<Mutex<HashMap<String, String>>>,
    recent_signals: &Arc<Mutex<HashMap<String, std::time::Instant>>>,
) {
    let token: &str = &ctx.token;
    let session: &str = &ctx.session;
    let timeout_secs: u64 = ctx.timeout_secs;
    let verbose: bool = ctx.verbose;
    let uidgid = ctx.uidgid;

    // Parse request
    let req = match http::read_http_request(stream) {
        Ok(r) => r,
        Err(_e) => {
            respond_plain(stream, "400 Bad Request", 86, ERR_BAD_REQUEST);
            let _ = stream.flush();
            return;
        }
    };

    // Endpoint classification and method enforcement
    let endpoint = http::classify_endpoint(&req.path_lc);
    match endpoint {
        Some(http::Endpoint::Exec)
        | Some(http::Endpoint::Notifications)
        | Some(http::Endpoint::Signal) => {
            if req.method != http::Method::Post {
                respond_plain(stream, "405 Method Not Allowed", 86, ERR_METHOD_NOT_ALLOWED);
                let _ = stream.flush();
                return;
            }
        }
        None => {
            respond_plain(stream, "404 Not Found", 86, ERR_NOT_FOUND);
            let _ = stream.flush();
            return;
        }
    }

    // Auth/proto centralized
    let auth_res = auth::validate_auth_and_proto(&req.headers, token);

    // Merge form/query
    let form = String::from_utf8_lossy(&req.body).to_string();
    let mut tool = String::new();
    let mut cwd = "/workspace".to_string();
    let mut argv: Vec<String> = Vec::new();
    for (k, v) in req
        .query
        .iter()
        .cloned()
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

    // ExecId: accept header or generate, and log parsed request including exec_id
    let exec_id = req
        .headers
        .get("x-aifo-exec-id")
        .cloned()
        .unwrap_or_else(random_token);
    if matches!(endpoint, Some(http::Endpoint::Exec)) {
        log_parsed_request(verbose, &tool, &argv, &cwd, &exec_id);
    }

    // Notifications
    if matches!(endpoint, Some(http::Endpoint::Notifications)) {
        let noauth = std_env::var("AIFO_NOTIFICATIONS_NOAUTH").ok().as_deref() == Some("1");
        if noauth {
            let notif_to = std_env::var("AIFO_NOTIFICATIONS_TIMEOUT_SECS")
                .ok()
                .and_then(|s| s.parse::<u64>().ok())
                .filter(|&v| v > 0)
                .unwrap_or(if timeout_secs == 0 { 5 } else { timeout_secs });
            match notifications::notifications_handle_request(&argv, verbose, notif_to) {
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

        match auth_res {
            auth::AuthResult::Authorized { proto: _ } => {
                let notif_to = std_env::var("AIFO_NOTIFICATIONS_TIMEOUT_SECS")
                    .ok()
                    .and_then(|s| s.parse::<u64>().ok())
                    .filter(|&v| v > 0)
                    .unwrap_or(if timeout_secs == 0 { 5 } else { timeout_secs });
                match notifications::notifications_handle_request(&argv, verbose, notif_to) {
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
            auth::AuthResult::MissingOrInvalidProto => {
                respond_plain(stream, "426 Upgrade Required", 86, ERR_UNSUPPORTED_PROTO);
                let _ = stream.flush();
                return;
            }
            auth::AuthResult::MissingOrInvalidAuth => {
                respond_plain(stream, "401 Unauthorized", 86, ERR_UNAUTHORIZED);
                let _ = stream.flush();
                return;
            }
        }
    }

    // /signal endpoint
    if matches!(endpoint, Some(http::Endpoint::Signal)) {
        match auth_res {
            auth::AuthResult::Authorized { .. } => {
                // Parse form for exec_id and signal
                let form = String::from_utf8_lossy(&req.body).to_string();
                let mut exec_id = String::new();
                let mut signal = "TERM".to_string();
                for (k, v) in http::parse_form_urlencoded(&form) {
                    match k.as_str() {
                        "exec_id" => exec_id = v,
                        "signal" => signal = v,
                        _ => {}
                    }
                }
                if exec_id.is_empty() {
                    respond_plain(stream, "400 Bad Request", 86, ERR_BAD_REQUEST);
                    let _ = stream.flush();
                    return;
                }
                let container = if let Some(name) = exec_registry.lock().unwrap().get(&exec_id).cloned() {
                    name
                } else {
                    respond_plain(stream, "404 Not Found", 86, ERR_NOT_FOUND);
                    let _ = stream.flush();
                    return;
                };
                // Allow only a safe subset of signals
                let sig = signal.to_ascii_uppercase();
                let allowed = ["INT", "TERM", "HUP", "KILL"];
                if !allowed.contains(&sig.as_str()) {
                    respond_plain(stream, "400 Bad Request", 86, ERR_BAD_REQUEST);
                    let _ = stream.flush();
                    return;
                }
                if verbose {
                    log_stderr_and_file(&format!(
                        "\raifo-coder: proxy signal: exec_id={} sig={}",
                        exec_id, sig
                    ));
                }
                kill_in_container(&ctx.runtime, &container, &exec_id, &sig, verbose);
                // Record recent /signal for this exec to suppress duplicate disconnect escalation
                {
                    let mut rs = recent_signals.lock().unwrap();
                    rs.insert(exec_id.clone(), std::time::Instant::now());
                }
                // 204 No Content without exit code header
                let _ = stream.write_all(
                    b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                );
                let _ = stream.flush();
                return;
            }
            auth::AuthResult::MissingOrInvalidProto => {
                respond_plain(stream, "426 Upgrade Required", 86, ERR_UNSUPPORTED_PROTO);
                let _ = stream.flush();
                return;
            }
            auth::AuthResult::MissingOrInvalidAuth => {
                respond_plain(stream, "401 Unauthorized", 86, ERR_UNAUTHORIZED);
                let _ = stream.flush();
                return;
            }
        }
    }

    // Exec path: early allowlist any-kind
    if !tool.is_empty() && !is_tool_allowed_any_sidecar(&tool) {
        respond_plain(stream, "403 Forbidden", 86, ERR_FORBIDDEN);
        let _ = stream.flush();
        return;
    }

    if tool.is_empty() {
        match auth_res {
            auth::AuthResult::MissingOrInvalidProto => {
                respond_plain(stream, "426 Upgrade Required", 86, ERR_UNSUPPORTED_PROTO);
                let _ = stream.flush();
                return;
            }
            auth::AuthResult::MissingOrInvalidAuth => {
                respond_plain(stream, "401 Unauthorized", 86, ERR_UNAUTHORIZED);
                let _ = stream.flush();
                return;
            }
            auth::AuthResult::Authorized { .. } => {
                respond_plain(stream, "400 Bad Request", 86, ERR_BAD_REQUEST);
                let _ = stream.flush();
                return;
            }
        }
    }

    let (authorized, proto_v2) = match auth_res {
        auth::AuthResult::Authorized { proto } => (true, matches!(proto, auth::Proto::V2)),
        auth::AuthResult::MissingOrInvalidProto => {
            respond_plain(stream, "426 Upgrade Required", 86, ERR_UNSUPPORTED_PROTO);
            let _ = stream.flush();
            return;
        }
        auth::AuthResult::MissingOrInvalidAuth => {
            respond_plain(stream, "401 Unauthorized", 86, ERR_UNAUTHORIZED);
            let _ = stream.flush();
            return;
        }
    };
    debug_assert!(authorized);

    // Route to sidecar kind and enforce allowlist
    let selected_kind = {
        let mut cache = tool_cache.lock().unwrap();
        select_kind_for_tool(session, &tool, timeout_secs, &mut *cache)
    };
    let kind = selected_kind.as_str();
    let allow = sidecar_allowlist(kind);
    if !allow.contains(&tool.as_str()) {
        respond_plain(stream, "403 Forbidden", 86, ERR_FORBIDDEN);
        let _ = stream.flush();
        return;
    }

    let name = sidecar::sidecar_container_name(kind, session);
    if !container_exists(&name) {
        let msg = format!(
            "tool '{}' not available in running sidecars; start an appropriate toolchain (e.g., --toolchain c-cpp or --toolchain rust)\n",
            tool
        );
        respond_plain(stream, "409 Conflict", 86, msg.as_bytes());
        let _ = stream.flush();
        return;
    }

    let pwd = std::path::PathBuf::from(cwd);
    let mut full_args: Vec<String>;
    if tool == "tsc" {
        let nm_tsc = pwd.join("node_modules").join(".bin").join("tsc");
        if nm_tsc.exists() {
            full_args = vec!["./node_modules/.bin/tsc".to_string()];
            full_args.extend(argv.clone());
            if verbose {
                eprintln!("\raifo-coder: proxy exec: tsc via local node_modules\r\n\r");
            }
        } else {
            full_args = vec!["npx".to_string(), "tsc".to_string()];
            full_args.extend(argv.clone());
            if verbose {
                eprintln!("\raifo-coder: proxy exec: tsc via npx\r\n\r");
            }
        }
    } else {
        full_args = vec![tool.clone()];
        full_args.extend(argv.clone());
    }

    // ExecId already determined above; reuse
    // Register exec_id -> container
    {
        let mut er = exec_registry.lock().unwrap();
        er.insert(exec_id.clone(), name.clone());
    }

    let exec_preview_args = sidecar::build_sidecar_exec_preview_with_exec_id(
        &name,
        if cfg!(unix) { uidgid } else { None },
        &pwd,
        kind,
        &full_args,
        Some(&exec_id),
    );

    if verbose {
        eprintln!(
            "\raifo-coder: proxy docker: {}",
            shell_join(&exec_preview_args)
        );
    }

    if proto_v2 {
        // Streaming (v2)
        if verbose {
            log_stderr_and_file("\raifo-coder: proxy exec: proto=v2 (streaming)\r\n\r");
        }
        let started = std::time::Instant::now();

        let use_tty = std_env::var("AIFO_TOOLEEXEC_TTY").ok().as_deref() != Some("0");
        let spawn_args = build_exec_args_with_wrapper(&name, &exec_preview_args, use_tty);
        let mut cmd = Command::new(&ctx.runtime);
        for a in &spawn_args {
            cmd.arg(a);
        }
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                let mut b = format!("aifo-coder proxy error: {}", e).into_bytes();
                b.push(b'\n');
                log_request_result(verbose, &tool, kind, 86, &started);
                respond_plain(stream, "500 Internal Server Error", 86, &b);
                let _ = stream.flush();
                return;
            }
        };

        // Optional max-runtime escalation watcher
        let done = Arc::new(AtomicBool::new(false));
        if timeout_secs > 0 {
            let done_cl = done.clone();
            let runtime_cl = ctx.runtime.clone();
            let container_cl = name.clone();
            let exec_id_cl = exec_id.clone();
            let verbose_cl = verbose;
            std::thread::spawn(move || {
                let mut accum: u64 = 0;
                for (sig, dur) in [("INT", timeout_secs), ("TERM", 5), ("KILL", 5)].into_iter() {
                    let mut secs = dur;
                    while secs > 0 {
                        if done_cl.load(std::sync::atomic::Ordering::SeqCst) {
                            return;
                        }
                        let step = secs.min(1);
                        std::thread::sleep(Duration::from_secs(step));
                        secs -= step;
                        accum = accum.saturating_add(step);
                    }
                    if done_cl.load(std::sync::atomic::Ordering::SeqCst) {
                        return;
                    }
                    if verbose_cl {
                        eprintln!(
                            "\raifo-coder: max-runtime: sending {} to exec_id={} after {}s",
                            sig, exec_id_cl, accum
                        );
                    }
                    kill_in_container(&runtime_cl, &container_cl, &exec_id_cl, sig, verbose_cl);
                }
            });
        }

        // Send prelude after successful spawn (include ExecId)
        respond_chunked_prelude(stream, Some(&exec_id));

        // Drain stderr to avoid backpressure
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

        // Stream stdout
        let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
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
        drop(tx);

        // Stream until EOF or write error
        let mut write_failed = false;
        loop {
            match rx.recv_timeout(Duration::from_millis(200)) {
                Ok(chunk) => {
                    if let Err(_e) = respond_chunked_write_chunk(stream, &chunk) {
                        write_failed = true;
                        break;
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }

        if write_failed {
            // Client disconnected: if we saw a recent /signal for this exec, skip duplicate escalation.
            let suppress = {
                let rs = recent_signals.lock().unwrap();
                rs.get(&exec_id)
                    .map(|ts| ts.elapsed() < Duration::from_millis(2300))
                    .unwrap_or(false)
            };
            if suppress {
                log_stderr_and_file("\raifo-coder: disconnect");
                if let Some(ac) = ctx.agent_container.as_deref() {
                    kill_agent_shell_in_agent_container(&ctx.runtime, ac, &exec_id, verbose);
                }
            } else {
                disconnect_terminate_exec_in_container(
                    &ctx.runtime,
                    &name,
                    &exec_id,
                    verbose,
                    ctx.agent_container.as_deref(),
                );
            }
            let _ = child.kill();
            let _ = child.wait();
            // Mark watcher done and remove from registry
            done.store(true, std::sync::atomic::Ordering::SeqCst);
            {
                let mut er = exec_registry.lock().unwrap();
                let _ = er.remove(&exec_id);
            }
            {
                let mut rs = recent_signals.lock().unwrap();
                let _ = rs.remove(&exec_id);
            }
            return;
        }

        let code = child.wait().ok().and_then(|s| s.code()).unwrap_or(1);
        // Mark watcher done and remove from registry
        done.store(true, std::sync::atomic::Ordering::SeqCst);
        {
            let mut er = exec_registry.lock().unwrap();
            let _ = er.remove(&exec_id);
        }
        {
            let mut rs = recent_signals.lock().unwrap();
            let _ = rs.remove(&exec_id);
        }
        log_request_result(verbose, &tool, kind, code, &started);
        respond_chunked_trailer(stream, code);
        return;
    }

    // Buffered (v1)
    if verbose {
        log_stderr_and_file("\raifo-coder: proxy exec: proto=v1 (buffered)\r\n\r");
    }
    let started = std::time::Instant::now();

    let spawn_args = build_exec_args_with_wrapper(&name, &exec_preview_args, false);
    let mut cmd = Command::new(&ctx.runtime);
    for a in &spawn_args {
        cmd.arg(a);
    }
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            let mut b = format!("aifo-coder proxy error: {}", e).into_bytes();
            b.push(b'\n');
            log_request_result(verbose, &tool, kind, 86, &started);
            respond_plain(stream, "500 Internal Server Error", 86, &b);
            let _ = stream.flush();
            return;
        }
    };

    // Optional max-runtime escalation watcher
    let done = Arc::new(AtomicBool::new(false));
    if timeout_secs > 0 {
        let done_cl = done.clone();
        let runtime_cl = ctx.runtime.clone();
        let container_cl = name.clone();
        let exec_id_cl = exec_id.clone();
        let verbose_cl = verbose;
        std::thread::spawn(move || {
            let mut accum: u64 = 0;
            for (sig, dur) in [("INT", timeout_secs), ("TERM", 5), ("KILL", 5)].into_iter() {
                let mut secs = dur;
                while secs > 0 {
                    if done_cl.load(std::sync::atomic::Ordering::SeqCst) {
                        return;
                    }
                    let step = secs.min(1);
                    std::thread::sleep(Duration::from_secs(step));
                    secs -= step;
                    accum = accum.saturating_add(step);
                }
                if done_cl.load(std::sync::atomic::Ordering::SeqCst) {
                    return;
                }
                if verbose_cl {
                    eprintln!(
                        "\raifo-coder: max-runtime: sending {} to exec_id={} after {}s",
                        sig, exec_id_cl, accum
                    );
                }
                kill_in_container(&runtime_cl, &container_cl, &exec_id_cl, sig, verbose_cl);
            }
        });
    }

    // Drain stdout/stderr concurrently
    let out_buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::<u8>::new()));
    let err_buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::<u8>::new()));

    let mut h_out = None;
    if let Some(mut so) = child.stdout.take() {
        let out_buf_cl = out_buf.clone();
        h_out = Some(std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            loop {
                match so.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if let Ok(mut w) = out_buf_cl.lock() {
                            w.extend_from_slice(&buf[..n]);
                        }
                    }
                    Err(_) => break,
                }
            }
        }));
    }

    let mut h_err = None;
    if let Some(mut se) = child.stderr.take() {
        let err_buf_cl = err_buf.clone();
        h_err = Some(std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            loop {
                match se.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if let Ok(mut w) = err_buf_cl.lock() {
                            w.extend_from_slice(&buf[..n]);
                        }
                    }
                    Err(_) => break,
                }
            }
        }));
    }

    // Wait for child without hard timeout; join output threads
    let st = child.wait();
    done.store(true, std::sync::atomic::Ordering::SeqCst);
    let final_code: i32 = st.ok().and_then(|s| s.code()).unwrap_or(1);

    if let Some(h) = h_out {
        let _ = h.join();
    }
    if let Some(h) = h_err {
        let _ = h.join();
    }

    let mut body_bytes = {
        let out = out_buf.lock().ok().map(|b| b.clone()).unwrap_or_default();
        out
    };
    if let Ok(err) = err_buf.lock() {
        if !err.is_empty() {
            body_bytes.extend_from_slice(&err);
        }
    }

    {
        let mut er = exec_registry.lock().unwrap();
        let _ = er.remove(&exec_id);
    }
    {
        let mut rs = recent_signals.lock().unwrap();
        let _ = rs.remove(&exec_id);
    }
    let code = final_code;
    log_request_result(verbose, &tool, kind, code, &started);
    let header = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nX-Exit-Code: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        code,
        body_bytes.len()
    );
    let _ = stream.write_all(header.as_bytes());
    let _ = stream.write_all(&body_bytes);
    let _ = stream.flush();
}

fn is_tool_allowed_any_sidecar(tool: &str) -> bool {
    let tl = tool.to_ascii_lowercase();
    ["rust", "node", "python", "c-cpp", "go"]
        .iter()
        .any(|k| sidecar_allowlist(k).contains(&tl.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_exec_args_with_wrapper_v1_like_includes_pgid_file_and_no_tty() {
        let container = "tc-container";
        // Minimal plausible preview: docker exec ... <container> <user-cmd...>
        let exec_preview_args: Vec<String> = vec![
            "docker".into(),
            "exec".into(),
            "-w".into(),
            "/workspace".into(),
            container.into(),
            "echo".into(),
            "hello".into(),
        ];
        let out = build_exec_args_with_wrapper(container, &exec_preview_args, false);
        // Should not include -t when use_tty=false
        assert!(
            !out.iter().any(|s| s == "-t"),
            "unexpected -t (tty) in non-streaming wrapper args: {:?}",
            out
        );
        // Should end with "sh -c <script>" and script must contain exec dir + pgid file + setsid
        assert!(
            out.iter().any(|s| s == "sh") && out.iter().any(|s| s == "-c"),
            "wrapper should invoke sh -c, got: {:?}",
            out
        );
        let script = out.last().expect("script arg");
        assert!(
            script.contains("/.aifo-exec/${AIFO_EXEC_ID:-}") && script.contains("/pgid"),
            "wrapper script should create pgid file under exec dir, got: {}",
            script
        );
        assert!(
            script.contains("setsid"),
            "wrapper script should use setsid to create a new process group, got: {}",
            script
        );
    }
}
