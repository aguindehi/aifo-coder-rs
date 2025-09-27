/*!
Test support helpers shared across integration tests.

- have_git(): check git availability on PATH
- which(bin): cross-platform which/where lookup
- init_repo_with_default_user(dir): initialize a git repo with default user.name/email

These helpers do not print skip messages themselves so tests can preserve their
existing "skipping: ..." outputs verbatim.
*/

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

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

/// Return true if a Docker image is present locally (without pulling).
#[allow(dead_code)]
pub fn docker_image_present(runtime: &std::path::Path, image: &str) -> bool {
    std::process::Command::new(runtime)
        .args(["image", "inspect", image])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Minimal raw HTTP POST helper over TCP returning (status, headers, body).
#[allow(dead_code)]
pub fn http_post_tcp(
    port: u16,
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
        "POST /exec HTTP/1.1\r\nHost: host.docker.internal\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\nConnection: close\r\n",
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

    // Split headers/body
    let mut status: u16 = 0;
    let headers_s;
    let mut body_out: Vec<u8> = Vec::new();

    if let Some(pos) =
        aifo_coder::find_crlfcrlf(&buf).or_else(|| buf.windows(2).position(|w| w == b"\n\n"))
    {
        let h = &buf[..pos];
        headers_s = String::from_utf8_lossy(h).to_string();
        // Parse status code from status line
        if let Some(line) = headers_s.lines().next() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                status = parts[1].parse::<u16>().unwrap_or(0);
            }
        }
        // Body (best-effort)
        let mut body_bytes = buf[pos..].to_vec();
        // Drop leading CRLFCRLF or LF+LF
        while body_bytes.first() == Some(&b'\r') || body_bytes.first() == Some(&b'\n') {
            body_bytes.remove(0);
        }
        body_out = body_bytes;
    } else {
        headers_s = String::from_utf8_lossy(&buf).to_string();
    }
    (status, headers_s, body_out)
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

#[cfg(unix)]
/// Capture stdout to a temporary file while running `f`, returning the captured text.
/// Intended for integration tests; mirrors repeated inline helpers.
#[allow(dead_code)]
pub fn capture_stdout<F: FnOnce()>(f: F) -> String {
    use libc::{dup, dup2, fflush, fileno, fopen, STDOUT_FILENO};
    use std::os::fd::{FromRawFd, RawFd};
    unsafe {
        // Open a temporary file (unique per call to avoid cross-test interleaving)
        let unique = format!(
            "/tmp/aifo-coder-test-stdout-{}-{}.tmp",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_else(|_| std::time::Duration::from_secs(0))
                .as_nanos()
        );
        let path = std::ffi::CString::new(unique).unwrap();
        let mode = std::ffi::CString::new("w+").unwrap();
        let file = fopen(path.as_ptr(), mode.as_ptr());
        assert!(!file.is_null(), "failed to open temp file for capture");
        let fd: RawFd = fileno(file);

        // Duplicate current stdout
        let stdout_fd = STDOUT_FILENO;
        let saved = dup(stdout_fd);
        assert!(saved >= 0, "dup(stdout) failed");

        // Redirect stdout to file
        assert!(dup2(fd, stdout_fd) >= 0, "dup2 failed");

        // Run the function
        f();

        // Flush and restore stdout
        fflush(std::ptr::null_mut());
        assert!(dup2(saved, stdout_fd) >= 0, "restore dup2 failed");

        // Read back the file
        let mut f = std::fs::File::from_raw_fd(fd);
        use std::io::{Read, Seek};
        let mut s = String::new();
        let _ = f.rewind();
        f.read_to_string(&mut s).expect("read captured");
        s
    }
}
