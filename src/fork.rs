#![allow(clippy::module_name_repetitions)]
//! Fork lifecycle: repo detection, snapshotting, cloning panes, merging, cleaning and notices.

use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::SystemTime;

use crate::{
    color_enabled_stderr, color_enabled_stdout, json_escape, paint, toolchain_cleanup_session,
};

#[path = "fork_impl/git.rs"]
mod fork_impl_git;
#[path = "fork_impl/merge.rs"]
mod fork_impl_merge;
#[path = "fork_impl/notice.rs"]
mod fork_impl_notice;
#[path = "fork_impl/panecheck.rs"]
mod fork_impl_panecheck;
#[path = "fork_impl/scan.rs"]
mod fork_impl_scan;
#[path = "fork_impl/list.rs"]
mod fork_impl_list;
#[path = "fork_impl/clone.rs"]
mod fork_impl_clone;
#[path = "fork_impl/snapshot.rs"]
mod fork_impl_snapshot;

/// Try to detect the Git repository root (absolute canonical path).
/// Returns Some(repo_root) when inside a Git repository; otherwise None.
pub fn repo_root() -> Option<PathBuf> {
    // Use helper to detect the repository top-level
    let s = match fork_impl_git::git_stdout_str(None, &["rev-parse", "--show-toplevel"]) {
        Some(v) => v.trim().to_string(),
        None => return None,
    };
    if s.is_empty() {
        return None;
    }
    let p = PathBuf::from(s);
    // Prefer canonical absolute path if possible
    fs::canonicalize(&p).ok().or(Some(p))
}

// Sanitize a ref path component: lowercase, replace invalid chars with '-',
// collapse repeated '-', and strip leading/trailing '/', '-' and '.'.
// Additionally trim to a safe length to keep branch names manageable.
pub fn fork_sanitize_base_label(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_dash = false;
    for ch in s.chars().map(|c| c.to_ascii_lowercase()) {
        let valid = ch.is_ascii_alphanumeric();
        let replace = match ch {
            '-' | '.' | '/' | ' ' | '_' => true, // treat these as separators
            _ => !valid,
        };
        if replace {
            if !last_dash && !out.is_empty() {
                out.push('-');
                last_dash = true;
            }
        } else {
            out.push(ch);
            last_dash = false;
        }
    }
    // Trim leading/trailing separators
    while matches!(out.chars().next(), Some('-') | Some('/') | Some('.')) {
        out.remove(0);
    }
    while matches!(out.chars().last(), Some('-') | Some('/') | Some('.')) {
        out.pop();
    }
    let mut res = if out.is_empty() {
        "base".to_string()
    } else {
        out
    };
    // Collapse any accidental double dashes that may remain
    while res.contains("--") {
        res = res.replace("--", "-");
    }
    // Enforce a conservative max length for the component
    const MAX_LEN: usize = 48;
    if res.len() > MAX_LEN {
        res.truncate(MAX_LEN);
        // Avoid trailing dash after truncation
        while matches!(res.chars().last(), Some('-') | Some('/') | Some('.')) && !res.is_empty() {
            res.pop();
        }
        if res.is_empty() {
            res = "base".to_string();
        }
    }
    res
}

/// Compute base ref/SHA and label for the current repository state.
/// Returns (base_label, base_ref_or_sha, base_commit_sha).
pub fn fork_base_info(repo_root: &Path) -> std::io::Result<(String, String, String)> {
    let root = repo_root;
    // Determine current branch or detached state
    let branch_out = Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("rev-parse")
        .arg("--abbrev-ref")
        .arg("HEAD")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()?;
    let head_out = Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("rev-parse")
        .arg("--verify")
        .arg("HEAD")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok();

    let head_sha = head_out
        .as_ref()
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_default();

    let (base_label, base_ref_or_sha) = if branch_out.status.success() {
        let name = String::from_utf8_lossy(&branch_out.stdout)
            .trim()
            .to_string();
        if name == "HEAD" {
            ("detached".to_string(), head_sha.clone())
        } else {
            (fork_sanitize_base_label(&name), name)
        }
    } else {
        ("detached".to_string(), head_sha.clone())
    };
    Ok((base_label, base_ref_or_sha, head_sha))
}

