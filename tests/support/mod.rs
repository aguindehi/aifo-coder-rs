/*!
Test support helpers shared across integration tests.

These helpers do not print skip messages themselves so tests can preserve their
existing "skipping: ..." outputs verbatim.
*/

use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

/// Return true if `git` is available on PATH.
#[allow(dead_code)]
pub fn have_git() -> bool {
    Command::new("git")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Cross-platform which() helper.
/// On Windows uses `where`, on other platforms uses `which`.
#[allow(dead_code)]
pub fn which(bin: &str) -> Option<PathBuf> {
    #[cfg(windows)]
    let cmd = "where";
    #[cfg(not(windows))]
    let cmd = "which";

    Command::new(cmd)
        .arg(bin)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                let s = String::from_utf8_lossy(&o.stdout);
                // Take first non-empty line
                s.lines()
                    .map(|l| l.trim())
                    .find(|l| !l.is_empty())
                    .map(PathBuf::from)
            } else {
                None
            }
        })
}

#[allow(dead_code)]
pub fn port_from_http_url(url: &str) -> u16 {
    let after = url.split("://").nth(1).unwrap_or(url);
    let host_port = after.split('/').next().unwrap_or(after);
    host_port
        .rsplit(':')
        .next()
        .unwrap_or("0")
        .parse()
        .unwrap_or(0)
}

/// Minimal raw HTTP POST helper over TCP returning (status, headers, body).
#[allow(dead_code)]
pub fn http_post_tcp(
    port: u16,
    headers: &[(&str, &str)],
    body_kv: &[(&str, &str)],
) -> (u16, String, Vec<u8>) {
    http_post_form_tcp(port, "/exec", headers, body_kv)
}

/// Minimal raw HTTP sender over TCP returning the full response as a String.
#[allow(dead_code)]
pub fn http_send_raw(port: u16, request: &str) -> String {
    use std::io::{Read, Write};
    use std::net::TcpStream;
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect failed");
    stream.write_all(request.as_bytes()).expect("write failed");
    let mut buf = Vec::new();
    let _ = stream.read_to_end(&mut buf);
    String::from_utf8_lossy(&buf).to_string()
}

/// Return default Node image for tests, from env or fallback to node:22-bookworm-slim
#[allow(dead_code)]
pub fn default_node_test_image() -> String {
    std::env::var("AIFO_CODER_TEST_NODE_IMAGE")
        .unwrap_or_else(|_| "node:22-bookworm-slim".to_string())
}

/// Return default Python image for tests, from env or fallback to python:3.12-slim
#[allow(dead_code)]
pub fn default_python_test_image() -> String {
    std::env::var("AIFO_CODER_TEST_PY_IMAGE")
        .or_else(|_| std::env::var("AIFO_CODER_TEST_PYTHON_IMAGE"))
        .unwrap_or_else(|_| "python:3.12-slim".to_string())
}

/// Initialize a git repository at `dir` and set a default user identity.
/// Idempotent: safe to call when repo already exists.
#[allow(dead_code)]
pub fn init_repo_with_default_user(dir: &Path) -> io::Result<()> {
    std::fs::create_dir_all(dir)?;
    // git init (ignore if already a repo)
    let _ = Command::new("git")
        .args(["init"])
        .current_dir(dir)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    // Configure default identity best-effort
    let _ = Command::new("git")
        .args(["config", "user.name", "AIFO Test"])
        .current_dir(dir)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    let _ = Command::new("git")
        .args(["config", "user.email", "aifo@example.com"])
        .current_dir(dir)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    Ok(())
}

#[allow(dead_code)]
pub fn urlencode(s: &str) -> String {
    urlencoding::encode(s).into_owned()
}

#[allow(dead_code)]
pub fn docker_runtime() -> Option<PathBuf> {
    aifo_coder::container_runtime_path().ok()
}

/// Return true if a Docker image is present locally (without pulling).
#[allow(dead_code)]
pub fn docker_image_present(runtime: &Path, image: &str) -> bool {
    aifo_coder::image_exists(runtime, image)
}

