#![allow(clippy::module_name_repetitions)]
//! Fork launcher and runner decomposition.
//!
//! Overview
//! - Coordinates fork session lifecycle: preflight checks, base detection, optional snapshot,
//!   cloning, metadata writing, orchestrator selection/launch, post-merge application, and guidance.
//! - Platforms: tmux on Unix; Windows Terminal (non-waitable), PowerShell (waitable), Git Bash/mintty.
//!
//! Design
//! - Delegates pane launch to orchestrators selected by crate::fork::orchestrators::select_orchestrator.
//! - Preserves all user-visible strings and behavior; color usage and guidance text remain verbatim.
//! - Binary-side glue leverages public aifo_coder::* helpers; internal helpers live under fork_impl/*.
//!
//! The module keeps the external CLI stable and focuses on maintainability and clarity for contributors.

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use crate::cli::{Agent, Cli};
use crate::fork::orchestrators::Orchestrator;
use crate::fork_args::fork_build_child_args;
use crate::guidance::print_inspect_merge_guidance;

// Orchestrate tmux-based fork session (Linux/macOS/WSL) — moved from main.rs (Phase 1)
#[allow(clippy::needless_return, unreachable_code, unused_assignments)]
pub fn fork_run(cli: &Cli, panes: usize) -> ExitCode {
    // Pre-compute stderr color usage once per run
    let use_err_color = aifo_coder::color_enabled_stderr();
    let _ = use_err_color;
    // Preflight
    if let Err(code) = crate::fork::preflight::ensure_git_and_orchestrator_present_on_platform() {
        return code;
    }
    let repo_root = match aifo_coder::repo_root() {
        Some(p) => p,
        None => {
            aifo_coder::log_error_stderr(
                use_err_color,
                "aifo-coder: error: fork mode must be run inside a Git repository.",
            );
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
                aifo_coder::log_error_stderr(
                    use_err_color,
                    &format!("aifo-coder: error determining base: {}", e),
                );
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
            aifo_coder::log_error_stderr(
                use_err_color,
                &format!("aifo-coder: error during cloning: {}", e),
            );
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
        aifo_coder::log_info_stderr(
            use_err_color,
            &format!(
                "aifo-coder: tmux layout requested: {} -> effective: {}",
                layout, layout_effective
            ),
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

    // Phase 2: delegate orchestration to platform-specific orchestrators
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
    let mut panes_vec: Vec<crate::fork::types::Pane> = Vec::new();
    for (idx, (pane_dir, branch)) in clones.iter().enumerate() {
        let i = idx + 1;
        let pane_state_dir = crate::fork::env::pane_state_dir(&state_base, &sid, i);
        let container_name = crate::fork::env::pane_container_name(agent, &sid, i);
        panes_vec.push(crate::fork::session::make_pane(
            i,
            pane_dir.as_path(),
            branch,
            &pane_state_dir,
            &container_name,
        ));
    }
    let selected = crate::fork::orchestrators::select_orchestrator(cli, &layout);
    let merge_requested = !matches!(cli.fork_merging_strategy, aifo_coder::MergingStrategy::None);

    #[allow(unused_mut)]
    let mut launched_in: &'static str = if cfg!(target_os = "windows") {
        "Windows Terminal"
    } else {
        "tmux"
    };

    #[cfg(not(windows))]
    {
        match selected {
            crate::fork::orchestrators::Selected::Tmux { .. } => {
                let orch = crate::fork::orchestrators::tmux::Tmux;
                if let Err(e) = orch.launch(&session, &panes_vec, &child_args) {
                    aifo_coder::log_error_stderr(
                        use_err_color,
                        &format!("aifo-coder: {}", e),
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
                launched_in = "tmux";
                let _ = orch.supports_post_merge();
            }
        }
    }

    #[cfg(windows)]
    {
        match selected {
            crate::fork::orchestrators::Selected::WindowsTerminal { .. } => {
                let orch = crate::fork::orchestrators::windows_terminal::WindowsTerminal;
                if let Err(e) = orch.launch(&session, &panes_vec, &child_args) {
                    aifo_coder::log_error_stderr(
                        use_err_color,
                        &format!("aifo-coder: {}", e),
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
                launched_in = "Windows Terminal";
                let _ = orch.supports_post_merge();
            }
            crate::fork::orchestrators::Selected::PowerShell { .. } => {
                let orch = crate::fork::orchestrators::powershell::PowerShell {
                    wait: merge_requested,
                };
                if let Err(e) = orch.launch(&session, &panes_vec, &child_args) {
                    aifo_coder::log_error_stderr(
                        use_err_color,
                        &format!("aifo-coder: {}", e),
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
                launched_in = "PowerShell windows";
                let _ = orch.supports_post_merge();
            }
            crate::fork::orchestrators::Selected::GitBashMintty { .. } => {
                let orch = crate::fork::orchestrators::gitbash_mintty::GitBashMintty {
                    exec_shell_tail: !merge_requested,
                };
                if let Err(e) = orch.launch(&session, &panes_vec, &child_args) {
                    aifo_coder::log_error_stderr(
                        use_err_color,
                        &format!("aifo-coder: {}", e),
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
                launched_in = "Git Bash";
                let _ = orch.supports_post_merge();
            }
        }
    }

    // Apply post-merge or print fallback guidance when non-waitable
    if merge_requested {
        #[cfg(not(windows))]
        {
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
        #[cfg(windows)]
        {
            match selected {
                crate::fork::orchestrators::Selected::PowerShell { .. } => {
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
                crate::fork::orchestrators::Selected::WindowsTerminal { .. }
                | crate::fork::orchestrators::Selected::GitBashMintty { .. } => {
                    let use_err = aifo_coder::color_enabled_stderr();
                    aifo_coder::log_warn_stderr(
                        use_err,
                        &format!(
                            concat!(
                                "aifo-coder: note: no waitable orchestrator found; ",
                                "automatic post-fork merging ({}) is unavailable."
                            ),
                            match cli.fork_merging_strategy {
                                aifo_coder::MergingStrategy::Fetch => "fetch",
                                aifo_coder::MergingStrategy::Octopus => "octopus",
                                _ => "none",
                            }
                        ),
                    );
                    aifo_coder::log_warn_stderr(
                        use_err,
                        &format!(
                            concat!(
                                "aifo-coder: after you close all panes, run: ",
                                "aifo-coder fork merge --session {} --strategy {}"
                            ),
                            sid,
                            match cli.fork_merging_strategy {
                                aifo_coder::MergingStrategy::Fetch => "fetch",
                                aifo_coder::MergingStrategy::Octopus => "octopus",
                                _ => "none",
                            }
                        ),
                    );
                }
            }
        }
    }

    println!();
    match launched_in {
        "tmux" => {
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
        }
        other => {
            println!("aifo-coder: fork session {} launched ({}).", sid, other);
            #[cfg(windows)]
            let include_remote_examples = matches!(
                selected,
                crate::fork::orchestrators::Selected::GitBashMintty { .. }
            );
            #[cfg(not(windows))]
            let include_remote_examples = false;
            print_inspect_merge_guidance(
                &repo_root,
                &sid,
                &base_label,
                &base_ref_or_sha,
                &clones,
                false,
                include_remote_examples,
                true,
            );
        }
    }
    return ExitCode::from(0);
}
