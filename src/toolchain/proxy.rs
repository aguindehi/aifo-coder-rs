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
#[cfg(feature = "otel")]
use crate::telemetry::{hash_string_hex, telemetry_pii_enabled};
use once_cell::sync::Lazy;
#[cfg(feature = "otel")]
use opentelemetry::global;
#[cfg(feature = "otel")]
use opentelemetry::propagation::{Extractor, Injector};
#[cfg(feature = "otel")]
use opentelemetry::Context;
use std::collections::HashMap;
use std::collections::HashSet;
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
use std::sync::atomic::{AtomicBool, AtomicUsize};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;
#[cfg(feature = "otel")]
use tracing::{info_span, instrument};
#[cfg(feature = "otel")]
use tracing_opentelemetry::OpenTelemetrySpanExt;

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

#[cfg(feature = "otel")]
struct HeaderMapExtractor<'a> {
    headers: &'a std::collections::HashMap<String, String>,
}

#[cfg(feature = "otel")]
impl<'a> Extractor for HeaderMapExtractor<'a> {
    fn get(&self, key: &str) -> Option<&str> {
        // Try exact key first, then lowercase variant for robustness.
        self.headers
            .get(key)
            .or_else(|| self.headers.get(&key.to_ascii_lowercase()))
            .map(|s| s.as_str())
    }

    fn keys(&self) -> Vec<&str> {
        self.headers.keys().map(|k| k.as_str()).collect()
    }
}

#[cfg(feature = "otel")]
struct HeaderMapInjector<'a> {
    headers: &'a mut std::collections::HashMap<String, String>,
}

#[cfg(feature = "otel")]
impl<'a> Injector for HeaderMapInjector<'a> {
    fn set(&mut self, key: &str, value: String) {
        self.headers.insert(key.to_string(), value);
    }
}

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

