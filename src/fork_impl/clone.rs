use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::process::Stdio;

pub(crate) fn fork_clone_and_checkout_panes_impl(
    repo_root: &Path,
    sid: &str,
    panes: usize,
    base_ref_or_sha: &str,
    base_label: &str,
    dissociate: bool,
) -> std::io::Result<Vec<(PathBuf, String)>> {
    if panes < 1 {
        return Ok(Vec::new());
    }
    let repo_abs = fs::canonicalize(repo_root).unwrap_or_else(|_| repo_root.to_path_buf());
    let root_str = repo_abs.to_string_lossy().to_string();
    let src_url = if cfg!(windows) {
        format!("file:///{}", root_str.replace('\\', "/"))
    } else {
        format!("file://{}", root_str)
    };
    let session_dir = super::fork_session_dir(&repo_abs, sid);
    fs::create_dir_all(&session_dir)?;

    // Try to capture push URL from base repo (non-fatal if unavailable)
    let base_push_url = Command::new("git")
        .arg("-C")
        .arg(&repo_abs)
        .arg("remote")
        .arg("get-url")
        .arg("--push")
        .arg("origin")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty());

    let mut results: Vec<(PathBuf, String)> = Vec::with_capacity(panes);

    for i in 1..=panes {
        let pane_dir = session_dir.join(format!("pane-{}", i));
        // Try cloning using a plain local path first (most compatible), then fall back to file:// with protocol allow.
        let mut cloned_ok = false;
        for (source, allow_file_proto) in [(&root_str, false), (&src_url, true)] {
            let mut clone = Command::new("git");
            if allow_file_proto {
                // Newer Git may restrict file:// by default; allow it explicitly for local cloning.
                clone.arg("-c").arg("protocol.file.allow=always");
            }
            clone
                .arg("clone")
                .arg("--no-checkout")
                .arg("--reference-if-able")
                .arg(&root_str);
            if dissociate {
                clone.arg("--dissociate");
            }
            // repository URL/path and destination directory
            clone.arg(source).arg(&pane_dir);
            clone.stdout(Stdio::null()).stderr(Stdio::null());
            let st = clone.status()?;
            if st.success() {
                cloned_ok = true;
                break;
            } else {
                // Clean up any partial directory before next attempt
                let _ = fs::remove_dir_all(&pane_dir);
            }
        }
        if !cloned_ok {
            return Err(std::io::Error::other(format!(
                "git clone failed for pane {}",
                i
            )));
        }

        // Optional: set origin push URL to match base repo
        if let Some(ref url) = base_push_url {
            let _ = Command::new("git")
                .arg("-C")
                .arg(&pane_dir)
                .arg("remote")
                .arg("set-url")
                .arg("origin")
                .arg(url)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }

        // git checkout -b fork/<base>/<sid>-<i> <base_ref_or_sha>
        let branch = super::fork_branch_name(base_label, sid, i);
        let st = Command::new("git")
            .arg("-C")
            .arg(&pane_dir)
            .arg("checkout")
            .arg("-b")
            .arg(&branch)
            .arg(base_ref_or_sha)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
        if !st.success() {
            let _ = fs::remove_dir_all(&pane_dir);
            return Err(std::io::Error::other(format!(
                "git checkout failed for pane {} (branch {})",
                i, branch
            )));
        }

        // Best-effort submodules and Git LFS
        if pane_dir.join(".gitmodules").exists() {
            let mut sm = Command::new("git");
            super::fork_impl_git::set_file_allow(&mut sm);
            let _ = sm
                .arg("-C")
                .arg(&pane_dir)
                .arg("submodule")
                .arg("update")
                .arg("--init")
                .arg("--recursive")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        // Git LFS: if git lfs is available and repository appears to use LFS, perform install/fetch/checkout
        let lfs_available = super::fork_impl_git::git_supports_lfs();
        if lfs_available {
            let uses_lfs = crate::repo_uses_lfs_quick(&pane_dir);
            if uses_lfs {
                let _ = Command::new("git")
                    .arg("-C")
                    .arg(&pane_dir)
                    .arg("lfs")
                    .arg("install")
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
                let _ = Command::new("git")
                    .arg("-C")
                    .arg(&pane_dir)
                    .arg("lfs")
                    .arg("fetch")
                    .arg("--all")
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
                let _ = Command::new("git")
                    .arg("-C")
                    .arg(&pane_dir)
                    .arg("lfs")
                    .arg("checkout")
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
            }
        }

        results.push((pane_dir, branch));
    }

    Ok(results)
}
