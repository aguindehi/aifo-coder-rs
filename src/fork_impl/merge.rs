/*!
Internal merge helpers extracted from fork.rs.

- collect_pane_branches_impl: determine actual branch names per pane.
- preflight_clean_working_tree_impl: ensure original repo working tree is clean.
- compose_merge_message_impl: build an octopus merge message from pane commit subjects.
- fork_merge_branches_impl: perform fetch-only or octopus merge based on strategy.
- fork_merge_branches_by_session_impl: convenience wrapper to read session metadata and merge.
*/

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::SystemTime;

use crate::{json_escape, shell_join};

pub(crate) fn collect_pane_branches_impl(
    panes: &[(PathBuf, String)],
) -> io::Result<Vec<(PathBuf, String)>> {
    let mut pane_branches: Vec<(PathBuf, String)> = Vec::new();
    for (pdir, branch_hint) in panes {
        let actual_branch = Command::new("git")
            .arg("-C")
            .arg(pdir)
            .arg("rev-parse")
            .arg("--abbrev-ref")
            .arg("HEAD")
            .output()
            .ok()
            .and_then(|o| {
                if o.status.success() {
                    Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                } else {
                    None
                }
            })
            .filter(|s| !s.is_empty() && s.as_str() != "HEAD")
            .unwrap_or_else(|| branch_hint.clone());
        if actual_branch.is_empty() || actual_branch == "HEAD" {
            continue;
        }
        pane_branches.push((pdir.clone(), actual_branch));
    }
    if pane_branches.is_empty() {
        return Err(io::Error::other(
            "no pane branches to process (empty pane set or detached HEAD)",
        ));
    }
    Ok(pane_branches)
}

pub(crate) fn preflight_clean_working_tree_impl(repo_root: &Path) -> io::Result<()> {
    let dirty = match Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .arg("status")
        .arg("--porcelain=v1")
        .arg("-uall")
        .output()
    {
        Ok(o) => {
            if !o.status.success() {
                true
            } else {
                let s = String::from_utf8_lossy(&o.stdout);
                s.lines().any(|line| {
                    let path = if line.len() > 3 { &line[3..] } else { "" };
                    let ignore = path == ".aifo-coder"
                        || path.starts_with(".aifo-coder/")
                        || path.starts_with(".aifo-coder\\");
                    !ignore && !path.is_empty()
                })
            }
        }
        Err(_) => true,
    };
    if dirty {
        return Err(io::Error::other(
            "octopus merge requires a clean working tree in the original repository",
        ));
    }
    Ok(())
}