/// Create a temporary snapshot commit that includes staged + unstaged changes without
/// altering user index or working tree. Uses a temporary index (GIT_INDEX_FILE) and
/// git commit-tree. Returns the new snapshot commit SHA on success.
pub fn fork_create_snapshot(repo_root: &Path, sid: &str) -> std::io::Result<String> {
    fork_impl_snapshot::fork_create_snapshot_impl(repo_root, sid)
}

/// Construct the fork branch name for pane i (1-based): fork/<base-label>/<sid>-<i>
pub fn fork_branch_name(base_label: &str, sid: &str, i: usize) -> String {
    format!(
        "fork/{}/{}-{}",
        fork_sanitize_base_label(base_label),
        sid,
        i
    )
}

/// Base directory for fork panes: <repo-root>/.aifo-coder/forks/<sid>
pub fn fork_session_dir(repo_root: &Path, sid: &str) -> PathBuf {
    repo_root.join(".aifo-coder").join("forks").join(sid)
}

/// Quick heuristic to detect if a repository uses Git LFS without requiring git-lfs to be installed.
/// Returns true if:
/// - .lfsconfig exists at repo root, or
/// - any .gitattributes file (top-level or nested) contains "filter=lfs".
pub fn repo_uses_lfs_quick(repo_root: &Path) -> bool {
    // .lfsconfig presence is a strong hint
    if repo_root.join(".lfsconfig").exists() {
        return true;
    }
    // Top-level .gitattributes
    if let Ok(s) = fs::read_to_string(repo_root.join(".gitattributes")) {
        if s.contains("filter=lfs") {
            return true;
        }
    }
    // Scan nested .gitattributes files (skip .git directory)
    fn scan(dir: &Path) -> bool {
        let rd = match fs::read_dir(dir) {
            Ok(d) => d,
            Err(_) => return false,
        };
        for ent in rd {
            let Ok(ent) = ent else { continue };
            let path = ent.path();
            let Ok(ft) = ent.file_type() else { continue };
            if ft.is_dir() {
                // Skip VCS directory
                if ent.file_name().to_string_lossy() == ".git" {
                    continue;
                }
                if scan(&path) {
                    return true;
                }
            } else if ft.is_file() && ent.file_name().to_string_lossy() == ".gitattributes" {
                if let Ok(s) = fs::read_to_string(&path) {
                    if s.contains("filter=lfs") {
                        return true;
                    }
                }
            }
        }
        false
    }
    scan(repo_root)
}

/// Clone and checkout N fork panes based on a base ref/SHA.
/// Each pane is created under <repo-root>/.aifo-coder/forks/<sid>/pane-<i> and
/// on success returns a vector of (pane_dir, branch_name).
pub fn fork_clone_and_checkout_panes(
    repo_root: &Path,
    sid: &str,
    panes: usize,
    base_ref_or_sha: &str,
    base_label: &str,
    dissociate: bool,
) -> std::io::Result<Vec<(PathBuf, String)>> {
    fork_impl_clone::fork_clone_and_checkout_panes_impl(
        repo_root,
        sid,
        panes,
        base_ref_or_sha,
        base_label,
        dissociate,
    )
}

/// Options for fork clean command.
pub struct ForkCleanOpts {
    pub session: Option<String>,
    pub older_than_days: Option<u64>,
    pub all: bool,
    pub dry_run: bool,
    pub yes: bool,
    pub force: bool,
    pub keep_dirty: bool,
    pub json: bool,
}

fn read_file_to_string(p: &Path) -> Option<String> {
    fs::read_to_string(p).ok()
}

/* moved: use crate::fork_meta::extract_value_string / extract_value_u64 */

// Append or upsert merge metadata fields into the session .meta.json
/* moved: use crate::fork_meta::append_fields_compact */

/* moved: use fork_impl_scan::session_dirs */

/* moved: use fork_impl_scan::pane_dirs_for_session */