fn respond_chunked_prelude<W: Write>(w: &mut W, exec_id: Option<&str>) -> io::Result<()> {
    let mut hdr = String::from("HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nTransfer-Encoding: chunked\r\nTrailer: X-Exit-Code\r\nConnection: close\r\n");
    if let Some(id) = exec_id {
        hdr.push_str(&format!("X-Exec-Id: {}\r\n", id));
    }
    hdr.push_str("\r\n");
    w.write_all(hdr.as_bytes())?;
    w.flush()
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

fn respond_chunked_trailer<W: Write>(w: &mut W, code: i32) -> io::Result<()> {
    w.write_all(b"0\r\n")?;
    let trailer = format!("X-Exit-Code: {code}\r\n\r\n");
    w.write_all(trailer.as_bytes())?;
    w.flush()
}

/// Best-effort detection of an unreadable/untraversable /workspace inside the container
/// for the current uid:gid. Returns a diagnostic body if access appears denied.
fn workspace_access_hint(
    runtime: &PathBuf,
    container: &str,
    uidgid: Option<(u32, u32)>,
    verbose: bool,
) -> Option<Vec<u8>> {
    let mut args: Vec<String> = vec!["docker".into(), "exec".into()];
    if let Some((uid, gid)) = uidgid {
        args.push("-u".into());
        args.push(format!("{uid}:{gid}"));
    }
    args.push(container.into());
    args.push("sh".into());
    args.push("-c".into());
    let script = "set -e; ls -ld /workspace 2>&1; if [ -x /workspace ] && [ -r /workspace ]; then echo OK; else echo DENIED; fi";
    args.push(script.into());
    if verbose {
        log_compact(&format!("aifo-coder: docker: {}", shell_join(&args)));
    }
    let mut cmd = Command::new(runtime);
    for a in &args[1..] {
        cmd.arg(a);
    }
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let out = match cmd.output() {
        Ok(o) => o,
        Err(_) => return None,
    };
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    if stdout.contains("OK") {
        None
    } else {
        let mut msg = String::new();
        msg.push_str("workspace not readable/traversable for current uid:gid inside container; cwd=/workspace\n");
        msg.push_str(&stdout);
        msg.push_str("\nHint: On macOS, temporary directories may be 0700 and not traversable in Docker. Consider 'chmod -R 755 <project>' or adjust your test harness to relax permissions.\n");
        Some(msg.into_bytes())
    }
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
    let _ = io::stderr().flush();
}

/// Compact stderr logger for streaming contexts:
/// - Re-anchors at column 0 when needed (prefix '\r').
/// - Always terminates with '\n\r' to ensure the next line starts at column 0.
/// - Also tees a plain line to AIFO_TEST_LOG_PATH (without CRs) when set.
fn log_compact(s: &str) {
    // Re-anchor at column 0, clear to end of line to avoid stale characters, terminate with \n\r
    eprint!("\r{}\x1b[K\n\r", s);
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
    let _ = io::stderr().flush();
}

/// Per-connection verbose logger with boundary-aware printing.
/// - info(): plain line (clears to EOL), ends with \n\r
/// - compact(): forces CR anchor + clear-to-EOL, ends with \n\r
/// - boundary_log(): compact if boundary is set, otherwise info
/// - set_boundary(): mark that previous operation streamed payload; next log should re-anchor
struct StreamLogger {
    verbose: bool,
    boundary: bool,
    tee_path: Option<PathBuf>,
}

impl StreamLogger {
    fn new(verbose: bool) -> Self {
        let tee_path = std_env::var("AIFO_TEST_LOG_PATH")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .map(PathBuf::from);
        StreamLogger {
            verbose,
            boundary: false,
            tee_path,
        }
    }

    fn tee(&self, s: &str) {
        if let Some(p) = &self.tee_path {
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(p)
            {
                use std::io::Write as _;
                let _ = writeln!(f, "{}", s);
            }
        }
    }

    fn info(&mut self, s: &str) {
        if !self.verbose {
            return;
        }
        eprint!("{}\x1b[K\n\r", s);
        self.tee(s);
        let _ = io::stderr().flush();
        // info() does not alter boundary
    }

    fn compact(&mut self, s: &str) {
        if !self.verbose {
            return;
        }
        eprint!("\r{}\x1b[K\n\r", s);
        self.tee(s);
        let _ = io::stderr().flush();
        self.boundary = false;
    }

    fn boundary_log(&mut self, s: &str) {
        if self.boundary {
            self.compact(s);
        } else {
            self.info(s);
        }
    }

    fn set_boundary(&mut self) {
        self.boundary = true;
    }
}

// Small helpers/constants to reduce duplication in proxy logs
const DISCONNECT_MSG: &str = "aifo-coder: disconnect";

fn log_disconnect() {
    log_compact(DISCONNECT_MSG);
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
        "-c".into(),
        script,
    ];
    if verbose {
        log_compact(&format!("aifo-coder: docker: {}", shell_join(&args)));
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
           # Also inject an 'exit' and Ctrl-D to the controlling TTY (best-effort, opt-in) \
           if [ -n \"$t\" ] && [ \"${{AIFO_PROXY_INJECT_EXIT_ON_TTY:-0}}\" = \"1\" ]; then \
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
        "-c".into(),
        script,
    ];
    if verbose {
        log_compact(&format!("aifo-coder: docker: {}", shell_join(&args)));
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
    log_disconnect();
    // Small grace to allow shim's trap to POST /signal.
    std::thread::sleep(Duration::from_millis(50));
    log_compact(&format!(
        "aifo-coder: disconnect escalate: sending INT to exec_id={}",
        exec_id
    ));
    kill_in_container(runtime, container, exec_id, "INT", verbose);
    // In parallel, try to close the transient /run shell in the agent container, if known.
    if let Some(ac) = agent_container {
        kill_agent_shell_in_agent_container(runtime, ac, exec_id, verbose);
    }
    std::thread::sleep(Duration::from_millis(250));
    log_compact(&format!(
        "aifo-coder: disconnect escalate: sending TERM to exec_id={}",
        exec_id
    ));
    kill_in_container(runtime, container, exec_id, "TERM", verbose);
    std::thread::sleep(Duration::from_millis(750));
    log_compact(&format!(
        "aifo-coder: disconnect escalate: sending KILL to exec_id={}",
        exec_id
    ));
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
        // Keep STDIN open as well to behave like interactive docker exec (-it)
        spawn_args.insert(1, "-t".to_string());
        spawn_args.insert(1, "-i".to_string());
    }
    // User command slice after container name
    let user_slice: Vec<String> = exec_preview_args[idx + 1..].to_vec();
    let inner = shell_join(&user_slice);
    let script = format!(
        "set -e; export PATH=\"/usr/local/go/bin:/home/coder/.cargo/bin:/usr/local/cargo/bin:$PATH\"; export RUSTUP_NO_UPDATE_CHECK=1; export RUSTUP_SELF_UPDATE=0; export RUSTUP_USE_CURL=1; \
         if [ -f /workspace/corp-ca.crt ]; then export SSL_CERT_FILE=/workspace/corp-ca.crt; export CURL_CA_BUNDLE=/workspace/corp-ca.crt; export CARGO_HTTP_CAINFO=/workspace/corp-ca.crt; export REQUESTS_CA_BUNDLE=/workspace/corp-ca.crt; \
         elif [ -f /etc/ssl/certs/aifo-corp-ca.crt ]; then export SSL_CERT_FILE=/etc/ssl/certs/aifo-corp-ca.crt; export CURL_CA_BUNDLE=/etc/ssl/certs/aifo-corp-ca.crt; export CARGO_HTTP_CAINFO=/etc/ssl/certs/aifo-corp-ca.crt; export REQUESTS_CA_BUNDLE=/etc/ssl/certs/aifo-corp-ca.crt; fi; \
         eid=\"${{AIFO_EXEC_ID:-}}\"; if [ -z \"$eid\" ]; then exec {inner} 2>&1; fi; \
         d=\"${{HOME:-/home/coder}}/.aifo-exec/${{AIFO_EXEC_ID:-}}\"; mkdir -p \"$d\" 2>/dev/null || {{ d=\"/tmp/.aifo-exec/${{AIFO_EXEC_ID:-}}\"; mkdir -p \"$d\" || true; }}; \
         ( setsid sh -lc \"export PATH=\\\"/usr/local/go/bin:/home/coder/.cargo/bin:/usr/local/cargo/bin:\\$PATH\\\"; export RUSTUP_NO_UPDATE_CHECK=1; export RUSTUP_SELF_UPDATE=0; export RUSTUP_USE_CURL=1; \
           if [ -f /workspace/corp-ca.crt ]; then export SSL_CERT_FILE=/workspace/corp-ca.crt; export CURL_CA_BUNDLE=/workspace/corp-ca.crt; export CARGO_HTTP_CAINFO=/workspace/corp-ca.crt; export REQUESTS_CA_BUNDLE=/workspace/corp-ca.crt; \
           elif [ -f /etc/ssl/certs/aifo-corp-ca.crt ]; then export SSL_CERT_FILE=/etc/ssl/certs/aifo-corp-ca.crt; export CURL_CA_BUNDLE=/etc/ssl/certs/aifo-corp-ca.crt; export CARGO_HTTP_CAINFO=/etc/ssl/certs/aifo-corp-ca.crt; export REQUESTS_CA_BUNDLE=/etc/ssl/certs/aifo-corp-ca.crt; fi; \
           exec {inner} 2>&1\" ) & pg=$!; \
         printf \"%s\\n\" \"$pg\" > \"$d/pgid\" 2>/dev/null || true; wait \"$pg\"; rm -rf \"$d\" || true",
        inner = inner
    );
    spawn_args.push("sh".to_string());
    spawn_args.push("-c".to_string());
    spawn_args.push(script);
    spawn_args
}

#[cfg_attr(
    feature = "otel",
    instrument(
        level = "info",
        err,
        skip(verbose),
        fields(session_id = %session_id, verbose = %verbose)
    )
)]
pub fn toolexec_start_proxy(
    session_id: &str,
    verbose: bool,
) -> io::Result<(String, String, Arc<AtomicBool>, JoinHandle<()>)> {
    let runtime = container_runtime_path()?;
    if verbose {
        eprintln!(
            "aifo-coder: proxy build={} target={} profile={} rust={} ver={}",
            env!("AIFO_SHIM_BUILD_DATE"),
            env!("AIFO_SHIM_BUILD_TARGET"),
            env!("AIFO_SHIM_BUILD_PROFILE"),
            env!("AIFO_SHIM_BUILD_RUSTC"),
            env!("CARGO_PKG_VERSION")
        );
    }

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
            let listener = UnixListener::bind(&sock_path).map_err(|e| {
                io::Error::new(
                    e.kind(),
                    crate::display_for_toolchain_error(&crate::ToolchainError::Message(format!(
                        "proxy unix bind failed: {e}"
                    ))),
                )
            })?;
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
                let tool_cache =
                    std::sync::Arc::new(std::sync::Mutex::new(
                        HashMap::<(String, String), bool>::new(),
                    ));
                let exec_registry =
                    std::sync::Arc::new(std::sync::Mutex::new(HashMap::<String, String>::new()));
                let recent_signals =
                    std::sync::Arc::new(std::sync::Mutex::new(
                        HashMap::<String, std::time::Instant>::new(),
                    ));
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
                        let disable_user =
                            std_env::var("AIFO_TOOLEEXEC_DISABLE_USER").ok().as_deref()
                                == Some("1");
                        let ctx2 = ProxyCtx {
                            runtime: runtime_cl,
                            token: token_cl,
                            session: session_cl,
                            timeout_secs,
                            verbose,
                            agent_container: std_env::var("AIFO_CODER_CONTAINER_NAME").ok(),
                            uidgid: if cfg!(unix) && !disable_user {
                                Some((uid, gid))
                            } else {
                                None
                            },
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
    let listener = TcpListener::bind((bind_host, 0)).map_err(|e| {
        io::Error::new(
            e.kind(),
            crate::display_for_toolchain_error(&crate::ToolchainError::Message(format!(
                "proxy bind failed: {e}"
            ))),
        )
    })?;
    let addr = listener.local_addr().map_err(|e| {
        io::Error::new(
            e.kind(),
            crate::display_for_toolchain_error(&crate::ToolchainError::Message(format!(
                "proxy addr failed: {e}"
            ))),
        )
    })?;
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
        let tool_cache = std::sync::Arc::new(std::sync::Mutex::new(HashMap::<
            (String, String),
            bool,
        >::new()));
        let exec_registry =
            std::sync::Arc::new(std::sync::Mutex::new(HashMap::<String, String>::new()));
        let recent_signals = std::sync::Arc::new(std::sync::Mutex::new(HashMap::<
            String,
            std::time::Instant,
        >::new()));
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
                let disable_user =
                    std_env::var("AIFO_TOOLEEXEC_DISABLE_USER").ok().as_deref() == Some("1");
                let ctx2 = ProxyCtx {
                    runtime: runtime_cl,
                    token: token_cl,
                    session: session_cl,
                    timeout_secs,
                    verbose,
                    agent_container: std_env::var("AIFO_CODER_CONTAINER_NAME").ok(),
                    uidgid: if cfg!(unix) && !disable_user {
                        Some((uid, gid))
                    } else {
                        None
                    },
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
// Warm up rust toolchain once (per container) to suppress rustup channel sync chatter in streams.
static RUST_WARMED: Lazy<std::sync::Mutex<HashSet<String>>> =
    Lazy::new(|| std::sync::Mutex::new(HashSet::new()));

fn ensure_rust_toolchain_warm(
    runtime: &PathBuf,
    container: &str,
    uidgid: Option<(u32, u32)>,
    verbose: bool,
) {
    {
        let warmed = RUST_WARMED.lock().unwrap();
        if warmed.contains(container) {
            return;
        }
    }
    // docker exec [-u uid:gid] <container> sh -lc "rustc -V >/dev/null 2>&1 || true"
    let mut args: Vec<String> = vec!["docker".into(), "exec".into()];
    if let Some((uid, gid)) = uidgid {
        args.push("-u".into());
        args.push(format!("{uid}:{gid}"));
    }
    args.push(container.into());
    args.push("sh".into());
    args.push("-lc".into());
    args.push("rustc -V >/dev/null 2>&1 || true".into());

    if verbose {
        log_compact(&format!("aifo-coder: docker: {}", shell_join(&args)));
    }

    let mut cmd = Command::new(runtime);
    for a in &args[1..] {
        cmd.arg(a);
    }
    cmd.stdout(Stdio::null()).stderr(Stdio::null());
    let _ = cmd.status();

    let mut warmed = RUST_WARMED.lock().unwrap();
    warmed.insert(container.to_string());
}

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
        Err(e) => {
            match e.kind() {
                io::ErrorKind::InvalidInput => {
                    // e.g., too many headers
                    respond_plain(
                        stream,
                        "431 Request Header Fields Too Large",
                        86,
                        b"request headers too large\n",
                    );
                }
                io::ErrorKind::InvalidData => {
                    // e.g., Content-Length mismatch or malformed body
                    respond_plain(stream, "400 Bad Request", 86, ERR_BAD_REQUEST);
                }
                _ => {
                    respond_plain(stream, "400 Bad Request", 86, ERR_BAD_REQUEST);
                }
            }
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

    // Extract incoming trace context (if any) for propagation into shim/tool execs.
    #[cfg(feature = "otel")]
    let parent_cx = global::get_text_map_propagator(|prop| {
        prop.extract(&HeaderMapExtractor {
            headers: &req.headers,
        })
    });

    // Merge form/query
    let form = String::from_utf8_lossy(&req.body).to_string();
    let mut tool = String::new();
    let mut cwd = "/workspace".to_string();
    let mut argv: Vec<String> = Vec::new();
    let mut notif_cmd: String = String::new();
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
            "cmd" => notif_cmd = v,
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
        if verbose {
            let client = req.headers.get("x-aifo-client").cloned();
            let client_sfx = client
                .as_deref()
                .map(|c| format!(" client={}", c))
                .unwrap_or_default();
            log_compact(&format!(
                "aifo-coder: proxy notify parsed cmd={} argv={} cwd={}{}",
                notif_cmd,
                shell_join(&argv),
                cwd,
                client_sfx
            ));
        }
        let noauth = std_env::var("AIFO_NOTIFICATIONS_NOAUTH").ok().as_deref() == Some("1");
        if noauth {
            // Enforce X-Aifo-Proto: "2" even in noauth mode
            if req.headers.get("x-aifo-proto").map(|s| s.trim()) != Some("2") {
                respond_plain(
                    stream,
                    "426 Upgrade Required",
                    86,
                    b"unsupported notify protocol; expected 2\n",
                );
                let _ = stream.flush();
                return;
            }
            // In noauth mode, cmd is required (400 if missing)
            if notif_cmd.is_empty() {
                respond_plain(stream, "400 Bad Request", 86, ERR_BAD_REQUEST);
                let _ = stream.flush();
                return;
            }
            let notif_to = std_env::var("AIFO_NOTIFICATIONS_TIMEOUT_SECS")
                .ok()
                .and_then(|s| s.parse::<u64>().ok())
                .filter(|&v| v > 0)
                .unwrap_or(if timeout_secs == 0 { 5 } else { timeout_secs });
            let started = std::time::Instant::now();
            match notifications::notifications_handle_request(&notif_cmd, &argv, verbose, notif_to)
            {
                Ok((status_code, body_out)) => {
                    log_request_result(verbose, &notif_cmd, "notify", status_code, &started);
                    // Tiny nudge to improve host-log vs agent-UI ordering
                    let nudge_ms = std_env::var("AIFO_NOTIFY_PROXY_NUDGE_MS")
                        .ok()
                        .and_then(|s| s.parse::<u64>().ok())
                        .unwrap_or(15);
                    if nudge_ms > 0 {
                        std::thread::sleep(Duration::from_millis(nudge_ms.min(100)));
                    }
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
                Err(notif_err) => {
                    match notif_err {
                        notifications::NotifyError::Policy(reason) => {
                            let mut body = reason.into_bytes();
                            body.push(b'\n');
                            respond_plain(stream, "403 Forbidden", 86, &body);
                        }
                        notifications::NotifyError::ExecSpawn(reason) => {
                            let mut body = reason.into_bytes();
                            body.push(b'\n');
                            respond_plain(stream, "500 Internal Server Error", 86, &body);
                        }
                        notifications::NotifyError::Timeout => {
                            respond_plain(stream, "408 Request Timeout", 124, b"timeout\n");
                        }
                    }
                    let _ = stream.flush();
                    return;
                }
            }
        }

        match auth_res {
            auth::AuthResult::Authorized { proto } => {
                if !matches!(proto, auth::Proto::V2) {
                    respond_plain(
                        stream,
                        "426 Upgrade Required",
                        86,
                        b"unsupported notify protocol; expected 2\n",
                    );
                    let _ = stream.flush();
                    return;
                }
                // After auth+proto checks, require cmd (400 if missing)
                if notif_cmd.is_empty() {
                    respond_plain(stream, "400 Bad Request", 86, ERR_BAD_REQUEST);
                    let _ = stream.flush();
                    return;
                }
                let notif_to = std_env::var("AIFO_NOTIFICATIONS_TIMEOUT_SECS")
                    .ok()
                    .and_then(|s| s.parse::<u64>().ok())
                    .filter(|&v| v > 0)
                    .unwrap_or(if timeout_secs == 0 { 5 } else { timeout_secs });
                let started = std::time::Instant::now();
                match notifications::notifications_handle_request(
                    &notif_cmd, &argv, verbose, notif_to,
                ) {
                    Ok((status_code, body_out)) => {
                        log_request_result(verbose, &notif_cmd, "notify", status_code, &started);
                        // Tiny nudge to improve host-log vs agent-UI ordering
                        let nudge_ms = std_env::var("AIFO_NOTIFY_PROXY_NUDGE_MS")
                            .ok()
                            .and_then(|s| s.parse::<u64>().ok())
                            .unwrap_or(15);
                        if nudge_ms > 0 {
                            std::thread::sleep(Duration::from_millis(nudge_ms.min(100)));
                        }
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
                    Err(notif_err) => {
                        match notif_err {
                            notifications::NotifyError::Policy(reason) => {
                                let mut body = reason.into_bytes();
                                body.push(b'\n');
                                respond_plain(stream, "403 Forbidden", 86, &body);
                            }
                            notifications::NotifyError::ExecSpawn(reason) => {
                                let mut body = reason.into_bytes();
                                body.push(b'\n');
                                respond_plain(stream, "500 Internal Server Error", 86, &body);
                            }
                            notifications::NotifyError::Timeout => {
                                respond_plain(stream, "408 Request Timeout", 124, b"timeout\n");
                            }
                        }
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
                let container =
                    if let Some(name) = exec_registry.lock().unwrap().get(&exec_id).cloned() {
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
                    log_compact(&format!(
                        "aifo-coder: proxy signal: exec_id={} sig={}",
                        exec_id, sig
                    ));
                }
                // Record recent /signal for this exec immediately to suppress duplicate disconnect escalation
                {
                    let mut rs = recent_signals.lock().unwrap();
                    rs.insert(exec_id.clone(), std::time::Instant::now());
                }
                kill_in_container(&ctx.runtime, &container, &exec_id, &sig, verbose);
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

    let (authorized, mut proto_v2) = match auth_res {
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
    // Env override: AIFO_PROXY_PROTO=1 forces buffered (v1), =2 forces streaming (v2)
    if let Ok(v) = std_env::var("AIFO_PROXY_PROTO") {
        match v.trim() {
            "1" => {
                proto_v2 = false;
            }
            "2" => {
                proto_v2 = true;
            }
            _ => {}
        }
    }

    // Route to sidecar kind and enforce allowlist
    let selected_kind = {
        let mut cache = tool_cache.lock().unwrap();
        select_kind_for_tool(session, &tool, timeout_secs, &mut cache)
    };
    let kind = selected_kind.as_str();
    let allow = sidecar_allowlist(kind);
    if !allow.contains(&tool.as_str()) {
        respond_plain(stream, "403 Forbidden", 86, ERR_FORBIDDEN);
        let _ = stream.flush();
        return;
    }

    let name = sidecar::sidecar_container_name(kind, session);

    // Build OpenTelemetry span for this proxy request (after routing is known).
    #[cfg(feature = "otel")]
    let _proxy_span_guard = {
        let cwd_field = if telemetry_pii_enabled() {
            cwd.clone()
        } else {
            hash_string_hex(&cwd)
        };
        let span = info_span!(
            "proxy_request",
            tool = %tool,
            kind = %kind,
            arg_count = argv.len(),
            cwd = %cwd_field,
            session_id = %session
        );
        span.set_parent(parent_cx);
        span.entered()
    };

    if !container_exists(&name) {
        let msg = format!(
            "\r\ntool '{}' not available in running sidecars; start an appropriate toolchain (e.g., --toolchain c-cpp or --toolchain rust)\n",
            tool
        );
        respond_plain(stream, "409 Conflict", 86, msg.as_bytes());
        let _ = stream.flush();
        return;
    }

    // Rust sidecar can emit rustup sync on first use; warm up silently off-stream
    if kind == "rust" {
        ensure_rust_toolchain_warm(&ctx.runtime, &name, uidgid, verbose);
    }

    let pwd = std::path::PathBuf::from(cwd);
    // Optional hardening: detect unreadable /workspace for current uid:gid and surface a helpful hint
    if pwd.as_path() == std::path::Path::new("/workspace") {
        if let Some(hint) = workspace_access_hint(&ctx.runtime, &name, uidgid, verbose) {
            respond_plain(stream, "409 Conflict", 86, &hint);
            let _ = stream.flush();
            return;
        }
    }
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
        // For python sidecar, prefer python3 (present in python:3.12-slim)
        if kind == "python" && tool == "python" && !full_args.is_empty() {
            full_args[0] = "python3".to_string();
        }
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
        log_compact(&format!(
            "aifo-coder: proxy docker: {}",
            shell_join(&exec_preview_args)
        ));
    }

    if proto_v2 {
        // Streaming (v2)
        if verbose {
            log_compact("aifo-coder: proxy exec: proto=v2 (streaming)");
        }
        let started = std::time::Instant::now();

        let verbose_level: u32 = std_env::var("AIFO_TOOLCHAIN_VERBOSE")
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(1);
        let preview_bytes: usize = std_env::var("AIFO_PROXY_LOG_PREVIEW_BYTES")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(120);

        let use_tty = std_env::var("AIFO_TOOLEEXEC_TTY").ok().as_deref() == Some("1");
        if verbose {
            log_compact(&format!("aifo-coder: proxy stream: use_tty={}", use_tty));
        }
        #[cfg_attr(not(feature = "otel"), allow(unused_mut))]
        let mut spawn_args = build_exec_args_with_wrapper(&name, &exec_preview_args, use_tty);

        // Inject W3C traceparent into shim environment for downstream propagation.
        #[cfg(feature = "otel")]
        {
            // Create a temporary header map and inject current span context.
            let mut headers = std::collections::HashMap::<String, String>::new();
            global::get_text_map_propagator(|prop| {
                prop.inject_context(
                    &Context::current(),
                    &mut HeaderMapInjector {
                        headers: &mut headers,
                    },
                );
            });
            if let Some(traceparent_val) = headers
                .get("traceparent")
                .cloned()
                .or_else(|| headers.get("Traceparent").cloned())
            {
                // Ensure we pass it through as environment to the shim.
                // The wrapper always uses "docker exec ... sh -c <script>", so we can prefix
                // an export of TRACEPARENT before the main script.
                if let Some(last) = spawn_args.last_mut() {
                    let original_script = last.clone();
                    let injected = format!(
                        "export TRACEPARENT={q}{v}{q}; {orig}",
                        q = "'",
                        v = traceparent_val,
                        orig = original_script,
                    );
                    *last = injected;
                }
            }
        }
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
                #[cfg(feature = "otel")]
                {
                    use opentelemetry::trace::Status;
                    use tracing_opentelemetry::OpenTelemetrySpanExt;
                    let sp = tracing::Span::current();
                    sp.set_status(Status::error("spawn_failed"));
                }
                log_request_result(verbose, &tool, kind, 86, &started);
                respond_plain(stream, "500 Internal Server Error", 86, &b);
                let _ = stream.flush();
                return;
            }
        };
        if verbose {
            log_compact(&format!(
                "aifo-coder: proxy exec: spawned child pid={}",
                child.id()
            ));
        }
        // Streaming log boundary: when we just wrote payload to client, next log must re-anchor with '\r'
        let mut logger = StreamLogger::new(verbose);

        // Optional max-runtime escalation watcher
        let done = Arc::new(AtomicBool::new(false));
        let timed_out = Arc::new(AtomicBool::new(false));
        if timeout_secs > 0 {
            let done_cl = done.clone();
            let timed_out_cl = timed_out.clone();
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
                    if sig == "INT" {
                        timed_out_cl.store(true, std::sync::atomic::Ordering::SeqCst);
                    }
                    kill_in_container(&runtime_cl, &container_cl, &exec_id_cl, sig, verbose_cl);
                }
            });
        }

        // Defer sending prelude until the first chunk is available to avoid early client disconnects
        logger.boundary_log("aifo-coder: proxy stream: deferring prelude until first chunk");

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

        // Stream stdout (bounded channel to limit memory under client stalls)
        let use_unbounded = std_env::var("AIFO_PROXY_UNBOUNDED").ok().as_deref() == Some("1");
        // Bounded channel capacity (configurable via AIFO_PROXY_CHANNEL_CAP; default 64)
        let cap: usize = std_env::var("AIFO_PROXY_CHANNEL_CAP")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .filter(|&v| v > 0)
            .unwrap_or(256);
        let dropped_count = Arc::new(AtomicUsize::new(0));
        let drop_warned = Arc::new(AtomicBool::new(false));

        // Create channel (bounded by default; unbounded when AIFO_PROXY_UNBOUNDED=1)
        #[derive(Clone)]
        enum TxKind {
            Unbounded(std::sync::mpsc::Sender<Vec<u8>>),
            Bounded(std::sync::mpsc::SyncSender<Vec<u8>>),
        }
        let (tx_kind, rx) = if use_unbounded {
            let (t, r) = std::sync::mpsc::channel::<Vec<u8>>();
            (TxKind::Unbounded(t), r)
        } else {
            let (t, r) = std::sync::mpsc::sync_channel::<Vec<u8>>(cap);
            (TxKind::Bounded(t), r)
        };

        if let Some(mut so) = child.stdout.take() {
            let txo = match &tx_kind {
                TxKind::Unbounded(s) => TxKind::Unbounded(s.clone()),
                TxKind::Bounded(s) => TxKind::Bounded(s.clone()),
            };
            let verbose_cl = verbose;
            let verbose_level_cl = verbose_level;
            let preview_bytes_cl = preview_bytes;
            let drop_warned_cl = drop_warned.clone();
            let dropped_count_cl = dropped_count.clone();
            std::thread::spawn(move || {
                let mut buf = [0u8; 8192];
                loop {
                    match so.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            if verbose_cl && verbose_level_cl >= 2 {
                                let mut prev =
                                    String::from_utf8_lossy(&buf[..n.min(preview_bytes_cl)])
                                        .into_owned();
                                prev = prev.replace("\r", "\\r").replace("\n", "\\n");
                                log_compact(&format!(
                                    "aifo-coder: proxy stream: stdout reader read {} bytes preview='{}'",
                                    n, prev
                                ));
                            }
                            let chunk = buf[..n].to_vec();
                            match txo {
                                TxKind::Unbounded(ref s) => {
                                    let _ = s.send(chunk);
                                }
                                TxKind::Bounded(ref s) => {
                                    // Best-effort small backoff attempts before dropping under backpressure
                                    let mut msg = chunk;
                                    let mut attempts = 0usize;
                                    loop {
                                        match s.try_send(msg) {
                                            Ok(()) => break,
                                            Err(std::sync::mpsc::TrySendError::Full(c)) => {
                                                attempts += 1;
                                                if attempts <= 2 {
                                                    std::thread::sleep(
                                                        std::time::Duration::from_millis(5),
                                                    );
                                                    msg = c;
                                                    continue;
                                                }
                                                if !drop_warned_cl
                                                    .swap(true, std::sync::atomic::Ordering::SeqCst)
                                                {
                                                    log_compact(
                                                        "aifo-coder: proxy stream: dropping output (backpressure)",
                                                    );
                                                }
                                                let _ = dropped_count_cl.fetch_add(
                                                    1,
                                                    std::sync::atomic::Ordering::SeqCst,
                                                );
                                                break;
                                            }
                                            Err(std::sync::mpsc::TrySendError::Disconnected(
                                                _c,
                                            )) => break,
                                        }
                                    }
                                }
                            }
                        }
                        Err(_) => break,
                    }
                }
            });
        }
        drop(tx_kind);

        // Stream until EOF or write error
        #[allow(unused_assignments)]
        let mut write_failed = false;
        let mut timeout_chunk_emitted = false;
        #[allow(unused_assignments)]
        let mut prelude_sent = false;
        let mut wrote_any_chunk = false;
        #[allow(unused_assignments)]
        let mut prelude_failed = false;
        let mut first_chunk_write_failed = false;
        let mut first_wait_logged = false;
        let mut total_bytes: usize = 0;
        let mut chunk_count_log: usize = 0;
        loop {
            // Emit timeout chunk once when INT has been sent
            if !timeout_chunk_emitted && timed_out.load(std::sync::atomic::Ordering::SeqCst) {
                // Ensure prelude is sent before emitting any chunk
                if !prelude_sent {
                    if let Err(e) = respond_chunked_prelude(stream, Some(&exec_id)) {
                        prelude_failed = true;
                        let _ = prelude_failed;
                        write_failed = true;
                        let _ = write_failed;
                        if verbose {
                            logger.boundary_log(&format!(
                                "aifo-coder: proxy stream: prelude write failed: kind={:?} errno={:?}",
                                e.kind(),
                                e.raw_os_error()
                            ));
                        }
                        break;
                    }
                    prelude_sent = true;
                    let _ = prelude_sent;
                    logger.boundary_log("aifo-coder: proxy stream: prelude sent");
                }
                let _ = respond_chunked_write_chunk(stream, b"aifo-coder proxy timeout\n");
                timeout_chunk_emitted = true;
            }
            match rx.recv_timeout(Duration::from_millis(200)) {
                Ok(chunk) => {
                    if !prelude_sent {
                        // Ensure this log starts at column 0
                        logger.set_boundary();
                        logger.boundary_log(
                            "aifo-coder: proxy stream: sending prelude before first chunk",
                        );
                        if let Err(e) = respond_chunked_prelude(stream, Some(&exec_id)) {
                            prelude_failed = true;
                            let _ = prelude_failed;
                            write_failed = true;
                            let _ = write_failed;
                            if verbose {
                                logger.boundary_log(&format!(
                                    "aifo-coder: proxy stream: prelude write failed: kind={:?} errno={:?}",
                                    e.kind(),
                                    e.raw_os_error()
                                ));
                            }
                            break;
                        }
                        prelude_sent = true;
                        let _ = prelude_sent;
                        logger.boundary_log("aifo-coder: proxy stream: prelude sent");
                    }
                    if verbose && verbose_level >= 2 {
                        let mut prev =
                            String::from_utf8_lossy(&chunk[..chunk.len().min(preview_bytes)])
                                .into_owned();
                        prev = prev.replace("\r", "\\r").replace("\n", "\\n");
                        logger.boundary_log(&format!(
                            "aifo-coder: proxy stream: chunk size={} preview='{}'",
                            chunk.len(),
                            prev
                        ));
                    }
                    // Insert a visual separator line before the very first streamed payload line
                    if !wrote_any_chunk {
                        eprint!("\n\r");
                        let _ = io::stderr().flush();
                    }
                    if let Err(e) = respond_chunked_write_chunk(stream, &chunk) {
                        if !wrote_any_chunk {
                            first_chunk_write_failed = true;
                        }
                        write_failed = true;
                        if verbose {
                            logger.boundary_log(&format!(
                                "aifo-coder: proxy stream: chunk write failed: kind={:?} errno={:?}",
                                e.kind(),
                                e.raw_os_error()
                            ));
                        }
                        break;
                    } else {
                        wrote_any_chunk = true;
                        total_bytes = total_bytes.saturating_add(chunk.len());
                        chunk_count_log = chunk_count_log.saturating_add(1);
                        logger.set_boundary();
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    if !prelude_sent && !first_wait_logged && verbose {
                        logger.boundary_log("aifo-coder: proxy stream: waiting for first chunk...");
                        first_wait_logged = true;
                    }
                    continue;
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    if !prelude_sent && !wrote_any_chunk && verbose {
                        logger.boundary_log(
                            "aifo-coder: proxy stream: stdout closed before any data",
                        );
                    }
                    break;
                }
            }
        }

        // Verbose metric: how many chunks dropped under backpressure
        if verbose && dropped_count.load(std::sync::atomic::Ordering::SeqCst) > 0 {
            let line = format!(
                "aifo-coder: proxy stream: dropped {} chunk(s)",
                dropped_count.load(std::sync::atomic::Ordering::SeqCst)
            );
            logger.boundary_log(&line);
        }

        if write_failed {
            // Detail why the write failed when verbose
            if verbose {
                if prelude_failed {
                    logger.boundary_log("aifo-coder: proxy stream: prelude write failed; client closed before reading headers");
                } else if prelude_sent && !wrote_any_chunk && first_chunk_write_failed {
                    logger.boundary_log("aifo-coder: proxy stream: first chunk write failed; client closed before first payload");
                } else {
                    let line = format!(
                        "aifo-coder: proxy stream: write failed after {} dropped chunk(s)",
                        dropped_count.load(std::sync::atomic::Ordering::SeqCst)
                    );
                    logger.boundary_log(&line);
                }
            }
            // Emit a single drop-warning and mark at least one dropped chunk for metrics.
            if !drop_warned.swap(true, std::sync::atomic::Ordering::SeqCst) {
                logger.boundary_log("aifo-coder: proxy stream: dropping output (backpressure)");
            }
            // Ensure a dropped-counter line appears even if producer didn't drop due to channel backpressure.
            let dropped_now = dropped_count
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
                .saturating_add(1);
            if verbose {
                logger.boundary_log(&format!(
                    "aifo-coder: proxy stream: dropped {} chunk(s)",
                    dropped_now
                ));
            }
            // Client disconnected: allow a brief grace window for /signal to arrive, then decide suppression.
            let suppress = {
                let grace_ms: u64 = std_env::var("AIFO_PROXY_SIGNAL_GRACE_MS")
                    .ok()
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(500);
                let deadline = std::time::Instant::now() + Duration::from_millis(grace_ms);
                loop {
                    let seen = {
                        let rs = recent_signals.lock().unwrap();
                        rs.get(&exec_id)
                            .map(|ts| ts.elapsed() < Duration::from_millis(2300))
                            .unwrap_or(false)
                    };
                    if seen || std::time::Instant::now() >= deadline {
                        break seen;
                    }
                    std::thread::sleep(Duration::from_millis(25));
                }
            };
            if suppress {
                log_disconnect();
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

        // Emit timeout chunk late if not already sent
        if !timeout_chunk_emitted && timed_out.load(std::sync::atomic::Ordering::SeqCst) {
            let _ = respond_chunked_write_chunk(stream, b"aifo-coder proxy timeout\n");
        }

        let mut code = child.wait().ok().and_then(|s| s.code()).unwrap_or(1);
        if timeout_secs > 0 && timed_out.load(std::sync::atomic::Ordering::SeqCst) {
            code = 124;
        }
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

        #[cfg(feature = "otel")]
        {
            use opentelemetry::trace::Status;
            use tracing_opentelemetry::OpenTelemetrySpanExt;
            let secs = started.elapsed().as_secs_f64();
            let result = if timeout_secs > 0 && timed_out.load(std::sync::atomic::Ordering::SeqCst)
            {
                "timeout"
            } else if code == 0 {
                "ok"
            } else {
                "err"
            };
            // Set span status on errors/timeouts (concise message).
            let sp = tracing::Span::current();
            if result == "timeout" {
                sp.set_status(Status::error("proxy_timeout"));
            } else if result == "err" {
                sp.set_status(Status::error(format!("exit_code={}", code)));
            }
            crate::telemetry::metrics::record_proxy_exec_duration(&tool, secs);
            crate::telemetry::metrics::record_proxy_request(&tool, result);
        }
        if verbose {
            logger.boundary_log(&format!(
                "aifo-coder: proxy stream: totals bytes={} chunks={}",
                total_bytes, chunk_count_log
            ));
        }
        // If no prelude was sent (no payload), send it now to ensure a valid chunked response
        if !prelude_sent {
            if let Err(e) = respond_chunked_prelude(stream, Some(&exec_id)) {
                prelude_failed = true;
                let _ = prelude_failed;
                write_failed = true;
                let _ = write_failed;
                if verbose {
                    logger.boundary_log(&format!(
                        "aifo-coder: proxy stream: prelude write failed before trailer: kind={:?} errno={:?}",
                        e.kind(),
                        e.raw_os_error()
                    ));
                }
                // Fall through and attempt to write trailer; client may still accept it
            } else {
                prelude_sent = true;
                let _ = prelude_sent;
                logger.boundary_log("aifo-coder: proxy stream: prelude sent");
            }
        }
        if let Err(e) = respond_chunked_trailer(stream, code) {
            if !drop_warned.swap(true, std::sync::atomic::Ordering::SeqCst) {
                logger.boundary_log("aifo-coder: proxy stream: dropping output (backpressure)");
            }
            // Also show a dropped-counter line even if no producer-side drop occurred.
            let dropped_now = dropped_count
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
                .saturating_add(1);
            if verbose {
                logger.boundary_log(&format!(
                    "aifo-coder: proxy stream: dropped {} chunk(s)",
                    dropped_now
                ));
                logger.boundary_log(&format!(
                    "aifo-coder: proxy stream: trailer write failed: kind={:?} errno={:?}",
                    e.kind(),
                    e.raw_os_error()
                ));
            }
            // Canonical disconnect signal for acceptance tests and operator clarity.
            log_disconnect();
            if let Some(ac) = ctx.agent_container.as_deref() {
                kill_agent_shell_in_agent_container(&ctx.runtime, ac, &exec_id, verbose);
            }
        }
        return;
    }

    // Buffered (v1)
    if verbose {
        log_stderr_and_file("aifo-coder: proxy exec: proto=v1 (buffered)");
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
            #[cfg(feature = "otel")]
            {
                use opentelemetry::trace::Status;
                use tracing_opentelemetry::OpenTelemetrySpanExt;
                let sp = tracing::Span::current();
                sp.set_status(Status::error("spawn_failed"));
            }
            log_request_result(verbose, &tool, kind, 86, &started);
            respond_plain(stream, "500 Internal Server Error", 86, &b);
            let _ = stream.flush();
            return;
        }
    };

    // Optional max-runtime escalation watcher
    let done = Arc::new(AtomicBool::new(false));
    let timed_out = Arc::new(AtomicBool::new(false));
    if timeout_secs > 0 {
        let done_cl = done.clone();
        let timed_out_cl = timed_out.clone();
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
                if sig == "INT" {
                    timed_out_cl.store(true, std::sync::atomic::Ordering::SeqCst);
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

    #[cfg(feature = "otel")]
    {
        use opentelemetry::trace::Status;
        use tracing_opentelemetry::OpenTelemetrySpanExt;
        let secs = started.elapsed().as_secs_f64();
        let result = if timeout_secs > 0 && timed_out.load(std::sync::atomic::Ordering::SeqCst) {
            "timeout"
        } else if code == 0 {
            "ok"
        } else {
            "err"
        };
        let sp = tracing::Span::current();
        if result == "timeout" {
            sp.set_status(Status::error("proxy_timeout"));
        } else if result == "err" {
            sp.set_status(Status::error(format!("exit_code={}", code)));
        }
        crate::telemetry::metrics::record_proxy_exec_duration(&tool, secs);
        crate::telemetry::metrics::record_proxy_request(&tool, result);
    }
    // If watcher timed out (on initial INT), map to 504 with exit code 124
    if timeout_secs > 0 && timed_out.load(std::sync::atomic::Ordering::SeqCst) {
        respond_plain(stream, "504 Gateway Timeout", 124, b"timeout\n");
        let _ = stream.flush();
        return;
    }
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