#[allow(dead_code)]
pub fn unique_name(prefix: &str) -> String {
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{prefix}-{pid}-{nanos}")
}

#[allow(dead_code)]
pub fn stop_container(runtime: &Path, name: &str) {
    let _ = Command::new(runtime)
        .args(["stop", "--time", "1", name])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

#[allow(dead_code)]
pub fn docker_exec_sh(runtime: &Path, name: &str, script: &str) -> (i32, String) {
    if let Err(e) = aifo_coder::validate_docker_exec_sh_script(script) {
        return (1, e);
    }

    let mut cmd = Command::new(runtime);
    cmd.arg("exec")
        .arg(name)
        .arg("/bin/sh")
        .arg("-c")
        .arg(script);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    match cmd.output() {
        Ok(o) => {
            let out = String::from_utf8_lossy(&o.stdout).to_string()
                + &String::from_utf8_lossy(&o.stderr).to_string();
            (o.status.code().unwrap_or(1), out)
        }
        Err(e) => (1, format!("exec failed: {e}")),
    }
}

#[allow(dead_code)]
pub fn wait_for_config_copied(runtime: &Path, name: &str) -> bool {
    let script = aifo_coder::ShellScript::new()
        .push(
            r#"if [ -f "$HOME/.aifo-config/.copied" ] || [ -d "$HOME/.aifo-config" ]; then echo READY; fi"#
                .to_string(),
        )
        .build()
        .unwrap_or_else(|_| String::new());
    for _ in 0..50 {
        let (_ec, out) = docker_exec_sh(runtime, name, &script);
        if out.contains("READY") {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

#[allow(dead_code)]
pub fn http_post_form_tcp(
    port: u16,
    path: &str,
    headers: &[(&str, &str)],
    body_kv: &[(&str, &str)],
) -> (u16, String, Vec<u8>) {
    use std::io::{Read, Write};
    use std::net::TcpStream;

    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect failed");

    // Build urlencoded body
    let mut body = String::new();
    for (i, (k, v)) in body_kv.iter().enumerate() {
        if i > 0 {
            body.push('&');
        }
        body.push_str(&format!("{}={}", urlencode(k), urlencode(v)));
    }

    // Build request
    let mut req = format!(
        "POST {path} HTTP/1.1\r\nHost: host.docker.internal\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    for (k, v) in headers {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    req.push_str("\r\n");
    req.push_str(&body);

    stream.write_all(req.as_bytes()).expect("write failed");

    let mut buf = Vec::new();
    let mut tmp = [0u8; 1024];
    loop {
        match stream.read(&mut tmp) {
            Ok(0) => break,
            Ok(n) => buf.extend_from_slice(&tmp[..n]),
            Err(_) => break,
        }
    }

    // Split headers/body using the library helper (CRLFCRLF or LFLF)
    let mut status: u16 = 0;
    let headers_s: String;
    let body_out: Vec<u8>;

    if let Some(hend) = aifo_coder::find_header_end(&buf) {
        let h = &buf[..hend];
        headers_s = String::from_utf8_lossy(h).to_string();
        if let Some(line) = headers_s.lines().next() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                status = parts[1].parse::<u16>().unwrap_or(0);
            }
        }
        body_out = buf[hend..].to_vec();
    } else {
        headers_s = String::from_utf8_lossy(&buf).to_string();
        body_out = Vec::new();
    }

    (status, headers_s, body_out)
}

#[cfg(unix)]
/// Capture stdout while running `f` using a pipe, returning the captured text.
/// Intended for integration tests; mirrors repeated inline helpers.
#[allow(dead_code)]
pub fn capture_stdout<F: FnOnce()>(f: F) -> String {
    use nix::libc::STDOUT_FILENO;
    use nix::unistd::{close, dup, dup2, pipe, read};
    use std::io::Write;
    use std::os::fd::AsRawFd;

    // Create a pipe and redirect stdout to its write end
    let (r_fd, w_fd) = match pipe() {
        Ok(p) => p,
        Err(_) => return String::new(),
    };

    // Duplicate current stdout
    let saved = match dup(STDOUT_FILENO) {
        Ok(fd) => fd,
        Err(_) => {
            drop(r_fd);
            drop(w_fd);
            return String::new();
        }
    };

    // Redirect stdout to the pipe writer
    if dup2(w_fd.as_raw_fd(), STDOUT_FILENO).is_err() {
        let _ = close(saved);
        drop(r_fd);
        drop(w_fd);
        return String::new();
    }

    // Run the function while stdout is redirected
    f();

    // Flush Rust stdio buffers, restore stdout, and close writer end
    let _ = std::io::stdout().flush();
    let _ = dup2(saved, STDOUT_FILENO);
    let _ = close(saved);
    drop(w_fd);

    // Read captured bytes from the pipe reader
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    loop {
        match read(r_fd.as_raw_fd(), &mut tmp) {
            Ok(0) => break,
            Ok(n) => buf.extend_from_slice(&tmp[..n]),
            Err(_) => break,
        }
    }
    drop(r_fd);

    String::from_utf8_lossy(&buf).to_string()
}

#[allow(dead_code)]
/// Return true if running inside the macos-cross-rust-builder image (or explicitly opted in).
pub fn should_run_macos_cross() -> bool {
    if std::env::var("AIFO_E2E_MACOS_CROSS").ok().as_deref() == Some("1") {
        return true;
    }
    let have_oa64 = std::path::Path::new("/opt/osxcross/target/bin/oa64-clang").is_file();
    let have_cargo = std::path::Path::new("/usr/local/cargo/bin/cargo").is_file();
    if !(have_oa64 && have_cargo) {
        return false;
    }
    if std::path::Path::new("/opt/osxcross/SDK/SDK_DIR.txt").is_file() {
        return true;
    }
    if let Ok(rd) = std::fs::read_dir("/opt/osxcross/target/SDK") {
        for ent in rd.flatten() {
            // Own the string to avoid borrowing from a temporary OsString (E0716).
            let s = ent.file_name().to_string_lossy().into_owned();
            if s.starts_with("MacOSX") && s.ends_with(".sdk") {
                return true;
            }
        }
    }
    false
}

/// RAII guard for temporarily setting/unsetting environment variables.
#[derive(Debug)]
pub struct EnvGuard {
    saved: Vec<(String, Option<String>)>,
}

impl EnvGuard {
    #[allow(dead_code)]
    pub fn new() -> Self {
        EnvGuard { saved: Vec::new() }
    }

    pub fn set<K: Into<String>, V: Into<String>>(mut self, key: K, val: V) -> Self {
        let k = key.into();
        if !self.saved.iter().any(|(kk, _)| kk == &k) {
            self.saved.push((k.clone(), std::env::var(&k).ok()));
        }
        std::env::set_var(&k, val.into());
        self
    }

    #[allow(dead_code)]
    pub fn remove<K: Into<String>>(mut self, key: K) -> Self {
        let k = key.into();
        if !self.saved.iter().any(|(kk, _)| kk == &k) {
            self.saved.push((k.clone(), std::env::var(&k).ok()));
        }
        std::env::remove_var(&k);
        self
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (k, v) in self.saved.drain(..).rev() {
            if let Some(val) = v {
                std::env::set_var(k, val);
            } else {
                std::env::remove_var(k);
            }
        }
    }
}

/// Opt into notifications safe-dir overrides for tests that execute stub binaries from temp dirs.
#[allow(dead_code)]
pub fn notifications_allow_test_exec_from(dir: &Path) -> EnvGuard {
    // Use canonical form of the directory (best-effort) to match parse_notif_cfg canonicalization,
    // and include the original path if it differs (e.g., macOS /private/var/... vs /var/...).
    let dir_str = dir.to_string_lossy().to_string();
    let dir_canon = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    let dir_canon_str = dir_canon.to_string_lossy().to_string();
    let safe_dirs = if dir_canon_str != dir_str {
        format!("{},{}", dir_canon_str, dir_str)
    } else {
        dir_canon_str
    };

    EnvGuard::new()
        .set("AIFO_NOTIFICATIONS_UNSAFE_ALLOWLIST", "1")
        .set("AIFO_NOTIFICATIONS_SAFE_DIRS", safe_dirs)
}