pub(crate) fn compose_merge_message_impl(
    repo_root: &Path,
    pane_branches: &[(PathBuf, String)],
    base_ref_or_sha: &str,
) -> String {
    let mut merge_message = String::new();

    let mut summary_parts: Vec<String> = Vec::new();
    for (_p, br) in pane_branches {
        let subj_out = Command::new("git")
            .arg("-C")
            .arg(repo_root)
            .arg("log")
            .arg("--no-merges")
            .arg("--pretty=format:%s")
            .arg(format!("{}..{}", base_ref_or_sha, br))
            .output()
            .ok();
        if let Some(o) = subj_out {
            if o.status.success() {
                let body = String::from_utf8_lossy(&o.stdout);
                for s in body.lines() {
                    let mut t = s.trim().to_string();
                    if t.is_empty() {
                        continue;
                    }
                    if let Some(pos) = t.find(':') {
                        let (prefix, rest) = t.split_at(pos);
                        let pref = prefix.to_ascii_lowercase();
                        if [
                            "feat", "fix", "docs", "style", "refactor", "perf", "test", "chore",
                            "build", "ci", "revert",
                        ]
                        .contains(&pref.as_str())
                        {
                            t = rest.trim_start_matches(':').trim().to_string();
                        }
                    }
                    t = t.split_whitespace().collect::<Vec<_>>().join(" ");
                    if t.is_empty() {
                        continue;
                    }
                    if !summary_parts.iter().any(|e| e.eq_ignore_ascii_case(&t)) {
                        summary_parts.push(t);
                    }
                }
            }
        }
    }
    let mut summary_line = if summary_parts.is_empty() {
        format!("Octopus merge of {} branch(es)", pane_branches.len())
    } else {
        let joined = summary_parts.join(" / ");
        if joined.len() > 160 {
            format!("{} …", &joined[..160].trim_end())
        } else {
            joined
        }
    };
    if !summary_line
        .to_ascii_lowercase()
        .starts_with("octopus merge")
    {
        summary_line = format!("Octopus merge: {}", summary_line);
    }
    merge_message.push_str(&format!(
        "{}\n\nBranch summaries relative to {}:\n",
        summary_line, base_ref_or_sha
    ));

    for (_p, br) in pane_branches {
        let log_out = Command::new("git")
            .arg("-C")
            .arg(repo_root)
            .arg("log")
            .arg("--no-merges")
            .arg("--pretty=format:%h %s")
            .arg(format!("{}..{}", base_ref_or_sha, br))
            .output()
            .ok();
        merge_message.push_str(&format!("- branch '{}':\n", br));
        if let Some(o) = log_out {
            if o.status.success() {
                let body = String::from_utf8_lossy(&o.stdout);
                let mut has_any = false;
                for line in body.lines() {
                    let t = line.trim();
                    if !t.is_empty() {
                        has_any = true;
                        merge_message.push_str("  * ");
                        merge_message.push_str(t);
                        merge_message.push('\n');
                    }
                }
                if !has_any {
                    merge_message.push_str("  (no changes)\n");
                }
            } else {
                merge_message.push_str("  (unable to summarize changes)\n");
            }
        } else {
            merge_message.push_str("  (unable to summarize changes)\n");
        }
        merge_message.push('\n');
    }

    merge_message
}

