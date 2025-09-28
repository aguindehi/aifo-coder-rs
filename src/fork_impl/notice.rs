//! Stale notice and auto-clean logic for fork sessions (no behavior change from public facade).
use std::env;
use std::fs;
use std::time::SystemTime;

use super::fork_impl_panecheck;
use super::fork_impl_scan;
use crate::fork_meta;

/// Print a notice about stale fork sessions for the current repository (quiet; best-effort).
pub fn fork_print_stale_notice_impl() {
    let repo = match super::repo_root() {
        Some(p) => p,
        None => {
            // Fallback to current working directory (best-effort), e.g., for doctor runs
            match env::current_dir() {
                Ok(p) => p,
                Err(_) => return,
            }
        }
    };
    let base = repo.join(".aifo-coder").join("forks");
    if !base.exists() {
        return;
    }
    let threshold_days: u64 = env::var("AIFO_CODER_FORK_STALE_DAYS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(30);
    let now = fork_impl_scan::secs_since_epoch(SystemTime::now());
    let mut count = 0usize;
    let mut oldest = 0u64;
    for sd in fork_impl_scan::session_dirs(&base) {
        let meta = fs::read_to_string(sd.join(".meta.json")).ok();
        let created_at = meta
            .as_deref()
            .and_then(|s| fork_meta::extract_value_u64(s, "created_at"))
            .or_else(|| {
                fs::metadata(&sd)
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .map(fork_impl_scan::secs_since_epoch)
            })
            .unwrap_or(0);
        if created_at == 0 {
            continue;
        }
        let age_days = (now.saturating_sub(created_at) / 86400) as u64;
        if age_days >= threshold_days {
            count += 1;
            if age_days > oldest {
                oldest = age_days;
            }
        }
    }
    if count > 0 {
        let use_err = crate::color_enabled_stderr();
        crate::log_info_stderr(
            use_err,
            &format!(
                "Found {} old fork sessions (oldest {}d). Consider: aifo-coder fork clean --older-than {}",
                count, oldest, threshold_days
            ),
        );
    }
}

/// Auto-clean clean fork sessions older than the stale threshold when AIFO_CODER_FORK_AUTOCLEAN=1 is set.
pub fn fork_autoclean_if_enabled_impl() {
    if env::var("AIFO_CODER_FORK_AUTOCLEAN").ok().as_deref() != Some("1") {
        return;
    }
    let repo = match super::repo_root() {
        Some(p) => p,
        None => return,
    };
    let base = repo.join(".aifo-coder").join("forks");
    if !base.exists() {
        return;
    }
    let threshold_days: u64 = env::var("AIFO_CODER_FORK_STALE_DAYS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(30);
    let now = fork_impl_scan::secs_since_epoch(SystemTime::now());

    let mut deleted = 0usize;
    let mut kept = 0usize;
    let autoclean_verbose = env::var("AIFO_CODER_FORK_AUTOCLEAN_VERBOSE")
        .ok()
        .as_deref()
        == Some("1");
    let mut deleted_sids: Vec<String> = Vec::new();
    let mut kept_sids: Vec<String> = Vec::new();

    for sd in fork_impl_scan::session_dirs(&base) {
        let meta = fs::read_to_string(sd.join(".meta.json")).ok();
        let created_at = meta
            .as_deref()
            .and_then(|s| fork_meta::extract_value_u64(s, "created_at"))
            .or_else(|| {
                fs::metadata(&sd)
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .map(fork_impl_scan::secs_since_epoch)
            })
            .unwrap_or(0);
        if created_at == 0 {
            continue;
        }
        let age_days = (now.saturating_sub(created_at) / 86400) as u64;
        if age_days < threshold_days {
            continue;
        }

        // Determine if all panes are clean (safe to delete entire session)
        let base_commit = meta
            .as_deref()
            .and_then(|s| fork_meta::extract_value_string(s, "base_commit_sha"));
        let panes = fork_impl_scan::pane_dirs_for_session(&sd);

        let mut all_clean = true;
        for p in panes {
            let pc = fork_impl_panecheck::pane_check(&p, base_commit.as_deref());
            if !pc.clean {
                all_clean = false;
                break;
            }
        }

        if all_clean {
            let _ = std::fs::remove_dir_all(&sd);
            let sid = sd
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            if !sid.is_empty() {
                deleted_sids.push(sid);
            }
            deleted += 1;
        } else {
            let sid = sd
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            if !sid.is_empty() {
                kept_sids.push(sid);
            }
            kept += 1;
        }
    }

    if deleted > 0 {
        let use_err = crate::color_enabled_stderr();
        crate::log_info_stderr(
            use_err,
            &format!(
                "Auto-clean: removed {} clean fork session(s) older than {}d; kept {} protected session(s).",
                deleted, threshold_days, kept
            ),
        );
        if autoclean_verbose {
            if !deleted_sids.is_empty() {
                crate::log_info_stderr(
                    use_err,
                    &format!("  deleted sessions: {}", deleted_sids.join(" ")),
                );
            }
            if !kept_sids.is_empty() {
                crate::log_info_stderr(
                    use_err,
                    &format!("  protected sessions kept: {}", kept_sids.join(" ")),
                );
            }
        }
    }
}
