use std::path::Path;
use std::process::{Command, Output, Stdio};

/// Run a git command with optional -C <repo>. Returns Output on invocation success.
pub fn git(repo: Option<&Path>, args: &[&str]) -> std::io::Result<Output> {
    let mut cmd = Command::new("git");
    if let Some(r) = repo {
        cmd.arg("-C").arg(r);
    }
    for a in args {
        cmd.arg(a);
    }
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    cmd.output()
}

/// Run a git command and capture trimmed stdout as UTF-8 String on success.
pub fn git_stdout_str(repo: Option<&Path>, args: &[&str]) -> Option<String> {
    git(repo, args).ok().and_then(|o| {
        if o.status.success() {
            Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
        } else {
            None
        }
    })
}

/// Get porcelain v1 status as string (empty when clean). None if git invocation failed.
pub fn git_status_porcelain(repo: &Path) -> Option<String> {
    git(Some(repo), &["status", "--porcelain=v1", "-uall"])
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
}

/// Does this host have git-lfs available?
pub fn git_supports_lfs() -> bool {
    git(None, &["lfs", "version"])
        .map(|o| o.status.success())
        .unwrap_or(false)
}