/* moved: use fork_impl_scan::secs_since_epoch */

/// List fork sessions under the current repository.
/// Returns exit code (0 on success).
pub fn fork_list(repo_root: &Path, json: bool, all_repos: bool) -> std::io::Result<i32> {
    fork_impl_list::fork_list_impl(repo_root, json, all_repos)
}

/// Clean fork sessions and panes with safety protections.
/// Returns exit code (0 on success; 1 on refusal or error).
pub fn fork_clean(repo_root: &Path, opts: &ForkCleanOpts) -> std::io::Result<i32> {
    let base = repo_root.join(".aifo-coder").join("forks");
    if !base.exists() {
        eprintln!(
            "aifo-coder: no fork sessions directory at {}",
            base.display()
        );
        return Ok(0);
    }
    let targets: Vec<PathBuf> = if let Some(ref sid) = opts.session {
        let p = base.join(sid);
        if p.exists() {
            vec![p]
        } else {
            Vec::new()
        }
    } else if let Some(days) = opts.older_than_days {
        let now = fork_impl_scan::secs_since_epoch(SystemTime::now());
        fork_impl_scan::session_dirs(&base)
            .into_iter()
            .filter(|sd| {
                let meta = read_file_to_string(&sd.join(".meta.json"));
                let created_at = meta
                    .as_deref()
                    .and_then(|s| crate::fork_meta::extract_value_u64(s, "created_at"))
                    .or_else(|| {
                        fs::metadata(sd)
                            .ok()
                            .and_then(|m| m.modified().ok())
                            .map(fork_impl_scan::secs_since_epoch)
                    })
                    .unwrap_or(0);
                if created_at == 0 {
                    return false;
                }
                let age = (now.saturating_sub(created_at) / 86400) as u64;
                age >= days
            })
            .collect()
    } else if opts.all {
        fork_impl_scan::session_dirs(&base)
    } else {
        eprintln!(
            "aifo-coder: please specify one of --session <sid>, --older-than <days>, or --all."
        );
        return Ok(1);
    };

    if targets.is_empty() {
        eprintln!("aifo-coder: no matching fork sessions to clean.");
        return Ok(0);
    }

    struct PaneStatus {
        dir: PathBuf,
        clean: bool,
        reasons: Vec<String>,
    }

    let mut plan: Vec<(PathBuf, Vec<PaneStatus>)> = Vec::new();
    for sd in &targets {
        let meta = read_file_to_string(&sd.join(".meta.json"));
        let base_commit = meta
            .as_deref()
            .and_then(|s| crate::fork_meta::extract_value_string(s, "base_commit_sha"));
        let mut panes_status = Vec::new();
        for p in fork_impl_scan::pane_dirs_for_session(sd) {
            let pc = fork_impl_panecheck::pane_check(&p, base_commit.as_deref());
            panes_status.push(PaneStatus {
                dir: p,
                clean: pc.clean,
                reasons: pc.reasons,
            });
        }
        plan.push((sd.clone(), panes_status));
    }

    // If JSON + dry-run requested, print plan and exit before confirmation/execution
    if opts.json && opts.dry_run {
        let mut out = String::from("{\"plan\":true,\"sessions\":[");
        for (idx, (sd, panes)) in plan.iter().enumerate() {
            let sid = sd
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("(unknown)");
            let total = panes.len();
            let clean_count = panes.iter().filter(|ps| ps.clean).count();
            let protected = total.saturating_sub(clean_count);
            // Determine deletion scope per session
            let will_delete_session = if opts.force {
                true
            } else if opts.keep_dirty {
                clean_count == total
            } else {
                true
            };
            if idx > 0 {
                out.push(',');
            }
            out.push_str(&format!(
                "{{\"sid\":{},\"panes_total\":{},\"panes_clean\":{},\"panes_protected\":{},\"will_delete_session\":{}}}",
                json_escape(sid),
                total,
                clean_count,
                protected,
                if will_delete_session { "true" } else { "false" }
            ));
        }
        out.push_str("]}");
        println!("{}", out);
        return Ok(0);
    }

    // Default protection: if any protected pane and neither --force nor --keep-dirty, refuse
    if !opts.force && !opts.keep_dirty {
        let mut protected = 0usize;
        for (_sd, panes) in &plan {
            for ps in panes {
                if !ps.clean {
                    protected += 1;
                }
            }
        }
        if protected > 0 {
            let use_err = color_enabled_stderr();
            eprintln!(
                "{}: {} pane(s) are protected (dirty/ahead/base-unknown).",
                paint(use_err, "\x1b[31;1m", "aifo-coder: refusing to delete"),
                protected
            );
            eprintln!(
                "{}",
                paint(
                    use_err,
                    "\x1b[33m",
                    "Use --keep-dirty to remove only clean panes, or --force to delete everything."
                )
            );
            // Print summary
            for (sd, panes) in &plan {
                let sid = sd
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("(unknown)");
                for ps in panes {
                    if !ps.clean {
                        eprintln!(
                            "  {} :: {} [{}]",
                            paint(use_err, "\x1b[34;1m", sid),
                            ps.dir.display(),
                            paint(use_err, "\x1b[33m", &ps.reasons.join(","))
                        );
                    }
                }
            }
            return Ok(1);
        }
    }

    // Interactive confirmation before deletion (safety prompt)
    if !opts.dry_run && !opts.yes && !opts.json {
        if !atty::is(atty::Stream::Stdin) {
            eprintln!("aifo-coder: refusing to delete without confirmation on non-interactive stdin. Re-run with --yes or --dry-run.");
            return Ok(1);
        }
        let mut del_sessions = 0usize;
        let mut del_panes = 0usize;
        if opts.force {
            del_sessions = plan.len();
            for (_sd, panes) in &plan {
                del_panes += panes.len();
            }
        } else if opts.keep_dirty {
            for (_sd, panes) in &plan {
                let clean_count = panes.iter().filter(|ps| ps.clean).count();
                del_panes += clean_count;
                let remaining = panes.len().saturating_sub(clean_count);
                if remaining == 0 {
                    del_sessions += 1;
                }
            }
        } else {
            del_sessions = plan.len();
            for (_sd, panes) in &plan {
                del_panes += panes.len();
            }
        }
        if del_sessions > 0 || del_panes > 0 {
            let prompt = format!(
                "aifo-coder: about to delete {} session(s) and {} pane(s). Proceed? [y/N] ",
                del_sessions, del_panes
            );
            let use_err = color_enabled_stderr();
            eprint!("{}", paint(use_err, "\x1b[33m", &prompt));
            let _ = std::io::stderr().flush();
            let mut line = String::new();
            let _ = std::io::stdin().read_line(&mut line);
            let ans = line.trim().to_ascii_lowercase();
            if ans != "y" && ans != "yes" {
                eprintln!("aborted.");
                return Ok(1);
            }
        }
    }

    // Execute deletions (or print in dry-run)
    let mut deleted_sessions_count: usize = 0;
    let mut deleted_panes_count: usize = 0;

    for (sd, panes) in &plan {
        let sid = sd
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("(unknown)")
            .to_string();
        if opts.force {
            if opts.dry_run {
                let use_out = color_enabled_stdout();
                println!(
                    "{} {}",
                    paint(use_out, "\x1b[33m", "DRY-RUN:"),
                    paint(use_out, "\x1b[34;1m", &format!("rm -rf {}", sd.display()))
                );
            } else {
                // count all panes removed and the session
                deleted_panes_count += panes.len();
                deleted_sessions_count += 1;
                // Stop toolchain sidecars and remove session network (best-effort)
                toolchain_cleanup_session(&sid, false);
                let _ = fs::remove_dir_all(sd);
                // Success message
                let use_out = color_enabled_stdout();
                println!(
                    "{}",
                    paint(
                        use_out,
                        "\x1b[32;1m",
                        &format!("aifo-coder: deleted fork session {}", sid)
                    )
                );
            }
            continue;
        }
        if opts.keep_dirty {
            let mut remaining: Vec<PathBuf> = Vec::new();
            for ps in panes {
                if ps.clean {
                    if opts.dry_run {
                        let use_out = color_enabled_stdout();
                        println!(
                            "{} {}",
                            paint(use_out, "\x1b[33m", "DRY-RUN:"),
                            paint(
                                use_out,
                                "\x1b[34;1m",
                                &format!("rm -rf {}", ps.dir.display())
                            )
                        );
                    } else {
                        deleted_panes_count += 1;
                        let _ = fs::remove_dir_all(&ps.dir);
                    }
                } else {
                    remaining.push(ps.dir.clone());
                }
            }
            if remaining.is_empty() {
                if opts.dry_run {
                    let use_out = color_enabled_stdout();
                    println!(
                        "{} {}",
                        paint(use_out, "\x1b[33m", "DRY-RUN:"),
                        paint(use_out, "\x1b[34;1m", &format!("rmdir {}", sd.display()))
                    );
                } else {
                    deleted_sessions_count += 1;
                    // Stop toolchain sidecars and remove session network (best-effort)
                    toolchain_cleanup_session(&sid, false);
                    let _ = fs::remove_dir_all(sd);
                    // Success message
                    let use_out = color_enabled_stdout();
                    println!(
                        "{}",
                        paint(
                            use_out,
                            "\x1b[32;1m",
                            &format!("aifo-coder: deleted fork session {}", sid)
                        )
                    );
                }
            } else {
                // Update .meta.json with remaining panes (also refresh branches best-effort)
                if !opts.dry_run {
                    // Collect current branches for remaining panes
                    let mut branches: Vec<String> = Vec::new();
                    for p in &remaining {
                        if let Ok(out) = Command::new("git")
                            .arg("-C")
                            .arg(p)
                            .arg("rev-parse")
                            .arg("--abbrev-ref")
                            .arg("HEAD")
                            .stdout(Stdio::piped())
                            .stderr(Stdio::null())
                            .output()
                        {
                            if out.status.success() {
                                let b = String::from_utf8_lossy(&out.stdout).trim().to_string();
                                if !b.is_empty() {
                                    branches.push(b);
                                }
                            }
                        }
                    }

                    // Enrich metadata with prior fields and use valid JSON escaping
                    let prev = read_file_to_string(&sd.join(".meta.json"));
                    let created_at_num = prev
                        .as_deref()
                        .and_then(|s| crate::fork_meta::extract_value_u64(s, "created_at"))
                        .unwrap_or(0);
                    let base_label_prev = prev
                        .as_deref()
                        .and_then(|s| crate::fork_meta::extract_value_string(s, "base_label"))
                        .unwrap_or_else(|| "(unknown)".to_string());
                    let base_ref_prev = prev
                        .as_deref()
                        .and_then(|s| crate::fork_meta::extract_value_string(s, "base_ref_or_sha"))
                        .unwrap_or_default();
                    let base_commit_prev = prev
                        .as_deref()
                        .and_then(|s| crate::fork_meta::extract_value_string(s, "base_commit_sha"))
                        .unwrap_or_default();
                    let layout_prev = prev
                        .as_deref()
                        .and_then(|s| crate::fork_meta::extract_value_string(s, "layout"))
                        .unwrap_or_else(|| "tiled".to_string());

                    let mut meta_out = String::from("{");
                    meta_out.push_str(&format!("\"sid\":{},", json_escape(&sid)));
                    meta_out.push_str(&format!("\"created_at\":{},", created_at_num));
                    meta_out.push_str(&format!(
                        "\"base_label\":{},",
                        json_escape(&base_label_prev)
                    ));
                    meta_out.push_str(&format!(
                        "\"base_ref_or_sha\":{},",
                        json_escape(&base_ref_prev)
                    ));
                    meta_out.push_str(&format!(
                        "\"base_commit_sha\":{},",
                        json_escape(&base_commit_prev)
                    ));
                    meta_out.push_str(&format!("\"layout\":{},", json_escape(&layout_prev)));
                    meta_out.push_str(&format!("\"panes_remaining\":{},", remaining.len()));
                    meta_out.push_str("\"pane_dirs\":[");
                    for (idx, p) in remaining.iter().enumerate() {
                        if idx > 0 {
                            meta_out.push(',');
                        }
                        meta_out.push_str(&json_escape(&p.display().to_string()));
                    }
                    meta_out.push_str("],\"branches\":[");
                    for (i, b) in branches.iter().enumerate() {
                        if i > 0 {
                            meta_out.push(',');
                        }
                        meta_out.push_str(&json_escape(b));
                    }
                    meta_out.push_str("]}");
                    let _ = fs::write(sd.join(".meta.json"), meta_out);
                    // Kept session summary
                    let use_out = color_enabled_stdout();
                    println!(
                        "{}",
                        paint(
                            use_out,
                            "\x1b[33m",
                            &format!(
                                "aifo-coder: kept fork session {} ({} protected pane(s) remain)",
                                sid,
                                remaining.len()
                            )
                        )
                    );
                }
            }
        } else {
            // all panes are clean here (or we would have bailed above)
            if opts.dry_run {
                let use_out = color_enabled_stdout();
                println!(
                    "{} {}",
                    paint(use_out, "\x1b[33m", "DRY-RUN:"),
                    paint(use_out, "\x1b[34;1m", &format!("rm -rf {}", sd.display()))
                );
            } else {
                deleted_panes_count += panes.len();
                deleted_sessions_count += 1;
                // Stop toolchain sidecars and remove session network (best-effort)
                toolchain_cleanup_session(&sid, false);
                let _ = fs::remove_dir_all(sd);
                // Success message
                let use_out = color_enabled_stdout();
                println!(
                    "{}",
                    paint(
                        use_out,
                        "\x1b[32;1m",
                        &format!("aifo-coder: deleted fork session {}", sid)
                    )
                );
            }
        }
    }

    if !opts.yes && !opts.dry_run {
        // nothing interactive implemented; --yes is accepted to match CLI but not required
    }

    if opts.json && !opts.dry_run {
        println!(
            "{{\"executed\":true,\"deleted_sessions\":{},\"deleted_panes\":{}}}",
            deleted_sessions_count, deleted_panes_count
        );
    }

    Ok(0)
}

