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
        // Verify base commit exists
        let base_ok = {
            let mut cmd = super::fork_impl_git::git_cmd(Some(pane_dir));
            cmd.arg("rev-parse").arg("--verify").arg(base_sha);
            cmd.stderr(std::process::Stdio::null());
            cmd.status().ok().map(|st| st.success()).unwrap_or(false)
        };
        if !base_ok {
            // Recorded base commit does not resolve in this pane
            (false, true)
        } else {
            // Resolve HEAD commit
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
            if let Some(head_sha) = head_sha_opt {
                if head_sha == base_sha {
                    // Exactly at base: not ahead, base is known
                    (false, false)
                } else {
                    // Prefer simple rev-list count; fall back to merge-base if needed
                    let ahead_count_opt = {
                        let mut cmd = super::fork_impl_git::git_cmd(Some(pane_dir));
                        cmd.arg("rev-list")
                            .arg("--count")
                            .arg(format!("{}..HEAD", base_sha));
                        cmd.stderr(std::process::Stdio::null());
                        cmd.output().ok().and_then(|o| {
                            if o.status.success() {
                                let c = String::from_utf8_lossy(&o.stdout)
                                    .trim()
                                    .parse::<u64>()
                                    .unwrap_or(0);
                                Some(c)
                            } else {
                                None
                            }
                        })
                    };
                    if let Some(c) = ahead_count_opt {
                        (c > 0, false)
                    } else {
                        // Try merge-base to decide if base is an ancestor; if not, mark base-unknown
                        let mb_opt = {
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
                        if let Some(mb) = mb_opt {
                            if !mb.is_empty() {
                                // Base is known; consider ahead if HEAD != base (already true here)
                                (true, false)
                            } else {
                                // merge-base returned empty; treat as unknown base
                                (false, true)
                            }
                        } else {
                            // Unable to determine; conservatively treat base as unknown
                            (false, true)
                        }
                    }
                }
            } else {
                // HEAD not resolvable; treat base as unknown
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
