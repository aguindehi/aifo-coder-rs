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

    // Normalize empty base_commit to None (treat as "no recorded base" — not protective)
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
                // Use merge-base --is-ancestor and classify by exit code:
                // 0 => ancestor; 1 => not ancestor; 128/other => invalid base -> base-unknown
                let exit_code_opt = {
                    let mut cmd = super::fork_impl_git::git_cmd(Some(pane_dir));
                    cmd.arg("merge-base")
                        .arg("--is-ancestor")
                        .arg(base_sha)
                        .arg("HEAD");
                    cmd.stderr(std::process::Stdio::null());
                    cmd.status().ok().and_then(|st| st.code())
                };
                match exit_code_opt {
                    Some(0) => (true, false),  // ancestor -> ahead
                    Some(1) => (false, false), // not ancestor -> not ahead
                    _ => (false, true),        // error/spawn -> base-unknown
                }
            }
        } else {
            // HEAD not resolvable -> treat as base-unknown
            (false, true)
        }
    } else {
        // No recorded base (missing/empty) -> do not mark base-unknown; consider clean if not dirty
        (false, false)
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
