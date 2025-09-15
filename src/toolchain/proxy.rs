/*!
Proxy module: dispatcher, accept loop, and public toolexec_start_proxy API.

Implements Phase 2 steady-state architecture:
- Listener setup (TCP/unix), accept loop with backoff.
- Per-connection dispatcher using http::read_http_request + http::classify_endpoint.
- Centralized auth/proto via auth::validate_auth_and_proto.
- Notifications policy per spec, including optional NOAUTH bypass.
- Streaming prelude only after successful spawn; plain 500 on spawn error.
- Buffered timeout kills child to avoid orphans.
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
use std::sync::Arc;
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
    build_sidecar_exec_preview, log_parsed_request, log_request_result, random_token,
    ERR_BAD_REQUEST, ERR_FORBIDDEN, ERR_METHOD_NOT_ALLOWED, ERR_NOT_FOUND, ERR_UNAUTHORIZED,
    ERR_UNSUPPORTED_PROTO,
};

struct ProxyCtx {
    runtime: PathBuf,
    token: String,
    session: String,
    timeout_secs: u64,
    verbose: bool,
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
        eprintln!("aifo-coder: docker: {}", shell_join(&args));
    }
    let mut cmd = Command::new(runtime);
    for a in &args[1..] {
        cmd.arg(a);
    }
    cmd.stdout(Stdio::null()).stderr(Stdio::null());
    let _ = cmd.status();
}

/// TERM then KILL with ~2s grace.
fn terminate_exec_in_container(
    runtime: &PathBuf,
    container: &str,
    exec_id: &str,
    verbose: bool,
) {
    kill_in_container(runtime, container, exec_id, "TERM", verbose);
    std::thread::sleep(Duration::from_secs(2));
    kill_in_container(runtime, container, exec_id, "KILL", verbose);
}


/// Build streaming docker exec spawn args: add -t and wrap with setsid+PGID script.
fn build_streaming_exec_args(
    container_name: &str,
    exec_preview_args: &[String],
    _exec_id: &str,
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
    // Allocate a TTY for streaming to improve interactive flushing.
    // Set AIFO_TOOLEEXEC_TTY=0 to disable TTY allocation if it interferes with tooling.
    let use_tty = std_env::var("AIFO_TOOLEEXEC_TTY").ok().as_deref() != Some("0");
    if use_tty {
        spawn_args.insert(1, "-t".to_string());
    }
    // User command slice after container name
    let user_slice: Vec<String> = exec_preview_args[idx + 1..].to_vec();
    let inner = shell_join(&user_slice);
    let script = format!(
        "set -e; d=\"/home/coder/.aifo-exec/${{AIFO_EXEC_ID:-}}\"; if [ -z \"$d\" ]; then exec {inner} 2>&1; fi; mkdir -p \"$d\"; ( setsid sh -lc \"exec {inner} 2>&1\" ) & pg=$!; printf \"%s\" \"$pg\" > \"$d/pgid\"; wait \"$pg\"",
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
                let mut tool_cache: HashMap<(String, String), bool> = HashMap::new();
                let mut exec_registry: HashMap<String, String> = HashMap::new();
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

                    let ctx = ProxyCtx {
                        runtime: runtime.clone(),
                        token: token_for_thread2.clone(),
                        session: session.clone(),
                        timeout_secs,
                        verbose,
                        uidgid: if cfg!(unix) { Some((uid, gid)) } else { None },
                    };
                    handle_connection(&ctx, &mut stream, &mut tool_cache, &mut exec_registry);
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
        let mut tool_cache: HashMap<(String, String), bool> = HashMap::new();
        let mut exec_registry: HashMap<String, String> = HashMap::new();
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

            let ctx = ProxyCtx {
                runtime: runtime.clone(),
                token: token_for_thread.clone(),
                session: session.clone(),
                timeout_secs,
                verbose,
                uidgid: if cfg!(unix) { Some((uid, gid)) } else { None },
            };
            handle_connection(&ctx, &mut stream, &mut tool_cache, &mut exec_registry);
        }
        if verbose {
            eprintln!("aifo-coder: toolexec proxy stopped");
        }
    });
    let url = format!("http://host.docker.internal:{}/exec", port);
    Ok((url, token, running, handle))
}

// Handle a single proxy connection
fn handle_connection<S: Read + Write>(
    ctx: &ProxyCtx,
    stream: &mut S,
    tool_cache: &mut HashMap<(String, String), bool>,
    exec_registry: &mut HashMap<String, String>,
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
        Some(http::Endpoint::Exec) | Some(http::Endpoint::Notifications) | Some(http::Endpoint::Signal) => {
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

    // Log
    log_parsed_request(verbose, &tool, &argv, &cwd);

    // Notifications
    if matches!(endpoint, Some(http::Endpoint::Notifications)) {
        let noauth = std_env::var("AIFO_NOTIFICATIONS_NOAUTH").ok().as_deref() == Some("1");
        if noauth {
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

        match auth_res {
            auth::AuthResult::Authorized { proto: _ } => {
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
                let container = if let Some(name) = exec_registry.get(&exec_id) {
                    name.clone()
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
                kill_in_container(&ctx.runtime, &container, &exec_id, &sig, verbose);
                // 204 No Content without exit code header
                let _ = stream.write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
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
    let selected_kind = select_kind_for_tool(session, &tool, timeout_secs, tool_cache);
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
                eprintln!("aifo-coder: proxy exec: tsc via local node_modules\r");
            }
        } else {
            full_args = vec!["npx".to_string(), "tsc".to_string()];
            full_args.extend(argv.clone());
            if verbose {
                eprintln!("aifo-coder: proxy exec: tsc via npx\r\n\r");
            }
        }
    } else {
        full_args = vec![tool.clone()];
        full_args.extend(argv.clone());
    }

    // ExecId: accept header or generate
    let exec_id = req
        .headers
        .get("x-aifo-exec-id")
        .cloned()
        .unwrap_or_else(|| random_token());
    // Register exec_id -> container
    exec_registry.insert(exec_id.clone(), name.clone());

    let exec_preview_args = build_sidecar_exec_preview(
        &name,
        if cfg!(unix) { uidgid } else { None },
        &pwd,
        kind,
        &full_args,
        Some(&exec_id),
    );

    if verbose {
        eprintln!(
            "aifo-coder: proxy docker: {}\r",
            shell_join(&exec_preview_args)
        );
    }

    if proto_v2 {
        // Streaming (v2)
        if verbose {
            eprintln!("aifo-coder: proxy exec: proto=v2 (streaming)\r\n\r");
        }
        let started = std::time::Instant::now();

        let spawn_args = build_streaming_exec_args(&name, &exec_preview_args, &exec_id);
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
            // Client disconnected: terminate process group in container and stop docker exec
            terminate_exec_in_container(&ctx.runtime, &name, &exec_id, verbose);
            let _ = child.kill();
            let _ = child.wait();
            // Remove from registry
            let _ = exec_registry.remove(&exec_id);
            return;
        }

        let code = child.wait().ok().and_then(|s| s.code()).unwrap_or(1);
        // Remove from registry
        let _ = exec_registry.remove(&exec_id);
        log_request_result(verbose, &tool, kind, code, &started);
        respond_chunked_trailer(stream, code);
        return;
    }

    // Buffered (v1)
    if verbose {
        eprintln!("aifo-coder: proxy exec: proto=v1 (buffered)\r\n\r");
    }
    let started = std::time::Instant::now();

    let mut cmd = Command::new(&ctx.runtime);
    for a in &exec_preview_args[1..] {
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

    let _ = exec_registry.remove(&exec_id);
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
