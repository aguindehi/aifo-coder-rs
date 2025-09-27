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
