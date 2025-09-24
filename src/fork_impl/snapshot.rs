//! Snapshot helper: create a temporary commit including dirty index/working tree via a temporary index.
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::{Command, Stdio};

pub(crate) fn fork_create_snapshot_impl(repo_root: &Path, sid: &str) -> std::io::Result<String> {
    // Create a unique temporary index path (under .git when possible)
    let tmp_idx = {
        let git_dir_out = {
            let mut cmd = Command::new("git");
            cmd.arg("-C")
                .arg(repo_root)
                .arg("rev-parse")
                .arg("--git-dir")
                .stdout(Stdio::piped())
                .stderr(Stdio::null());
            cmd.output().ok()
        };
        let git_dir = git_dir_out
            .and_then(|o| {
                if o.status.success() {
                    Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                } else {
                    None
                }
            })
            .map(PathBuf::from)
            .unwrap_or_else(|| repo_root.join(".git"));
        let pid = std::process::id();
        let idx_name = format!("index.aifo-{}-{}", sid, pid);
        git_dir.join(idx_name)
    };
    // Helper to run git with the temporary index
    let with_tmp_index = |args: &[&str]| -> std::io::Result<std::process::Output> {
        let mut cmd = Command::new("git");
        cmd.arg("-C").arg(repo_root);
        for a in args {
            cmd.arg(a);
        }
        cmd.env("GIT_INDEX_FILE", &tmp_idx);
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        cmd.output()
    };

    // 1) Index current working tree: git add -A
    let add_out = with_tmp_index(&["add", "-A"])?;
    if !add_out.status.success() {
        let _ = fs::remove_file(&tmp_idx);
        return Err(std::io::Error::other(
            aifo_coder::display_for_fork_error(&aifo_coder::ForkError::Message(
                "git add -A failed for snapshot".to_string(),
            )),
        ));
    }

    // 2) write-tree
    let wt = with_tmp_index(&["write-tree"])?;
    if !wt.status.success() {
        let _ = fs::remove_file(&tmp_idx);
        return Err(std::io::Error::other(
            aifo_coder::display_for_fork_error(&aifo_coder::ForkError::Message(
                "git write-tree failed for snapshot".to_string(),
            )),
        ));
    }
    let tree = String::from_utf8_lossy(&wt.stdout).trim().to_string();

    // 3) Determine parent if any (HEAD may not exist)
    let parent = {
        let mut cmd = super::fork_impl_git::git_cmd(Some(repo_root));
        cmd.arg("rev-parse")
            .arg("--verify")
            .arg("HEAD")
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        cmd.output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    };

    // 4) commit-tree
    let mut ct = super::fork_impl_git::git_cmd(Some(repo_root));
    ct.arg("commit-tree").arg(&tree);
    if let Some(p) = parent.as_deref() {
        ct.arg("-p").arg(p);
    }
    ct.arg("-m").arg(format!("aifo-fork snapshot {}", sid));
    ct.stdout(Stdio::piped()).stderr(Stdio::piped());
    let ct_out = ct.output()?;
    // Clean up temporary index (best-effort)
    let _ = fs::remove_file(&tmp_idx);
    if !ct_out.status.success() {
        return Err(std::io::Error::other(format!(
            "git commit-tree failed for snapshot: {}",
            String::from_utf8_lossy(&ct_out.stderr)
        )));
    }
    let sha = String::from_utf8_lossy(&ct_out.stdout).trim().to_string();
    if sha.is_empty() {
        return Err(std::io::Error::other(
            aifo_coder::display_for_fork_error(&aifo_coder::ForkError::Message(
                "empty snapshot SHA from commit-tree".to_string(),
            )),
        ));
    }
    Ok(sha)
}
