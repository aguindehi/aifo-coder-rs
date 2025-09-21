#![allow(clippy::module_name_repetitions)]
//! Fork orchestrator for launching panes (tmux on Unix; Windows Terminal/PowerShell/Git Bash on Windows).
//! This is binary-side code that leverages the public library facade (aifo_coder::*) and keeps
//! user-visible behavior unchanged.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, ExitCode};
use which::which;

use crate::cli::{Agent, Cli};
use crate::fork_args::fork_build_child_args;
use crate::guidance::print_inspect_merge_guidance;

// Orchestrate tmux-based fork session (Linux/macOS/WSL) — moved from main.rs (Phase 1)
#[allow(clippy::needless_return)]
pub fn fork_run(cli: &Cli, panes: usize) -> ExitCode {
    // Pre-compute stderr color usage once per run
    let use_err_color = aifo_coder::color_enabled_stderr();
    // Preflight
    if let Err(code) = crate::fork::preflight::ensure_git_and_orchestrator_present_on_platform() {
        return code;
    }
    let repo_root = match aifo_coder::repo_root() {
        Some(p) => p,
        None => {
            eprintln!("aifo-coder: error: fork mode must be run inside a Git repository.");
            return ExitCode::from(1);
        }
    };
    if let Err(code) = crate::fork::preflight::guard_panes_count_and_prompt(panes) {
        return code;
    }

    // Identify base
    let (base_label, mut base_ref_or_sha, base_commit_sha) =
        match aifo_coder::fork_base_info(&repo_root) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("aifo-coder: error determining base: {}", e);
                return ExitCode::from(1);
            }
        };

    // Session id and name
    let sid = aifo_coder::create_session_id();
    let session_name = cli
        .fork_session_name
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("aifo-{}", sid));

    // Snapshot when requested
    let mut snapshot_sha: Option<String> = None;
    if cli.fork_include_dirty {
        match aifo_coder::fork_create_snapshot(&repo_root, &sid) {
            Ok(sha) => {
                snapshot_sha = Some(sha.clone());
                base_ref_or_sha = sha;
            }
            Err(e) => {
                let msg = format!("failed to create snapshot of dirty working tree: {}", e);
                if !aifo_coder::warn_prompt_continue_or_quit(&[
                    &msg,
                    "the fork panes will not include your uncommitted changes.",
                ]) {
                    return ExitCode::from(1);
                }
            }
        }
    } else {
        // Warn if dirty but not including
        let dirty = aifo_coder::fork_impl_git::git_status_porcelain(&repo_root)
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false);
        if dirty
            && !aifo_coder::warn_prompt_continue_or_quit(&[
                "working tree has uncommitted changes; they will not be included in the fork panes.",
                "re-run with --fork-include-dirty to include them.",
            ])
        {
            return ExitCode::from(1);
        }
    }

    // Preflight: if octopus merging requested, ensure original repo is clean to avoid hidden merge failures
    if matches!(
        cli.fork_merging_strategy,
        aifo_coder::MergingStrategy::Octopus
    ) {
        let dirty_oct = aifo_coder::fork_impl_git::git_status_porcelain(&repo_root)
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false);
        if dirty_oct
            && !aifo_coder::warn_prompt_continue_or_quit(&[
                "octopus merge requires a clean working tree in the original repository.",
                "commit or stash your changes before proceeding, or merging will likely fail.",
            ])
        {
            return ExitCode::from(1);
        }
    }
    // Preflight: warn once about missing toolchains and allow abort
    {
        let agent_for_warn = match &cli.command {
            Agent::Codex { .. } => "codex",
            Agent::Crush { .. } => "crush",
            Agent::Aider { .. } => "aider",
            _ => "aider",
        };
        if !crate::warnings::maybe_warn_missing_toolchain_for_fork(cli, agent_for_warn) {
            return ExitCode::from(1);
        }
    }
    // Create clones
    let dissoc = cli.fork_dissociate;
    let _opts = crate::fork::types::ForkOptions {
        verbose: cli.verbose,
        keep_on_failure: cli.fork_keep_on_failure,
        merge_strategy: cli.fork_merging_strategy,
        autoclean: cli.fork_merging_autoclean,
        dry_run: cli.dry_run,
        include_dirty: cli.fork_include_dirty,
        dissociate: cli.fork_dissociate,
    };
    // Touch fields so clippy considers them read without changing behavior.
    _opts.touch();
    let clones = match aifo_coder::fork_clone_and_checkout_panes(
        &repo_root,
        &sid,
        panes,
        &base_ref_or_sha,
        &base_label,
        dissoc,
    ) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("aifo-coder: error during cloning: {}", e);
            return ExitCode::from(1);
        }
    };

    // Prepare per-pane env/state dirs
    let agent = match &cli.command {
        Agent::Codex { .. } => "codex",
        Agent::Crush { .. } => "crush",
        Agent::Aider { .. } => "aider",
        _ => "aider",
    };
    let state_base = env::var("AIFO_CODER_FORK_STATE_BASE")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            home::home_dir()
                .unwrap_or_else(|| PathBuf::from("~"))
                .join(".aifo-coder")
                .join("state")
        });
    let session_dir = aifo_coder::fork_session_dir(&repo_root, &sid);

    // Summary header
    let use_color_out = atty::is(atty::Stream::Stdout);
    crate::fork::summary::print_header(
        &sid,
        &base_label,
        &base_ref_or_sha,
        &session_dir,
        panes,
        snapshot_sha.as_deref(),
        cli.fork_include_dirty,
        !dissoc,
        use_color_out,
    );

    // Per-pane run
    let child_args = fork_build_child_args(cli);
    let layout = cli.fork_layout.as_deref().unwrap_or("tiled").to_string();
    let layout_effective = match layout.as_str() {
        "even-h" => "even-horizontal".to_string(),
        "even-v" => "even-vertical".to_string(),
        _ => "tiled".to_string(),
    };
    if cli.verbose {
        eprintln!(
            "aifo-coder: tmux layout requested: {} -> effective: {}",
            layout, layout_effective
        );
    }

    // Write metadata skeleton
    let created_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_else(|_| std::time::Duration::from_secs(0))
        .as_secs();
    // Determine the commit SHA used as the checkout base for metadata:
    // - Use snapshot SHA when include-dirty snapshot was created
    // - Otherwise resolve base_ref_or_sha to a SHA (branch or SHA), fall back to HEAD SHA from fork_base_info
    let base_commit_sha_for_meta = if let Some(ref snap) = snapshot_sha {
        snap.clone()
    } else {
        let mut cmd = aifo_coder::fork_impl_git::git_cmd(Some(&repo_root));
        cmd.arg("rev-parse")
            .arg("--verify")
            .arg(&base_ref_or_sha)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null());
        cmd.output()
            .ok()
            .and_then(|o| {
                if o.status.success() {
                    Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                } else {
                    None
                }
            })
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| base_commit_sha.clone())
    };
    // Shadow base_commit_sha so existing metadata builders pick the correct SHA
    let base_commit_sha = base_commit_sha_for_meta.clone();
    // Write initial session metadata using helper
    let meta_obj = crate::fork::meta::SessionMeta {
        created_at,
        base_label: &base_label,
        base_ref_or_sha: &base_ref_or_sha,
        base_commit_sha: String::new(),
        panes,
        pane_dirs: clones.iter().map(|(p, _)| p.clone()).collect(),
        branches: clones.iter().map(|(_, b)| b.clone()).collect(),
        layout: &layout,
        snapshot_sha: snapshot_sha.as_deref(),
    };
    let _ = crate::fork::meta::write_initial_meta(&repo_root, &sid, &meta_obj);

    // Print per-pane info lines
    crate::fork::summary::print_per_pane_blocks(agent, &sid, &state_base, &clones, use_color_out);

    // Orchestrate panes (Windows uses Windows Terminal or PowerShell; Unix-like uses tmux)
    if cfg!(target_os = "windows") {
        // Helper to PowerShell-quote a single token
        let ps_quote = |s: &str| -> String {
            let esc = s.replace('\'', "''");
            format!("'{}'", esc)
        };

        // Orchestrator preference override (optional): AIFO_CODER_FORK_ORCH={gitbash|powershell}
        let orch_pref = env::var("AIFO_CODER_FORK_ORCH")
            .ok()
            .map(|s| s.to_ascii_lowercase());
        if orch_pref.as_deref() == Some("gitbash") {
            // Force Git Bash orchestrator if available
            let gitbash = which("git-bash.exe").or_else(|_| which("bash.exe"));
            if let Ok(gb) = gitbash {
                let mut any_failed = false;
                for (idx, (pane_dir, _b)) in clones.iter().enumerate() {
                    let i = idx + 1;
                    let pane_state_dir = state_base.join(&sid).join(format!("pane-{}", i));
                    let session = crate::fork::session::make_session(
                        &sid,
                        &session_name,
                        &base_label,
                        &base_ref_or_sha,
                        &base_commit_sha,
                        created_at,
                        &layout,
                        agent,
                        &session_dir,
                    );
                    let container_name = crate::fork::env::pane_container_name(agent, &sid, i);
                    let pane = crate::fork::session::make_pane(
                        i,
                        pane_dir.as_path(),
                        _b,
                        &pane_state_dir,
                        &container_name,
                    );
                    // Touch pane fields to mark them as intentionally used
                    let _ = (
                        pane.index,
                        &pane.dir,
                        &pane.branch,
                        &pane.state_dir,
                        &pane.container_name,
                    );
                    let exec_shell_tail =
                        matches!(cli.fork_merging_strategy, aifo_coder::MergingStrategy::None);
                    let inner = crate::fork::inner::build_inner_gitbash(
                        &session.agent,
                        &session.sid,
                        i,
                        pane_dir.as_path(),
                        &pane_state_dir,
                        &child_args,
                        exec_shell_tail,
                    );

                    let mut cmd = Command::new(&gb);
                    cmd.arg("-c").arg(&inner);
                    if cli.verbose {
                        let preview =
                            vec![gb.display().to_string(), "-c".to_string(), inner.clone()];
                        eprintln!("aifo-coder: git-bash: {}", aifo_coder::shell_join(&preview));
                    }
                    match cmd.status() {
                        Ok(s) if s.success() => {}
                        _ => {
                            any_failed = true;
                            break;
                        }
                    }
                }

                if any_failed {
                    eprintln!("aifo-coder: failed to launch one or more Git Bash windows.");
                    crate::fork::cleanup::cleanup_and_update_meta(
                        &repo_root,
                        &sid,
                        &clones,
                        cli.fork_keep_on_failure,
                        &session_dir,
                        snapshot_sha.as_deref(),
                        &layout,
                        false,
                    );
                    return ExitCode::from(1);
                }

                // Apply post-fork merging if requested, then print guidance
                if crate::fork::post_merge::apply_post_merge(
                    &repo_root,
                    &sid,
                    cli.fork_merging_strategy,
                    cli.fork_merging_autoclean,
                    cli.dry_run,
                    cli.verbose,
                    false,
                )
                .is_err()
                {
                    // errors already logged
                }
                println!();
                println!("aifo-coder: fork session {} launched (Git Bash).", sid);
                print_inspect_merge_guidance(
                    &repo_root,
                    &sid,
                    &base_label,
                    &base_ref_or_sha,
                    &clones,
                    false,
                    true,
                    false,
                );
                return ExitCode::from(0);
            } else if let Ok(mt) = which("mintty.exe") {
                // Use mintty as a Git Bash UI launcher
                let mut any_failed = false;
                for (idx, (pane_dir, _b)) in clones.iter().enumerate() {
                    let i = idx + 1;
                    let pane_state_dir = state_base.join(&sid).join(format!("pane-{}", i));
                    let session = crate::fork::session::make_session(
                        &sid,
                        &session_name,
                        &base_label,
                        &base_ref_or_sha,
                        &base_commit_sha,
                        created_at,
                        &layout,
                        agent,
                        &session_dir,
                    );
                    let container_name = crate::fork::env::pane_container_name(agent, &sid, i);
                    let pane = crate::fork::session::make_pane(
                        i,
                        pane_dir.as_path(),
                        _b,
                        &pane_state_dir,
                        &container_name,
                    );
                    // Touch pane fields to mark them as intentionally used
                    let _ = (
                        pane.index,
                        &pane.dir,
                        &pane.branch,
                        &pane.state_dir,
                        &pane.container_name,
                    );
                    let exec_shell_tail =
                        matches!(cli.fork_merging_strategy, aifo_coder::MergingStrategy::None);
                    let inner = crate::fork::inner::build_inner_gitbash(
                        &session.agent,
                        &session.sid,
                        i,
                        pane_dir.as_path(),
                        &pane_state_dir,
                        &child_args,
                        exec_shell_tail,
                    );

                    let mut cmd = Command::new(&mt);
                    cmd.arg("-e").arg("bash").arg("-lc").arg(&inner);
                    if cli.verbose {
                        let preview = vec![
                            mt.display().to_string(),
                            "-e".to_string(),
                            "bash".to_string(),
                            "-lc".to_string(),
                            inner.clone(),
                        ];
                        eprintln!("aifo-coder: mintty: {}", aifo_coder::shell_join(&preview));
                    }
                    match cmd.status() {
                        Ok(s) if s.success() => {}
                        _ => {
                            any_failed = true;
                            break;
                        }
                    }
                }

                if any_failed {
                    eprintln!("aifo-coder: failed to launch one or more mintty windows.");
                    crate::fork::cleanup::cleanup_and_update_meta(
                        &repo_root,
                        &sid,
                        &clones,
                        cli.fork_keep_on_failure,
                        &session_dir,
                        snapshot_sha.as_deref(),
                        &layout,
                        false,
                    );
                    return ExitCode::from(1);
                }

                // Apply post-fork merging if requested, then print guidance
                if crate::fork::post_merge::apply_post_merge(
                    &repo_root,
                    &sid,
                    cli.fork_merging_strategy,
                    cli.fork_merging_autoclean,
                    cli.dry_run,
                    cli.verbose,
                    false,
                )
                .is_err()
                {
                    // errors already logged
                }
                println!();
                println!("aifo-coder: fork session {} launched (mintty).", sid);
                print_inspect_merge_guidance(
                    &repo_root,
                    &sid,
                    &base_label,
                    &base_ref_or_sha,
                    &clones,
                    false,
                    true,
                    false,
                );
                return ExitCode::from(0);
            } else {
                eprintln!("aifo-coder: error: AIFO_CODER_FORK_ORCH=gitbash requested but Git Bash/mintty were not found in PATH.");
                return ExitCode::from(1);
            }
        } else if orch_pref.as_deref() == Some("powershell") {
            // Fall through to PowerShell windows launcher below, bypassing Windows Terminal
        }
        // Prefer Windows Terminal (wt.exe)
        let wt = which("wt").or_else(|_| which("wt.exe"));
        if let Ok(wtbin) = wt {
            if !matches!(cli.fork_merging_strategy, aifo_coder::MergingStrategy::None) {
                {
                    eprintln!(
                        "{}",
                        aifo_coder::paint(
                            use_err_color,
                            "\x1b[33m",
                            concat!(
                                "aifo-coder: using PowerShell windows to enable post-fork merging ",
                                "(--fork-merge-strategy)."
                            )
                        )
                    );
                }
            } else {
                if let Err(code) = crate::fork::preflight::guard_no_panes(clones.len()) {
                    return code;
                }
                let psbin = which("pwsh")
                    .or_else(|_| which("powershell"))
                    .or_else(|_| which("powershell.exe"))
                    .unwrap_or_else(|_| std::path::PathBuf::from("powershell"));
                let orient_for_layout = |i: usize| -> &'static str {
                    match layout.as_str() {
                        "even-h" => "-H",
                        "even-v" => "-V",
                        _ => {
                            // tiled: alternate for some balance
                            if i.is_multiple_of(2) {
                                "-H"
                            } else {
                                "-V"
                            }
                        }
                    }
                };

                // Pane 1: new tab
                {
                    let (pane1_dir, _b) = &clones[0];
                    let pane_state_dir = state_base.join(&sid).join("pane-1");
                    let session = crate::fork::session::make_session(
                        &sid,
                        &session_name,
                        &base_label,
                        &base_ref_or_sha,
                        &base_commit_sha,
                        created_at,
                        &layout,
                        agent,
                        &session_dir,
                    );
                    let container_name = crate::fork::env::pane_container_name(agent, &sid, 1);
                    let pane = crate::fork::session::make_pane(
                        1,
                        pane1_dir.as_path(),
                        _b,
                        &pane_state_dir,
                        &container_name,
                    );
                    // Touch pane fields to mark them as intentionally used
                    let _ = (
                        pane.index,
                        &pane.dir,
                        &pane.branch,
                        &pane.state_dir,
                        &pane.container_name,
                    );
                    let inner = crate::fork::inner::build_inner_powershell(
                        &session.agent,
                        &session.sid,
                        1,
                        pane1_dir.as_path(),
                        &pane_state_dir,
                        &child_args,
                    );
                    let mut cmd = Command::new(&wtbin);
                    cmd.arg("new-tab")
                        .arg("-d")
                        .arg(pane1_dir)
                        .arg(&psbin)
                        .arg("-NoExit")
                        .arg("-Command")
                        .arg(&inner);
                    if cli.verbose {
                        #[cfg(windows)]
                        {
                            let preview = aifo_coder::wt_build_new_tab_args(
                                &psbin,
                                pane1_dir.as_path(),
                                &inner,
                            );
                            eprintln!(
                                "aifo-coder: windows-terminal: {}",
                                aifo_coder::shell_join(&preview)
                            );
                        }
                    }
                    match cmd.status() {
                        Ok(s) if s.success() => {}
                        Ok(_) => {
                            eprintln!("aifo-coder: Windows Terminal failed to start first pane (non-zero exit).");
                            crate::fork::cleanup::cleanup_and_update_meta(
                                &repo_root,
                                &sid,
                                &clones,
                                cli.fork_keep_on_failure,
                                &session_dir,
                                snapshot_sha.as_deref(),
                                &layout,
                                false,
                            );
                            return ExitCode::from(1);
                        }
                        Err(e) => {
                            eprintln!(
                                "aifo-coder: Windows Terminal failed to start first pane: {}",
                                e
                            );
                            crate::fork::cleanup::cleanup_and_update_meta(
                                &repo_root,
                                &sid,
                                &clones,
                                cli.fork_keep_on_failure,
                                &session_dir,
                                snapshot_sha.as_deref(),
                                &layout,
                                false,
                            );
                            return ExitCode::from(1);
                        }
                    }
                }

                // Additional panes: split-pane
                let mut split_failed = false;
                for (idx, (pane_dir, _b)) in clones.iter().enumerate().skip(1) {
                    let i = idx + 1;
                    let pane_state_dir = state_base.join(&sid).join(format!("pane-{}", i));
                    let session = crate::fork::session::make_session(
                        &sid,
                        &session_name,
                        &base_label,
                        &base_ref_or_sha,
                        &base_commit_sha,
                        created_at,
                        &layout,
                        agent,
                        &session_dir,
                    );
                    let container_name = crate::fork::env::pane_container_name(agent, &sid, i);
                    let pane = crate::fork::session::make_pane(
                        i,
                        pane_dir.as_path(),
                        _b,
                        &pane_state_dir,
                        &container_name,
                    );
                    // Touch pane fields to mark them as intentionally used
                    let _ = (
                        pane.index,
                        &pane.dir,
                        &pane.branch,
                        &pane.state_dir,
                        &pane.container_name,
                    );
                    let inner = crate::fork::inner::build_inner_powershell(
                        &session.agent,
                        &session.sid,
                        i,
                        pane_dir.as_path(),
                        &pane_state_dir,
                        &child_args,
                    );
                    let orient = orient_for_layout(i);
                    let mut cmd = Command::new(&wtbin);
                    cmd.arg("split-pane")
                        .arg(orient)
                        .arg("-d")
                        .arg(pane_dir)
                        .arg(&psbin)
                        .arg("-NoExit")
                        .arg("-Command")
                        .arg(&inner);
                    if cli.verbose {
                        #[cfg(windows)]
                        {
                            let preview = aifo_coder::wt_build_split_args(
                                orient,
                                &psbin,
                                pane_dir.as_path(),
                                &inner,
                            );
                            eprintln!(
                                "aifo-coder: windows-terminal: {}",
                                aifo_coder::shell_join(&preview)
                            );
                        }
                    }
                    match cmd.status() {
                        Ok(s) if s.success() => {}
                        _ => {
                            split_failed = true;
                            break;
                        }
                    }
                }
                if split_failed {
                    eprintln!(
                        "aifo-coder: Windows Terminal split-pane failed for one or more panes."
                    );
                    crate::fork::cleanup::cleanup_and_update_meta(
                        &repo_root,
                        &sid,
                        &clones,
                        cli.fork_keep_on_failure,
                        &session_dir,
                        snapshot_sha.as_deref(),
                        &layout,
                        true,
                    );
                    return ExitCode::from(1);
                }

                // Print guidance and return (wt.exe is detached)
                println!();
                println!(
                    "aifo-coder: fork session {} launched in Windows Terminal.",
                    sid
                );
                print_inspect_merge_guidance(
                    &repo_root,
                    &sid,
                    &base_label,
                    &base_ref_or_sha,
                    &clones,
                    false,
                    false,
                    true,
                );
                return ExitCode::from(0);
            }
        }

        // Fallback: separate PowerShell windows via cmd.exe start
        let powershell = which("pwsh")
            .or_else(|_| which("powershell"))
            .or_else(|_| which("powershell.exe"));
        if powershell.is_err() {
            // Fallback: Git Bash (Git Shell / mintty)
            let gitbash = which("git-bash.exe").or_else(|_| which("bash.exe"));
            if let Ok(gb) = gitbash {
                let mut any_failed = false;
                for (idx, (pane_dir, _b)) in clones.iter().enumerate() {
                    let i = idx + 1;
                    let pane_state_dir = state_base.join(&sid).join(format!("pane-{}", i));
                    let exec_shell_tail =
                        matches!(cli.fork_merging_strategy, aifo_coder::MergingStrategy::None);
                    let session = crate::fork::session::make_session(
                        &sid,
                        &session_name,
                        &base_label,
                        &base_ref_or_sha,
                        &base_commit_sha,
                        created_at,
                        &layout,
                        agent,
                        &session_dir,
                    );
                    let container_name = crate::fork::env::pane_container_name(agent, &sid, i);
                    let pane = crate::fork::session::make_pane(
                        i,
                        pane_dir.as_path(),
                        _b,
                        &pane_state_dir,
                        &container_name,
                    );
                    // Touch pane fields to mark them as intentionally used
                    let _ = (
                        pane.index,
                        &pane.dir,
                        &pane.branch,
                        &pane.state_dir,
                        &pane.container_name,
                    );
                    let inner = crate::fork::inner::build_inner_gitbash(
                        &session.agent,
                        &session.sid,
                        i,
                        pane_dir.as_path(),
                        &pane_state_dir,
                        &child_args,
                        exec_shell_tail,
                    );

                    let mut cmd = Command::new(&gb);
                    cmd.arg("-c").arg(&inner);
                    if cli.verbose {
                        let preview =
                            vec![gb.display().to_string(), "-c".to_string(), inner.clone()];
                        eprintln!("aifo-coder: git-bash: {}", aifo_coder::shell_join(&preview));
                    }
                    match cmd.status() {
                        Ok(s) if s.success() => {}
                        _ => {
                            any_failed = true;
                            break;
                        }
                    }
                }

                if any_failed {
                    eprintln!("aifo-coder: failed to launch one or more Git Bash windows.");
                    crate::fork::cleanup::cleanup_and_update_meta(
                        &repo_root,
                        &sid,
                        &clones,
                        cli.fork_keep_on_failure,
                        &session_dir,
                        snapshot_sha.as_deref(),
                        &layout,
                        false,
                    );
                    return ExitCode::from(1);
                }

                // Apply post-fork merging if requested, then print guidance
                if !matches!(cli.fork_merging_strategy, aifo_coder::MergingStrategy::None) {
                    let strat = match cli.fork_merging_strategy {
                        aifo_coder::MergingStrategy::None => "none",
                        aifo_coder::MergingStrategy::Fetch => "fetch",
                        aifo_coder::MergingStrategy::Octopus => "octopus",
                    };
                    {
                        eprintln!(
                            "{}",
                            aifo_coder::paint(
                                use_err_color,
                                "\x1b[36;1m",
                                &format!(
                                    "aifo-coder: applying post-fork merge strategy: {}",
                                    strat
                                )
                            )
                        );
                    }
                    match aifo_coder::fork_merge_branches_by_session(
                        &repo_root,
                        &sid,
                        cli.fork_merging_strategy,
                        cli.verbose,
                        cli.dry_run,
                    ) {
                        Ok(()) => {
                            {
                                let use_err = aifo_coder::color_enabled_stderr();
                                eprintln!(
                                    "{}",
                                    aifo_coder::paint(
                                        use_err,
                                        "\x1b[32;1m",
                                        &format!(
                                            "aifo-coder: merge strategy '{}' completed.",
                                            strat
                                        )
                                    )
                                );
                            }
                            if matches!(
                                cli.fork_merging_strategy,
                                aifo_coder::MergingStrategy::Octopus
                            ) && cli.fork_merging_autoclean
                                && !cli.dry_run
                            {
                                eprintln!();
                                eprintln!(
                                    "aifo-coder: octopus merge succeeded; disposing fork session {} ...",
                                    sid
                                );
                                let opts = aifo_coder::ForkCleanOpts {
                                    session: Some(sid.clone()),
                                    older_than_days: None,
                                    all: false,
                                    dry_run: false,
                                    yes: true,
                                    force: true,
                                    keep_dirty: false,
                                    json: false,
                                };
                                match aifo_coder::fork_clean(&repo_root, &opts) {
                                    Ok(_) => {
                                        eprintln!(
                                            "{}",
                                            aifo_coder::paint(
                                                use_err_color,
                                                "\x1b[32;1m",
                                                &format!(
                                                    "aifo-coder: disposed fork session {}.",
                                                    sid
                                                )
                                            )
                                        );
                                    }
                                    Err(e) => {
                                        eprintln!(
                                            "{}",
                                            aifo_coder::paint(
                                                use_err_color,
                                                "\x1b[33m",
                                                &format!(
                                                    "aifo-coder: warning: failed to dispose fork session {}: {}",
                                                    sid, e
                                                )
                                            )
                                        );
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!(
                                "{}",
                                aifo_coder::paint(
                                    use_err_color,
                                    "\x1b[31;1m",
                                    &format!(
                                        "aifo-coder: merge strategy '{}' failed: {}",
                                        strat, e
                                    )
                                )
                            );
                        }
                    }
                }
                println!();
                println!("aifo-coder: fork session {} launched (Git Bash).", sid);
                print_inspect_merge_guidance(
                    &repo_root,
                    &sid,
                    &base_label,
                    &base_ref_or_sha,
                    &clones,
                    false,
                    false,
                    true,
                );
                return ExitCode::from(0);
            } else if let Ok(mt) = which("mintty.exe") {
                // Use mintty as a Git Bash UI launcher
                let mut any_failed = false;
                for (idx, (pane_dir, _b)) in clones.iter().enumerate() {
                    let i = idx + 1;
                    let pane_state_dir = state_base.join(&sid).join(format!("pane-{}", i));
                    let exec_shell_tail =
                        matches!(cli.fork_merging_strategy, aifo_coder::MergingStrategy::None);
                    let session = crate::fork::session::make_session(
                        &sid,
                        &session_name,
                        &base_label,
                        &base_ref_or_sha,
                        &base_commit_sha,
                        created_at,
                        &layout,
                        agent,
                        &session_dir,
                    );
                    let container_name = crate::fork::env::pane_container_name(agent, &sid, i);
                    let pane = crate::fork::session::make_pane(
                        i,
                        pane_dir.as_path(),
                        _b,
                        &pane_state_dir,
                        &container_name,
                    );
                    // Touch pane fields to mark them as intentionally used
                    let _ = (
                        pane.index,
                        &pane.dir,
                        &pane.branch,
                        &pane.state_dir,
                        &pane.container_name,
                    );
                    let inner = crate::fork::inner::build_inner_gitbash(
                        &session.agent,
                        &session.sid,
                        i,
                        pane_dir.as_path(),
                        &pane_state_dir,
                        &child_args,
                        exec_shell_tail,
                    );

                    let mut cmd = Command::new(&mt);
                    cmd.arg("-e").arg("bash").arg("-lc").arg(&inner);
                    if cli.verbose {
                        let preview = vec![
                            mt.display().to_string(),
                            "-e".to_string(),
                            "bash".to_string(),
                            "-lc".to_string(),
                            inner.clone(),
                        ];
                        eprintln!("aifo-coder: mintty: {}", aifo_coder::shell_join(&preview));
                    }
                    match cmd.status() {
                        Ok(s) if s.success() => {}
                        _ => {
                            any_failed = true;
                            break;
                        }
                    }
                }

                if any_failed {
                    eprintln!("aifo-coder: failed to launch one or more mintty windows.");
                    crate::fork::cleanup::cleanup_and_update_meta(
                        &repo_root,
                        &sid,
                        &clones,
                        cli.fork_keep_on_failure,
                        &session_dir,
                        snapshot_sha.as_deref(),
                        &layout,
                        false,
                    );
                    return ExitCode::from(1);
                }

                // Apply post-fork merging if requested, then print guidance
                if crate::fork::post_merge::apply_post_merge(
                    &repo_root,
                    &sid,
                    cli.fork_merging_strategy,
                    cli.fork_merging_autoclean,
                    cli.dry_run,
                    cli.verbose,
                    true,
                )
                .is_err()
                {
                    // errors already logged
                }
                println!();
                println!("aifo-coder: fork session {} launched (mintty).", sid);
                print_inspect_merge_guidance(
                    &repo_root,
                    &sid,
                    &base_label,
                    &base_ref_or_sha,
                    &clones,
                    false,
                    false,
                    true,
                );
                return ExitCode::from(0);
            } else {
                // Fallback: launch Windows Terminal even though we cannot wait; print manual-merge advice
                let wt2 = which("wt").or_else(|_| which("wt.exe"));
                if let Ok(wtbin2) = wt2 {
                    if let Err(code) = crate::fork::preflight::guard_no_panes(clones.len()) {
                        return code;
                    }
                    let psbin = which("pwsh")
                        .or_else(|_| which("powershell"))
                        .or_else(|_| which("powershell.exe"))
                        .unwrap_or_else(|_| std::path::PathBuf::from("powershell"));
                    let orient_for_layout = |i: usize| -> &'static str {
                        match layout.as_str() {
                            "even-h" => "-H",
                            "even-v" => "-V",
                            _ => {
                                if i.is_multiple_of(2) {
                                    "-H"
                                } else {
                                    "-V"
                                }
                            }
                        }
                    };

                    // Pane 1
                    {
                        let (pane1_dir, _b) = &clones[0];
                        let pane_state_dir = state_base.join(&sid).join("pane-1");
                        let session = crate::fork::session::make_session(
                            &sid,
                            &session_name,
                            &base_label,
                            &base_ref_or_sha,
                            &base_commit_sha,
                            created_at,
                            &layout,
                            agent,
                            &session_dir,
                        );
                        let container_name = crate::fork::env::pane_container_name(agent, &sid, 1);
                        let pane = crate::fork::session::make_pane(
                            1,
                            pane1_dir.as_path(),
                            _b,
                            &pane_state_dir,
                            &container_name,
                        );
                        // Touch pane fields to mark them as intentionally used
                        let _ = (
                            pane.index,
                            &pane.dir,
                            &pane.branch,
                            &pane.state_dir,
                            &pane.container_name,
                        );
                        let inner = crate::fork::inner::build_inner_powershell(
                            &session.agent,
                            &session.sid,
                            1,
                            pane1_dir.as_path(),
                            &pane_state_dir,
                            &child_args,
                        );
                        let mut cmd = Command::new(&wtbin2);
                        cmd.arg("new-tab")
                            .arg("-d")
                            .arg(pane1_dir)
                            .arg(&psbin)
                            .arg("-NoExit")
                            .arg("-Command")
                            .arg(&inner);
                        if cli.verbose {
                            #[cfg(windows)]
                            {
                                let preview = aifo_coder::wt_build_new_tab_args(
                                    &psbin,
                                    pane1_dir.as_path(),
                                    &inner,
                                );
                                eprintln!(
                                    "aifo-coder: windows-terminal: {}",
                                    aifo_coder::shell_join(&preview)
                                );
                            }
                        }
                        let _ = cmd.status();
                    }

                    // Additional panes
                    let mut split_failed = false;
                    for (idx, (pane_dir, _b)) in clones.iter().enumerate().skip(1) {
                        let i = idx + 1;
                        let pane_state_dir = state_base.join(&sid).join(format!("pane-{}", i));
                        let session = crate::fork::session::make_session(
                            &sid,
                            &session_name,
                            &base_label,
                            &base_ref_or_sha,
                            &base_commit_sha,
                            created_at,
                            &layout,
                            agent,
                            &session_dir,
                        );
                        let container_name = crate::fork::env::pane_container_name(agent, &sid, i);
                        let pane = crate::fork::session::make_pane(
                            i,
                            pane_dir.as_path(),
                            _b,
                            &pane_state_dir,
                            &container_name,
                        );
                        // Touch pane fields to mark them as intentionally used
                        let _ = (
                            pane.index,
                            &pane.dir,
                            &pane.branch,
                            &pane.state_dir,
                            &pane.container_name,
                        );
                        let inner = crate::fork::inner::build_inner_powershell(
                            &session.agent,
                            &session.sid,
                            i,
                            pane_dir.as_path(),
                            &pane_state_dir,
                            &child_args,
                        );
                        let orient = orient_for_layout(i);
                        let mut cmd = Command::new(&wtbin2);
                        cmd.arg("split-pane")
                            .arg(orient)
                            .arg("-d")
                            .arg(pane_dir)
                            .arg(&psbin)
                            .arg("-NoExit")
                            .arg("-Command")
                            .arg(&inner);
                        if cli.verbose {
                            #[cfg(windows)]
                            {
                                let preview = aifo_coder::wt_build_split_args(
                                    orient,
                                    &psbin,
                                    pane_dir.as_path(),
                                    &inner,
                                );
                                eprintln!(
                                    "aifo-coder: windows-terminal: {}",
                                    aifo_coder::shell_join(&preview)
                                );
                            }
                        }
                        match cmd.status() {
                            Ok(s) if s.success() => {}
                            _ => {
                                split_failed = true;
                                break;
                            }
                        }
                    }
                    if split_failed {
                        eprintln!("aifo-coder: warning: one or more Windows Terminal panes failed to open.");
                    }

                    println!();
                    println!(
                        "aifo-coder: fork session {} launched in Windows Terminal.",
                        sid
                    );
                    if !matches!(cli.fork_merging_strategy, aifo_coder::MergingStrategy::None) {
                        let strat = match cli.fork_merging_strategy {
                            aifo_coder::MergingStrategy::Fetch => "fetch",
                            aifo_coder::MergingStrategy::Octopus => "octopus",
                            _ => "none",
                        };
                        {
                            eprintln!(
                                "{}",
                                aifo_coder::paint(
                                    use_err_color,
                                    "\x1b[33m",
                                    &format!(
                                        concat!(
                                            "aifo-coder: note: no waitable orchestrator found; ",
                                            "automatic post-fork merging ({}) is unavailable."
                                        ),
                                        strat
                                    )
                                )
                            );
                            eprintln!(
                                "{}",
                                aifo_coder::paint(
                                    use_err_color,
                                    "\x1b[33m",
                                    &format!(
                                        concat!(
                                            "aifo-coder: after you close all panes, run: ",
                                            "aifo-coder fork merge --session {} ",
                                            "--strategy {}"
                                        ),
                                        sid, strat
                                    )
                                )
                            );
                        }
                    }
                    print_inspect_merge_guidance(
                        &repo_root,
                        &sid,
                        &base_label,
                        &base_ref_or_sha,
                        &clones,
                        false,
                        false,
                        true,
                    );
                    return ExitCode::from(0);
                } else {
                    eprintln!("aifo-coder: error: neither Windows Terminal (wt.exe), PowerShell, nor Git Bash/mintty found in PATH.");
                    return ExitCode::from(1);
                }
            }
        }
        let ps_name = powershell.unwrap(); // used only for reference in logs

        let mut any_failed = false;
        let mut pids: Vec<String> = Vec::new();
        for (idx, (pane_dir, _b)) in clones.iter().enumerate() {
            let i = idx + 1;
            let pane_state_dir = state_base.join(&sid).join(format!("pane-{}", i));
            let session = crate::fork::session::make_session(
                &sid,
                &session_name,
                &base_label,
                &base_ref_or_sha,
                &base_commit_sha,
                created_at,
                &layout,
                agent,
                &session_dir,
            );
            let container_name = crate::fork::env::pane_container_name(agent, &sid, i);
            let pane = crate::fork::session::make_pane(
                i,
                pane_dir.as_path(),
                _b,
                &pane_state_dir,
                &container_name,
            );
            // Touch pane fields to mark them as intentionally used
            let _ = (
                pane.index,
                &pane.dir,
                &pane.branch,
                &pane.state_dir,
                &pane.container_name,
            );
            let inner = crate::fork::inner::build_inner_powershell(
                &session.agent,
                &session.sid,
                i,
                pane_dir.as_path(),
                &pane_state_dir,
                &child_args,
            );

            // Launch a new PowerShell window using Start-Process and capture its PID
            let script = {
                let wd = ps_quote(&pane_dir.display().to_string());
                let child = ps_quote(&ps_name.display().to_string());
                let inner_q = ps_quote(&inner);
                let arglist =
                    if matches!(cli.fork_merging_strategy, aifo_coder::MergingStrategy::None) {
                        "'-NoExit','-Command'".to_string()
                    } else {
                        "'-Command'".to_string()
                    };
                format!("(Start-Process -WindowStyle Normal -WorkingDirectory {wd} {child} -ArgumentList {arglist},{inner_q} -PassThru).Id")
            };
            if cli.verbose {
                eprintln!("aifo-coder: powershell start-script: {}", script);
                eprintln!("aifo-coder: powershell detected at: {}", ps_name.display());
            }
            let out = Command::new(&ps_name)
                .arg("-NoProfile")
                .arg("-Command")
                .arg(&script)
                .output();
            match out {
                Ok(o) if o.status.success() => {
                    let pid = String::from_utf8_lossy(&o.stdout).trim().to_string();
                    if !pid.is_empty() {
                        println!("[{}] started PID={} dir={}", i, pid, pane_dir.display());
                        pids.push(pid.clone());
                    } else {
                        println!("[{}] started dir={} (PID unknown)", i, pane_dir.display());
                    }
                }
                _ => {
                    any_failed = true;
                    break;
                }
            }
        }

        if any_failed {
            eprintln!("aifo-coder: failed to launch one or more PowerShell windows.");
            crate::fork::cleanup::cleanup_and_update_meta(
                &repo_root,
                &sid,
                &clones,
                cli.fork_keep_on_failure,
                &session_dir,
                snapshot_sha.as_deref(),
                &layout,
                false,
            );
            return ExitCode::from(1);
        }

        if !matches!(cli.fork_merging_strategy, aifo_coder::MergingStrategy::None) {
            if !pids.is_empty() {
                let list = pids.join(",");
                let wait_cmd = format!("Wait-Process -Id {}", list);
                if cli.verbose {
                    eprintln!("aifo-coder: powershell wait-script: {}", wait_cmd);
                }
                let _ = Command::new(&ps_name)
                    .arg("-NoProfile")
                    .arg("-Command")
                    .arg(&wait_cmd)
                    .status();
            }
            let _ = crate::fork::post_merge::apply_post_merge(
                &repo_root,
                &sid,
                cli.fork_merging_strategy,
                cli.fork_merging_autoclean,
                cli.dry_run,
                cli.verbose,
                false,
            );
        }

        // Print guidance and return
        println!();
        println!(
            "aifo-coder: fork session {} launched (PowerShell windows).",
            sid
        );
        print_inspect_merge_guidance(
            &repo_root,
            &sid,
            &base_label,
            &base_ref_or_sha,
            &clones,
            false,
            false,
            true,
        );
        return ExitCode::from(0);
    } else {
        // Build and run tmux session
        let tmux = which("tmux").expect("tmux not found");
        if let Err(code) = crate::fork::preflight::guard_no_panes(clones.len()) {
            return code;
        }

        // Prepare launcher and child command for tmux launch script builder
        let launcher = std::env::current_exe()
            .ok()
            .and_then(|p| p.canonicalize().ok())
            .and_then(|p| p.to_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "./aifo-coder".to_string());
        let mut child_cmd_words = vec![launcher.clone()];
        child_cmd_words.extend(child_args.clone());
        let child_joined = aifo_coder::shell_join(&child_cmd_words);
        // Build a session descriptor for tmux helper builders
        let session = crate::fork::session::make_session(
            &sid,
            &session_name,
            &base_label,
            &base_ref_or_sha,
            &base_commit_sha,
            created_at,
            &layout,
            agent,
            &session_dir,
        );
        // Touch fields so clippy sees them as read on this target too
        let _ = (
            &session.base_label,
            &session.base_ref_or_sha,
            &session.base_commit_sha,
            session.created_at,
            &session.layout,
            &session.session_name,
            &session.session_dir,
            &session.agent,
        );

        // Pane 1
        {
            let (pane1_dir, _b) = &clones[0];
            let mut cmd = Command::new(&tmux);
            cmd.arg("new-session")
                .arg("-d")
                .arg("-s")
                .arg(&session_name)
                .arg("-n")
                .arg("aifo-fork")
                .arg("-c")
                .arg(pane1_dir);
            if cli.verbose {
                let preview_new = vec![
                    "tmux".to_string(),
                    "new-session".to_string(),
                    "-d".to_string(),
                    "-s".to_string(),
                    session_name.clone(),
                    "-n".to_string(),
                    "aifo-fork".to_string(),
                    "-c".to_string(),
                    pane1_dir.display().to_string(),
                ];
                eprintln!("aifo-coder: tmux: {}", aifo_coder::shell_join(&preview_new));
            }
            let st = match cmd.status() {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("aifo-coder: tmux new-session failed to start: {}", e);
                    // Failure policy: keep clones by default; optionally remove if user disabled keep-on-failure
                    if !cli.fork_keep_on_failure {
                        for (dir, _) in &clones {
                            let _ = fs::remove_dir_all(dir);
                        }
                        println!(
                            "Removed all created pane directories under {}.",
                            session_dir.display()
                        );
                    } else {
                        println!(
                            "One or more clones were created under {}.",
                            session_dir.display()
                        );
                        println!("You can inspect them manually. Example:");
                        if let Some((first_dir, first_branch)) = clones.first() {
                            println!("  git -C \"{}\" status", first_dir.display());
                            println!(
                                "  git -C \"{}\" log --oneline --decorate -n 20",
                                first_dir.display()
                            );
                            println!(
                                "  git -C \"{}\" remote add fork-{}-1 \"{}\"",
                                repo_root.display(),
                                sid,
                                first_dir.display()
                            );
                            println!(
                                "  git -C \"{}\" fetch fork-{}-1 {}",
                                repo_root.display(),
                                sid,
                                first_branch
                            );
                        }
                    }
                    // Update metadata with panes_created and existing pane dirs
                    let existing: Vec<(PathBuf, String)> = clones
                        .iter()
                        .filter(|(p, _)| p.exists())
                        .map(|(p, b)| (p.clone(), b.clone()))
                        .collect();
                    let _ = crate::fork::meta::update_panes_created(
                        &repo_root,
                        &sid,
                        existing.len(),
                        &existing,
                        snapshot_sha.as_deref(),
                        &layout,
                    );
                    return ExitCode::from(1);
                }
            };
            if !st.success() {
                eprintln!("aifo-coder: tmux new-session failed.");
                // Best-effort: kill any stray session
                let mut kill = Command::new(&tmux);
                let _ = kill
                    .arg("kill-session")
                    .arg("-t")
                    .arg(&session_name)
                    .status();
                crate::fork::cleanup::cleanup_and_update_meta(
                    &repo_root,
                    &sid,
                    &clones,
                    cli.fork_keep_on_failure,
                    &session_dir,
                    snapshot_sha.as_deref(),
                    &layout,
                    false,
                );
                return ExitCode::from(1);
            }
        }

        // Panes 2..N
        let mut split_failed = false;
        for (idx, (pane_dir, _b)) in clones.iter().enumerate().skip(1) {
            let i = idx + 1;
            let _pane_state_dir = state_base.join(&sid).join(format!("pane-{}", i));
            let mut cmd = Command::new(&tmux);
            cmd.arg("split-window")
                .arg("-t")
                .arg(format!("{}:0", &session_name))
                .arg("-c")
                .arg(pane_dir);
            if cli.verbose {
                let target = format!("{}:0", &session_name);
                let preview_split = vec![
                    "tmux".to_string(),
                    "split-window".to_string(),
                    "-t".to_string(),
                    target,
                    "-c".to_string(),
                    pane_dir.display().to_string(),
                ];
                eprintln!(
                    "aifo-coder: tmux: {}",
                    aifo_coder::shell_join(&preview_split)
                );
            }
            let st = cmd.status();
            match st {
                Ok(s) if s.success() => {}
                Ok(_) | Err(_) => {
                    split_failed = true;
                    break;
                }
            }
        }
        if split_failed {
            eprintln!("aifo-coder: tmux split-window failed for one or more panes.");
            // Best-effort: kill the tmux session to avoid leaving a half-configured window
            let mut kill = Command::new(&tmux);
            let _ = kill
                .arg("kill-session")
                .arg("-t")
                .arg(&session_name)
                .status();

            crate::fork::cleanup::cleanup_and_update_meta(
                &repo_root,
                &sid,
                &clones,
                cli.fork_keep_on_failure,
                &session_dir,
                snapshot_sha.as_deref(),
                &layout,
                true,
            );
            return ExitCode::from(1);
        }

        // Layout and options
        let mut lay = Command::new(&tmux);
        lay.arg("select-layout")
            .arg("-t")
            .arg(format!("{}:0", &session_name))
            .arg(&layout_effective);
        if cli.verbose {
            let preview_layout = vec![
                "tmux".to_string(),
                "select-layout".to_string(),
                "-t".to_string(),
                format!("{}:0", &session_name),
                layout_effective.clone(),
            ];
            eprintln!(
                "aifo-coder: tmux: {}",
                aifo_coder::shell_join(&preview_layout)
            );
        }
        let _ = lay.status();

        let mut sync = Command::new(&tmux);
        sync.arg("set-window-option")
            .arg("-t")
            .arg(format!("{}:0", &session_name))
            .arg("synchronize-panes")
            .arg("off");
        if cli.verbose {
            let preview_sync = vec![
                "tmux".to_string(),
                "set-window-option".to_string(),
                "-t".to_string(),
                format!("{}:0", &session_name),
                "synchronize-panes".to_string(),
                "off".to_string(),
            ];
            eprintln!(
                "aifo-coder: tmux: {}",
                aifo_coder::shell_join(&preview_sync)
            );
        }
        let _ = sync.status();

        // Start commands in each pane via tmux send-keys now that the layout is ready
        for (idx, (_pane_dir, _b)) in clones.iter().enumerate() {
            let i = idx + 1;
            let pane_state_dir = state_base.join(&sid).join(format!("pane-{}", i));
            let container_name = crate::fork::env::pane_container_name(agent, &sid, i);
            let pane = crate::fork::session::make_pane(
                i,
                &clones[idx].0,
                &clones[idx].1,
                &pane_state_dir,
                &container_name,
            );
            // Touch fields so clippy sees them as read on this target too
            let _ = (
                pane.index,
                &pane.dir,
                &pane.branch,
                &pane.state_dir,
                &pane.container_name,
            );
            let inner = crate::fork::inner::build_tmux_launch_script(
                &sid,
                i,
                &container_name,
                &pane_state_dir,
                &child_joined,
                &launcher,
            );
            let script_path = pane_state_dir.join("launch.sh");
            let _ = fs::create_dir_all(&pane_state_dir);
            let _ = fs::write(&script_path, inner.as_bytes());
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(&script_path, fs::Permissions::from_mode(0o700));
            }
            let target = format!("{}:0.{}", &session_name, idx);
            let shwrap = format!(
                "sh -lc {}",
                aifo_coder::shell_escape(&script_path.display().to_string())
            );
            let mut sk = Command::new(&tmux);
            sk.arg("send-keys")
                .arg("-t")
                .arg(&target)
                .arg(&shwrap)
                .arg("C-m");
            if cli.verbose {
                let preview = vec![
                    "tmux".to_string(),
                    "send-keys".to_string(),
                    "-t".to_string(),
                    target.clone(),
                    shwrap.clone(),
                    "C-m".to_string(),
                ];
                eprintln!("aifo-coder: tmux: {}", aifo_coder::shell_join(&preview));
            }
            let _ = sk.status();
        }

        // Attach or switch
        let attach_cmd = if env::var("TMUX").ok().filter(|s| !s.is_empty()).is_some() {
            vec![
                "switch-client".to_string(),
                "-t".to_string(),
                session_name.clone(),
            ]
        } else {
            vec![
                "attach-session".to_string(),
                "-t".to_string(),
                session_name.clone(),
            ]
        };
        let mut att = Command::new(&tmux);
        for a in &attach_cmd {
            att.arg(a);
        }
        let _ = att.status();

        // After tmux session ends or switch completes, print merging guidance
        println!();
        if use_color_out {
            println!(
                "\x1b[36;1maifo-coder:\x1b[0m fork session \x1b[32;1m{}\x1b[0m completed.",
                sid
            );
        } else {
            println!("aifo-coder: fork session {} completed.", sid);
        }
        println!();
        print_inspect_merge_guidance(
            &repo_root,
            &sid,
            &base_label,
            &base_ref_or_sha,
            &clones,
            use_color_out,
            false,
            true,
        );

        {
            if !matches!(cli.fork_merging_strategy, aifo_coder::MergingStrategy::None) {
                let strat = match cli.fork_merging_strategy {
                    aifo_coder::MergingStrategy::None => "none",
                    aifo_coder::MergingStrategy::Fetch => "fetch",
                    aifo_coder::MergingStrategy::Octopus => "octopus",
                };
                // visual separation from the guidance block above
                println!();
                {
                    eprintln!(
                        "{}",
                        aifo_coder::paint(
                            use_err_color,
                            "\x1b[36;1m",
                            &format!("aifo-coder: applying post-fork merge strategy: {}", strat)
                        )
                    );
                }
                let _ = crate::fork::post_merge::apply_post_merge(
                    &repo_root,
                    &sid,
                    cli.fork_merging_strategy,
                    cli.fork_merging_autoclean,
                    cli.dry_run,
                    cli.verbose,
                    false,
                );
            }
            ExitCode::from(0)
        }
    }
}
