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
use std::path::{Path, PathBuf};
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
    build_sidecar_exec_preview, build_streaming_exec_args, log_parsed_request, log_request_result,
    random_token, ERR_BAD_REQUEST, ERR_FORBIDDEN, ERR_METHOD_NOT_ALLOWED, ERR_NOT_FOUND,
    ERR_UNAUTHORIZED, ERR_UNSUPPORTED_PROTO,
};

use super::{
    respond_chunked_prelude, respond_chunked_trailer, respond_chunked_write_chunk, respond_plain,
};

struct ProxyCtx {
    runtime: PathBuf,
    token: String,
    session: String,
    timeout_secs: u64,
    verbose: bool,
    uidgid: Option<(u32, u32)>,
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
    let timeout_secs: u64 = std_env::var("AIFO_TOOLEEXEC_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|&v| v > 0)
        .unwrap_or(60);
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
    let url = format!("http://host.docker.internal:{}/exec", port);
    Ok((url, token, running, handle))
}

// Handle a single proxy connection
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
        Some(http::Endpoint::Exec) | Some(http::Endpoint::Notifications) => {
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
                let _ = std::io::stdout().flush();
                let _ = std::io::stderr().flush();
                eprintln!("aifo-coder: proxy exec: tsc via local node_modules");
            }
        } else {
            full_args = vec!["npx".to_string(), "tsc".to_string()];
            full_args.extend(argv.clone());
            if verbose {
                let _ = std::io::stdout().flush();
                let _ = std::io::stderr().flush();
                eprintln!("aifo-coder: proxy exec: tsc via npx");
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
        eprintln!("aifo-coder: proxy docker:");
        eprintln!("  {}", shell_join(&exec_preview_args));
    }

    if proto_v2 {
        // Streaming (v2)
        if verbose {
            eprintln!("aifo-coder: proxy exec: proto=v2 (streaming)");
        }
        let started = std::time::Instant::now();

        let spawn_args = build_streaming_exec_args(&name, &exec_preview_args);
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

        // Send prelude after successful spawn
        respond_chunked_prelude(stream);

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

        // Timeout watcher
        let (tox, tor) = std::sync::mpsc::channel::<()> > ();
        let timeout_secs_cl = timeout_secs;
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(timeout_secs_cl));
            let _ = tox.send(());
        });

        let mut timed_out = false;
        loop {
            if tor.try_recv().is_ok() {
                let _ = child.kill();
                timed_out = true;
                break;
            }
            match rx.recv_timeout(Duration::from_millis(200)) {
                Ok(chunk) => {
                    respond_chunked_write_chunk(stream, &chunk);
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
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
        log_request_result(verbose, &tool, kind, code, &started);
        respond_chunked_trailer(stream, code);
        return;
    }

    // Buffered (v1)
    if verbose {
        eprintln!("aifo-coder: proxy exec: proto=v1 (buffered)");
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

    // Timeout and exit aggregation
    let deadline = std::time::Instant::now() + Duration::from_secs(timeout_secs);
    let mut exit_code: Option<i32> = None;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                exit_code = Some(status.code().unwrap_or(1));
                break;
            }
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    if let Some(h) = h_out.take() {
                        let _ = h.join();
                    }
                    if let Some(h) = h_err.take() {
                        let _ = h.join();
                    }
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
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(_e) => {
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }

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

    let code = exit_code.unwrap_or(1);
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
