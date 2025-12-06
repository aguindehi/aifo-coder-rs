use fs2::FileExt;
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

#[cfg(feature = "otel")]
use tracing::instrument;

/// Repository/user-scoped lock guard that removes the lock file on drop.
#[derive(Debug)]
pub struct RepoLock {
    file: File,
    path: PathBuf,
}

impl Drop for RepoLock {
    fn drop(&mut self) {
        // Best-effort unlock; ignore errors
        let _ = self.file.unlock();

        // Try removal with brief retries (avoid background threads to keep tests leak-free)
        let path = self.path.clone();
        for _ in 0..10 {
            if !path.exists() {
                break;
            }
            if fs::remove_file(&path).is_ok() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
}

#[cfg_attr(
    feature = "otel",
    instrument(
        level = "info",
        err,
        skip(),
        fields(aifo_coder_candidate_paths = candidate_lock_paths().len())
    )
)]
/// Acquire a non-blocking exclusive lock using default candidate lock paths.
pub fn acquire_lock() -> io::Result<RepoLock> {
    let paths = candidate_lock_paths();
    let mut last_err: Option<io::Error> = None;

    for p in paths {
        // Best effort to ensure parent exists
        if let Some(parent) = p.parent() {
            let _ = fs::create_dir_all(parent);
        }
        match OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(true)
            .open(&p)
        {
            Ok(f) => match f.try_lock_exclusive() {
                Ok(_) => {
                    return Ok(RepoLock {
                        file: f,
                        path: p.clone(),
                    });
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    #[cfg(feature = "otel")]
                    {
                        tracing::error!("lock acquisition failed: lock held by another process");
                        use opentelemetry::trace::{Status, TraceContextExt};
                        use tracing_opentelemetry::OpenTelemetrySpanExt;
                        let cx = tracing::Span::current().context();
                        cx.span().set_status(Status::error("aifo_coder_lock_held"));
                    }
                    return Err(io::Error::other(crate::display_for_fork_error(
                        &crate::ForkError::Message(
                            "Another coding agent is already running (lock held). Please try again later.".to_string(),
                        ),
                    )));
                }
                Err(e) => {
                    last_err = Some(e);
                    continue;
                }
            },
            Err(e) => {
                last_err = Some(e);
                continue;
            }
        }
    }

    let mut msg = String::from("Failed to create lock file in any candidate location: ");
    msg.push_str(
        &candidate_lock_paths()
            .into_iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", "),
    );
    if let Some(e) = last_err {
        msg.push_str(&format!(" (last error: {e})"));
        #[cfg(feature = "otel")]
        {
            // Avoid embedding raw paths directly; log a concise hashed summary instead.
            let status_msg = crate::telemetry::hash_string_hex(&msg);
            let status = format!("aifo_coder_lock_acquisition_failed:{}", status_msg);
            tracing::error!("lock acquisition failed: {}", status);
            use opentelemetry::trace::{Status, TraceContextExt};
            use tracing_opentelemetry::OpenTelemetrySpanExt;
            let cx = tracing::Span::current().context();
            cx.span().set_status(Status::error(status));
        }
    }
    Err(io::Error::other(crate::display_for_fork_error(
        &crate::ForkError::Message(msg),
    )))
}

#[cfg_attr(
    feature = "otel",
    instrument(
        level = "info",
        err,
        skip(),
        fields(path_hash = %crate::telemetry::hash_string_hex(&p.display().to_string()))
    )
)]
/// Acquire a lock at a specific path (helper for tests).
pub fn acquire_lock_at(p: &Path) -> io::Result<RepoLock> {
    if let Some(parent) = p.parent() {
        let _ = fs::create_dir_all(parent);
    }
    match OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(true)
        .open(p)
    {
        Ok(f) => {
            match f.try_lock_exclusive() {
                Ok(_) => Ok(RepoLock {
                    file: f,
                    path: p.to_path_buf(),
                }),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    #[cfg(feature = "otel")]
                    {
                        tracing::error!("lock acquisition failed at specific path: lock held by another process");
                        use opentelemetry::trace::{Status, TraceContextExt};
                        use tracing_opentelemetry::OpenTelemetrySpanExt;
                        let cx = tracing::Span::current().context();
                        cx.span().set_status(Status::error("aifo_coder_lock_held"));
                    }
                    Err(io::Error::other(crate::display_for_fork_error(
                    &crate::ForkError::Message(
                        "Another coding agent is already running (lock held). Please try again later."
                            .to_string(),
                    ),
                )))
                }
                Err(e) => Err(e),
            }
        }
        Err(e) => Err(e),
    }
}

/// Return true if the launcher should acquire a repository/user lock for this process.
/// Honor AIFO_CODER_SKIP_LOCK=1 to skip acquiring any lock (used by fork child panes).
pub fn should_acquire_lock() -> bool {
    env::var("AIFO_CODER_SKIP_LOCK").ok().as_deref() != Some("1")
}

