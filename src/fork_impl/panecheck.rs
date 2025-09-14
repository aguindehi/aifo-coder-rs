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
    let mut ahead = false;
    let mut base_unknown = false;
    if let Some(base_sha) = base_commit {
        let out = {
            let mut cmd = super::fork_impl_git::git_cmd(Some(pane_dir));
            cmd.arg("rev-list")
                .arg("--count")
                .arg(format!("{}..HEAD", base_sha))
                .output()
                .ok()
        };
        if let Some(o) = out {
            if o.status.success() {
                let c = String::from_utf8_lossy(&o.stdout)
                    .trim()
                    .parse::<u64>()
                    .unwrap_or(0);
                if c > 0 {
                    ahead = true;
                }
            } else {
                base_unknown = true;
            }
        } else {
            base_unknown = true;
        }
    } else {
        base_unknown = true;
    }
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
