//! Pane cleanliness checks: classify panes as clean or protected (dirty, submodules, ahead, base-unknown).
use std::path::Path;

/// Result of checking a pane's cleanliness and protection reasons.
pub struct PaneCheck {
    pub clean: bool,
    pub reasons: Vec<String>,
}

/// Determine pane state: dirty, submodules-dirty, ahead, base-unknown.
/// Mirrors existing logic in fork_clean/autoclean for behavior parity.
pub fn pane_check(pane_dir: &Path, base_commit: Option<&str>) -> PaneCheck {
    let mut reasons: Vec<String> = Vec::new();

    // dirty detection
    let dirty = super::fork_impl_git::git_status_porcelain(pane_dir)
        .map(|s| !s.is_empty())
        .unwrap_or(false);
    if dirty {
        reasons.push("dirty".to_string());
    } else {
        // submodule changes detect
        if let Ok(o) = {
            let mut cmd = super::fork_impl_git::git_cmd(Some(pane_dir));
            cmd.arg("submodule")
                .arg("status")
                .arg("--recursive")
                .stderr(std::process::Stdio::null())
                .output()
        } {
            let s = String::from_utf8_lossy(&o.stdout);
            if s.lines()
                .any(|l| l.starts_with('+') || l.starts_with('-') || l.starts_with('U'))
            {
                reasons.push("submodules-dirty".to_string());
            }
        }
    }

    // ahead/base-unknown detection
    let (ahead, base_unknown) = if let Some(base_sha) = base_commit {
        // Verify base and HEAD exist in this pane repo
        let base_known = {
            let mut cmd = super::fork_impl_git::git_cmd(Some(pane_dir));
            cmd.arg("rev-parse")
                .arg("--verify")
                .arg(format!("{base_sha}^{{commit}}"));
            cmd.stdout(std::process::Stdio::null());
            cmd.stderr(std::process::Stdio::null());
            cmd.status().ok().map(|st| st.success()).unwrap_or(false)
        };
        let head_sha_opt = {
            let mut cmd = super::fork_impl_git::git_cmd(Some(pane_dir));
            cmd.arg("rev-parse").arg("--verify").arg("HEAD");
            cmd.stderr(std::process::Stdio::null());
            cmd.output().ok().and_then(|o| {
                if o.status.success() {
                    Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                } else {
                    None
                }
            })
        };
        if !(base_known && head_sha_opt.is_some()) {
            (false, true)
        } else {
            // Fast path: exact equality means not-ahead and base is known
            if let Some(ref head_sha_eq) = head_sha_opt {
                if head_sha_eq == base_sha {
                    (false, false)
                } else {
                    // Use merge-base --is-ancestor to decide ancestry robustly
                    let is_ancestor = {
                        let mut cmd = super::fork_impl_git::git_cmd(Some(pane_dir));
                        cmd.arg("merge-base")
                            .arg("--is-ancestor")
                            .arg(base_sha)
                            .arg("HEAD");
                        cmd.stderr(std::process::Stdio::null());
                        cmd.status().ok().map(|st| st.success()).unwrap_or(false)
                    };
                    if is_ancestor {
                        (true, false)
                    } else {
                        // Base commit recorded but not an ancestor of HEAD -> treat as not-ahead with base known
                        (false, false)
                    }
                }
            } else {
                // HEAD not resolvable -> treat as base-unknown
                (false, true)
            }
        }
    } else {
        // No recorded base -> unknown
        (false, true)
    };
    if ahead {
        reasons.push("ahead".to_string());
    }
    if base_unknown {
        reasons.push("base-unknown".to_string());
    }

    PaneCheck {
        clean: reasons.is_empty(),
        reasons,
    }
}
