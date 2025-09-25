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
        // Ensure both base and HEAD resolve
        let base_ok = {
            let mut cmd = super::fork_impl_git::git_cmd(Some(pane_dir));
            cmd.arg("rev-parse").arg("--verify").arg(base_sha);
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
        if !base_ok {
            // Missing base object -> treat as base-unknown
            (false, true)
        } else {
            match head_sha_opt {
                Some(head_sha) => {
                    // Determine common ancestor; only consider 'ahead' when base is an ancestor
                    let mb = {
                        let mut cmd = super::fork_impl_git::git_cmd(Some(pane_dir));
                        cmd.arg("merge-base").arg(base_sha).arg("HEAD");
                        cmd.stderr(std::process::Stdio::null());
                        cmd.output().ok().and_then(|o| {
                            if o.status.success() {
                                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                            } else {
                                None
                            }
                        })
                    };
                    match mb {
                        Some(common) if !common.is_empty() && common == base_sha => {
                            // Base is an ancestor of HEAD; ahead iff HEAD != base
                            (head_sha != base_sha, false)
                        }
                        _ => {
                            // Base is not an ancestor or cannot be determined
                            (false, true)
                        }
                    }
                }
                None => {
                    // HEAD not resolvable -> treat as base-unknown
                    (false, true)
                }
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