pub(crate) fn fork_merge_branches_impl(
    repo_root: &Path,
    sid: &str,
    panes: &[(PathBuf, String)],
    base_ref_or_sha: &str,
    strategy: crate::MergingStrategy,
    verbose: bool,
    dry_run: bool,
) -> io::Result<()> {
    if matches!(strategy, crate::MergingStrategy::None) {
        return Ok(());
    }

    let pane_branches = collect_pane_branches_impl(panes)?;

    // 1) Fetch each pane branch back into the original repo as a local branch with the same name
    for (pdir, br) in &pane_branches {
        let pdir_str = pdir.display().to_string();
        let refspec = format!("{b}:refs/heads/{b}", b = br);
        let args = vec![
            "git".to_string(),
            "-C".to_string(),
            repo_root.display().to_string(),
            "-c".to_string(),
            "protocol.file.allow=always".to_string(),
            "fetch".to_string(),
            "--no-tags".to_string(),
            pdir_str.clone(),
            refspec.clone(),
        ];
        if verbose || dry_run {
            eprintln!("aifo-coder: git: {}", shell_join(&args));
        }
        if !dry_run {
            let mut cmd = Command::new("git");
            cmd.arg("-C")
                .arg(repo_root)
                .arg("-c")
                .arg("protocol.file.allow=always")
                .arg("fetch")
                .arg("--no-tags")
                .arg(&pdir_str)
                .arg(&refspec);
            if !verbose {
                cmd.stdout(Stdio::null()).stderr(Stdio::null());
            }
            let st = cmd.status()?;
            if !st.success() {
                return Err(io::Error::other(format!(
                    "git fetch failed for pane {} (branch {})",
                    pdir.display(),
                    br
                )));
            }
        }
    }

    if matches!(strategy, crate::MergingStrategy::Fetch) {
        // Update session metadata with fetched branches
        let fetched_names: Vec<String> = pane_branches.iter().map(|(_p, b)| b.clone()).collect();
        let _ = crate::fork_meta::append_fields_compact(
            repo_root,
            sid,
            &format!(
                "\"merge_strategy\":{},\"fetched\":[{}]",
                json_escape("fetch"),
                fetched_names
                    .iter()
                    .map(|b| json_escape(b))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        );
        return Ok(());
    }

    preflight_clean_working_tree_impl(repo_root)?;

    // Checkout or create merge/<sid> at base_ref_or_sha
    let target = format!("merge/{}", sid);
    let checkout_args = vec![
        "git".to_string(),
        "-C".to_string(),
        repo_root.display().to_string(),
        "checkout".to_string(),
        "-B".to_string(),
        target.clone(),
        base_ref_or_sha.to_string(),
    ];
    if verbose || dry_run {
        eprintln!("aifo-coder: git: {}", shell_join(&checkout_args));
    }
    if !dry_run {
        let st = Command::new("git")
            .arg("-C")
            .arg(repo_root)
            .arg("checkout")
            .arg("-B")
            .arg(&target)
            .arg(base_ref_or_sha)
            .status()?;
        if !st.success() {
            return Err(io::Error::other(
                "failed to checkout merge target branch",
            ));
        }
    }

    let merge_message = compose_merge_message_impl(repo_root, &pane_branches, base_ref_or_sha);

    // Prepare merge message file path; write it unless dry_run
    let mut merge_msg_path: Option<std::path::PathBuf> = None;
    let msg_path =
        std::env::temp_dir().join(format!("aifo-merge-{}-{}.txt", sid, std::process::id()));
    if verbose {
        eprintln!(
            "aifo-coder: preparing octopus merge message at {}",
            msg_path.display()
        );
    }
    if !dry_run {
        if fs::write(&msg_path, &merge_message).is_ok() {
            merge_msg_path = Some(msg_path.clone());
        } else if verbose {
            eprintln!("aifo-coder: warning: failed to write merge message file; falling back to default message");
        }
    }

    // Perform octopus merge (use -F <file> when available)
    let mut merge_args = vec![
        "git".to_string(),
        "-C".to_string(),
        repo_root.display().to_string(),
        "merge".to_string(),
        "--no-ff".to_string(),
        "--no-edit".to_string(),
    ];
    if merge_msg_path.is_some() || dry_run {
        merge_args.push("-F".to_string());
        merge_args.push(msg_path.display().to_string());
    }
    for (_p, br) in &pane_branches {
        merge_args.push(br.clone());
    }
    if verbose || dry_run {
        eprintln!("aifo-coder: git: {}", shell_join(&merge_args));
    }
    if !dry_run {
        let mut cmd = Command::new("git");
        cmd.arg("-C")
            .arg(repo_root)
            .arg("merge")
            .arg("--no-ff")
            .arg("--no-edit");
        if let Some(ref p) = merge_msg_path {
            cmd.arg("-F").arg(p);
        }
        for (_p, br) in &pane_branches {
            cmd.arg(br);
        }
        let st = cmd.status()?;
        // cleanup temp file best-effort
        if let Some(p) = merge_msg_path {
            let _ = fs::remove_file(p);
        }
        if !st.success() {
            // Record failed octopus merge in metadata
            let fetched_names: Vec<String> =
                pane_branches.iter().map(|(_p, b)| b.clone()).collect();
            let _ = crate::fork_meta::append_fields_compact(
                repo_root,
                sid,
                &format!(
                    "\"merge_strategy\":{},\"fetched\":[{}],\"merge_failed\":true",
                    json_escape("octopus"),
                    fetched_names
                        .iter()
                        .map(|b| json_escape(b))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            );
            return Err(io::Error::other(
                "octopus merge failed (conflicts likely). Resolve manually and retry.",
            ));
        }
    }

    // Update session metadata with octopus merge result
    {
        let fetched_names: Vec<String> = pane_branches.iter().map(|(_p, b)| b.clone()).collect();
        let merge_commit_sha = if !dry_run {
            Command::new("git")
                .arg("-C")
                .arg(repo_root)
                .arg("rev-parse")
                .arg("--verify")
                .arg("HEAD")
                .output()
                .ok()
                .and_then(|o| {
                    if o.status.success() {
                        Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                    } else {
                        None
                    }
                })
                .unwrap_or_default()
        } else {
            String::new()
        };
        let merged_at = super::fork_impl_scan::secs_since_epoch(SystemTime::now());
        let _ = crate::fork_meta::append_fields_compact(
            repo_root,
            sid,
            &format!(
                "\"merge_strategy\":{},\"fetched\":[{}],\"merge_target\":{},\"merge_commit_sha\":{},\"merged_at\":{}",
                json_escape("octopus"),
                fetched_names
                    .iter()
                    .map(|b| json_escape(b))
                    .collect::<Vec<_>>()
                    .join(", "),
                json_escape(&format!("merge/{}", sid)),
                json_escape(&merge_commit_sha),
                merged_at
            ),
        );
    }

    // Delete merged pane branches locally (best-effort)
    if !dry_run {
        let mut del = Command::new("git");
        del.arg("-C").arg(repo_root).arg("branch").arg("-D");
        for (_p, br) in &pane_branches {
            del.arg(br);
        }
        if !verbose {
            del.stdout(Stdio::null()).stderr(Stdio::null());
        }
        let _ = del.status();
    } else if verbose {
        let mut preview = vec![
            "git".to_string(),
            "-C".to_string(),
            repo_root.display().to_string(),
            "branch".to_string(),
            "-D".to_string(),
        ];
        for (_p, br) in &pane_branches {
            preview.push(br.clone());
        }
        eprintln!("aifo-coder: git: {}", shell_join(&preview));
    }

    Ok(())
}

pub(crate) fn fork_merge_branches_by_session_impl(
    repo_root: &Path,
    sid: &str,
    strategy: crate::MergingStrategy,
    verbose: bool,
    dry_run: bool,
) -> io::Result<()> {
    if matches!(strategy, crate::MergingStrategy::None) {
        return Ok(());
    }
    let session_dir = super::fork_session_dir(repo_root, sid);
    if !session_dir.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "fork session directory not found: {}",
                session_dir.display()
            ),
        ));
    }

    // Gather pane dirs
    let panes_dirs = super::fork_impl_scan::pane_dirs_for_session(&session_dir);
    if panes_dirs.is_empty() {
        return Err(io::Error::other(
            "no pane directories found under session",
        ));
    }

    // Determine base_ref_or_sha from .meta.json if present; otherwise fallback to HEAD
    let meta_path = session_dir.join(".meta.json");
    let meta = fs::read_to_string(&meta_path).ok();
    let base_ref_or_sha = meta
        .as_deref()
        .and_then(|s| crate::fork_meta::extract_value_string(s, "base_ref_or_sha"))
        .unwrap_or_else(|| "HEAD".to_string());

    // Determine each pane's current branch
    let mut panes: Vec<(PathBuf, String)> = Vec::new();
    for p in panes_dirs {
        let branch = Command::new("git")
            .arg("-C")
            .arg(&p)
            .arg("rev-parse")
            .arg("--abbrev-ref")
            .arg("HEAD")
            .output()
            .ok()
            .and_then(|o| {
                if o.status.success() {
                    Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                } else {
                    None
                }
            })
            .unwrap_or_default();
        if !branch.is_empty() && branch != "HEAD" {
            panes.push((p, branch));
        }
    }

    if panes.is_empty() {
        return Err(io::Error::other(
            "no pane branches found (detached HEAD?)",
        ));
    }

    super::fork_merge_branches(
        repo_root,
        sid,
        &panes,
        &base_ref_or_sha,
        strategy,
        verbose,
        dry_run,
    )
}