/// Candidate lock file locations.
/// - If inside a Git repository:
///   1) <repo_root>/.aifo-coder.lock
///   2) <xdg_runtime>/aifo-coder.<hash(repo_root)>.lock
/// - Otherwise (not in a Git repo), legacy ordered candidates:
///   HOME/.aifo-coder.lock, XDG_RUNTIME_DIR/aifo-coder.lock, /tmp/aifo-coder.lock, CWD/.aifo-coder.lock
pub fn candidate_lock_paths() -> Vec<PathBuf> {
    // Capture the current working directory immediately to avoid races with other tests
    // that may call set_current_dir() in parallel.
    let initial_cwd = env::current_dir().ok();

    if let Some(root) = crate::repo_root() {
        let mut paths = Vec::new();
        // Preferred: in-repo lock (if writable, acquire will succeed)
        paths.push(root.join(".aifo-coder.lock"));
        // Secondary: runtime-scoped hashed lock path
        let rt_base = env::var("XDG_RUNTIME_DIR")
            .ok()
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let key = normalized_repo_key_for_hash(&root);
        let hash = hash_repo_key_hex(&key);
        paths.push(rt_base.join(format!("aifo-coder.{}.lock", hash)));
        // Tertiary fallback: always include a tmp-scoped lock path for robustness and tests
        paths.push(PathBuf::from("/tmp/aifo-coder.lock"));
        return paths;
    }

    // Not inside a Git repository: legacy behavior
    let mut paths = Vec::new();
    if let Some(home) = home::home_dir() {
        paths.push(home.join(".aifo-coder.lock"));
    }
    if let Ok(rt) = env::var("XDG_RUNTIME_DIR") {
        if !rt.is_empty() {
            paths.push(PathBuf::from(rt).join("aifo-coder.lock"));
        }
    }
    paths.push(PathBuf::from("/tmp/aifo-coder.lock"));
    if let Some(cwd) = initial_cwd.clone().or_else(|| env::current_dir().ok()) {
        paths.push(cwd.join(".aifo-coder.lock"));
    }
    paths
}

/// Normalize a repository path string for hashing to a stable key.
pub fn normalized_repo_key_for_hash(p: &Path) -> String {
    let abs = fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let s = abs.to_string_lossy().to_string();
    #[cfg(windows)]
    {
        // Normalize separators to backslashes and case-fold
        let mut t = s.replace('/', "\\").to_ascii_lowercase();
        // Uppercase drive letter if path starts with "c:\" style
        if t.len() >= 2 && t.as_bytes()[1] == b':' {
            let mut chs: Vec<u8> = t.into_bytes();
            chs[0] = (chs[0] as char).to_ascii_uppercase() as u8;
            return String::from_utf8(chs).unwrap_or_default();
        }
        t
    }
    #[cfg(not(windows))]
    {
        s
    }
}

/// Simple stable 64-bit FNV-1a hash for strings; returns 16-hex lowercase id.
pub fn hash_repo_key_hex(s: &str) -> String {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 1099511628211;
    let mut h: u64 = FNV_OFFSET;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(FNV_PRIME);
    }
    format!("{:016x}", h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_candidate_lock_paths_includes_xdg_runtime_dir() {
        // Use a non-repo temp directory to exercise legacy fallback candidates
        let td = tempfile::tempdir().expect("tmpdir");
        let old = std::env::var("XDG_RUNTIME_DIR").ok();
        let old_cwd = std::env::current_dir().expect("cwd");
        std::env::set_var("XDG_RUNTIME_DIR", td.path());
        std::env::set_current_dir(td.path()).expect("chdir");

        let paths = candidate_lock_paths();
        let expected = td.path().join("aifo-coder.lock");
        assert!(
            paths.iter().any(|p| p == &expected),
            "candidate_lock_paths missing expected XDG_RUNTIME_DIR path: {:?}",
            expected
        );

        // Restore env and cwd
        if let Some(v) = old {
            std::env::set_var("XDG_RUNTIME_DIR", v);
        } else {
            std::env::remove_var("XDG_RUNTIME_DIR");
        }
        std::env::set_current_dir(old_cwd).ok();
    }

    #[test]
    fn test_candidate_lock_paths_includes_cwd_lock_outside_repo() {
        // In a non-repo directory, ensure CWD/.aifo-coder.lock appears among legacy candidates
        let td = tempfile::tempdir().expect("tmpdir");
        let old_cwd = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(td.path()).expect("chdir");
        let paths = candidate_lock_paths();
        let expected = td.path().join(".aifo-coder.lock");
        // Allow symlink differences by comparing canonicalized parent
        let expected_dir_canon =
            std::fs::canonicalize(td.path()).unwrap_or_else(|_| td.path().to_path_buf());
        let found = paths.iter().any(|p| {
            p.file_name()
                .map(|n| n == ".aifo-coder.lock")
                .unwrap_or(false)
                && p.parent()
                    .and_then(|d| std::fs::canonicalize(d).ok())
                    .map(|d| d == expected_dir_canon)
                    .unwrap_or(false)
        });
        assert!(
            found,
            "candidate_lock_paths missing expected CWD lock path: {:?} in {:?}",
            expected, paths
        );
        std::env::set_current_dir(old_cwd).ok();
    }

    #[test]
    fn test_should_acquire_lock_env() {
        // Default: acquire
        std::env::remove_var("AIFO_CODER_SKIP_LOCK");
        assert!(should_acquire_lock(), "should acquire lock by default");
        // Skip when set to "1"
        std::env::set_var("AIFO_CODER_SKIP_LOCK", "1");
        assert!(
            !should_acquire_lock(),
            "should not acquire lock when AIFO_CODER_SKIP_LOCK=1"
        );
        std::env::remove_var("AIFO_CODER_SKIP_LOCK");
    }
}
