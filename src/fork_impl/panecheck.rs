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

    // Track presence to distinguish missing vs present-empty, then normalize empty to None.
    let base_present = base_commit.is_some();
    let base_commit = match base_commit {
        Some(s) if s.trim().is_empty() => None,
        other => other,
    };
    // ahead/base-unknown detection
    let (ahead, base_unknown) = if let Some(base_sha) = base_commit {
        // Resolve HEAD sha
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
        // If HEAD not resolvable, base-unknown
        if let Some(head_sha) = head_sha_opt.as_ref() {
            // Fast path: exact equality means not-ahead and base is known
            if head_sha == base_sha {
                (false, false)
            } else {
                // Verify base exists, then detect ahead robustly.
                // If base does not resolve to a commit: base-unknown.
                let base_exists = {
                    let mut cmd = super::fork_impl_git::git_cmd(Some(pane_dir));
                    cmd.arg("cat-file")
                        .arg("-e")
                        .arg(format!("{}^{{commit}}", base_sha));
                    cmd.stdout(std::process::Stdio::null());
                    cmd.stderr(std::process::Stdio::null());
                    cmd.status().ok().map(|st| st.success()).unwrap_or(false)
                };
                if !base_exists {
                    (false, true)
                } else {
                    // Try merge-base base HEAD and compare output to base commit.
                    let mb_sha_opt = {
                        let mut cmd = super::fork_impl_git::git_cmd(Some(pane_dir));
                        cmd.arg("merge-base").arg(base_sha).arg("HEAD");
                        cmd.stdout(std::process::Stdio::piped());
                        cmd.stderr(std::process::Stdio::null());
                        cmd.output().ok().and_then(|o| {
                            if o.status.success() {
                                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                            } else {
                                None
                            }
                        })
                    };
                    if let Some(mb) = mb_sha_opt {
                        if mb == base_sha {
                            (true, false)
                        } else {
                            // Not ancestor -> not ahead
                            (false, false)
                        }
                    } else {
                        // Fallback: count commits in base..HEAD; >0 => ahead
                        let cnt_opt = {
                            let mut cmd = super::fork_impl_git::git_cmd(Some(pane_dir));
                            cmd.arg("rev-list").arg("--count").arg(format!("{}..HEAD", base_sha));
                            cmd.stdout(std::process::Stdio::piped());
                            cmd.stderr(std::process::Stdio::null());
                            cmd.output().ok().and_then(|o| {
                                if o.status.success() {
                                    Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                                } else {
                                    None
                                }
                            })
                        };
                        if let Some(cnt) = cnt_opt {
                            let n = cnt.parse::<u64>().unwrap_or(0);
                            if n > 0 { (true, false) } else { (false, false) }
                        } else {
                            (false, true)
                        }
                    }
                }
            }
        } else {
            // HEAD not resolvable -> treat as base-unknown
            (false, true)
        }
    } else {
        // Missing vs empty handling:
        // - Missing key (base_present=false): base-unknown=true (always).
        // - Empty string (base_present=true): base-unknown=true only if HEAD exists (repo has commits).
        let has_head = {
            let mut cmd = super::fork_impl_git::git_cmd(Some(pane_dir));
            cmd.arg("rev-parse").arg("--verify").arg("HEAD");
            cmd.stdout(std::process::Stdio::null());
            cmd.stderr(std::process::Stdio::null());
            cmd.status().ok().map(|st| st.success()).unwrap_or(false)
        };
        (false, (!base_present) || has_head)
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