/// Print a notice about stale fork sessions for the current repository (quiet; best-effort).
pub fn fork_print_stale_notice() {
    // Delegate to private helper module (no behavior change).
    fork_impl_notice::fork_print_stale_notice_impl();
}

/// Auto-clean clean fork sessions older than the stale threshold when AIFO_CODER_FORK_AUTOCLEAN=1 is set.
/// - Only removes sessions where all panes are clean (no dirty, ahead, or base-unknown panes).
/// - Threshold in days is taken from AIFO_CODER_FORK_STALE_DAYS (default 30).
/// - Prints a concise summary of deletions and survivors.
pub fn fork_autoclean_if_enabled() {
    // Delegate to private helper module (no behavior change).
    fork_impl_notice::fork_autoclean_if_enabled_impl();
}

/// Merge helper: fetch pane branches and optionally octopus-merge them.
pub fn fork_merge_branches(
    repo_root: &Path,
    sid: &str,
    panes: &[(PathBuf, String)],
    base_ref_or_sha: &str,
    strategy: crate::MergingStrategy,
    verbose: bool,
    dry_run: bool,
) -> std::io::Result<()> {
    fork_impl_merge::fork_merge_branches_impl(
        repo_root,
        sid,
        panes,
        base_ref_or_sha,
        strategy,
        verbose,
        dry_run,
    )
}

/// Convenience wrapper: read pane directories and base from session metadata, then merge.
pub fn fork_merge_branches_by_session(
    repo_root: &Path,
    sid: &str,
    strategy: crate::MergingStrategy,
    verbose: bool,
    dry_run: bool,
) -> std::io::Result<()> {
    fork_impl_merge::fork_merge_branches_by_session_impl(repo_root, sid, strategy, verbose, dry_run)
}
