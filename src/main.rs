#![allow(clippy::needless_return)]
use aifo_coder::{acquire_lock, build_docker_cmd, desired_apparmor_profile};
use clap::Parser;
use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::{Command, ExitCode};
use which::which;
mod agent_images;
mod banner;
mod cli;
mod commands;
mod doctor;
mod fork_args;
mod guidance;
mod toolchain_session;
mod warnings;
mod fork {
    pub mod env;
    pub mod inner;
    pub mod meta;
    pub mod orchestrators;
    pub mod post_merge;
    pub mod types;
}
use crate::agent_images::default_image_for;
use crate::banner::print_startup_banner;
use crate::cli::{Agent, Cli, Flavor, ForkCmd};
use crate::fork_args::fork_build_child_args;
use crate::guidance::print_inspect_merge_guidance;
use crate::warnings::{
    maybe_warn_missing_toolchain_agent, maybe_warn_missing_toolchain_for_fork,
    warn_if_tmp_workspace,
};

// Orchestrate tmux-based fork session (Linux/macOS/WSL)
fn fork_run(cli: &Cli, panes: usize) -> ExitCode {
    // Preflight
    if which("git").is_err() {
        eprintln!("aifo-coder: error: git is required and was not found in PATH.");
        return ExitCode::from(1);
    }
    if cfg!(target_os = "windows") {
        // Windows preflight: require at least one orchestrator (wt.exe, PowerShell, or Git Bash)
        let wt_ok = which("wt").or_else(|_| which("wt.exe")).is_ok();
        let ps_ok = which("pwsh")
            .or_else(|_| which("powershell"))
            .or_else(|_| which("powershell.exe"))
            .is_ok();
        let gb_ok = which("git-bash.exe")
            .or_else(|_| which("bash.exe"))
            .or_else(|_| which("mintty.exe"))
            .is_ok();
        if !(wt_ok || ps_ok || gb_ok) {
            eprintln!("aifo-coder: error: none of Windows Terminal (wt.exe), PowerShell, or Git Bash were found in PATH.");
            return ExitCode::from(127);
        }
    } else if which("tmux").is_err() {
        eprintln!("aifo-coder: error: tmux not found. Please install tmux to use fork mode.");
        return ExitCode::from(127);
    }
    let repo_root = match aifo_coder::repo_root() {
        Some(p) => p,
        None => {
            eprintln!("aifo-coder: error: fork mode must be run inside a Git repository.");
            return ExitCode::from(1);
        }
    };
    if panes > 8 {
        let msg = format!(
            "Launching {} panes may impact disk/memory and I/O performance.",
            panes
        );
        if !aifo_coder::warn_prompt_continue_or_quit(&[&msg]) {
            return ExitCode::from(1);
        }
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
        if let Ok(out) = Command::new("git")
            .arg("-C")
            .arg(&repo_root)
            .arg("status")
            .arg("--porcelain=v1")
            .arg("-uall")
            .output()
        {
            if !out.stdout.is_empty()
                && !aifo_coder::warn_prompt_continue_or_quit(&[
                    "working tree has uncommitted changes; they will not be included in the fork panes.",
                    "re-run with --fork-include-dirty to include them.",
                ])
            {
                return ExitCode::from(1);
            }
        }
    }

    // Preflight: if octopus merging requested, ensure original repo is clean to avoid hidden merge failures
    if matches!(
        cli.fork_merging_strategy,
        aifo_coder::MergingStrategy::Octopus
    ) {
        if let Ok(o) = Command::new("git")
            .arg("-C")
            .arg(&repo_root)
            .arg("status")
            .arg("--porcelain=v1")
            .arg("-uall")
            .output()
        {
            if !o.stdout.is_empty()
                && !aifo_coder::warn_prompt_continue_or_quit(&[
                    "octopus merge requires a clean working tree in the original repository.",
                    "commit or stash your changes before proceeding, or merging will likely fail.",
                ])
            {
                return ExitCode::from(1);
            }
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
        if !maybe_warn_missing_toolchain_for_fork(cli, agent_for_warn) {
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
    if use_color_out {
        println!(
            "\x1b[36;1maifo-coder:\x1b[0m fork session \x1b[32;1m{}\x1b[0m on base \x1b[34;1m{}\x1b[0m (\x1b[34m{}\x1b[0m)",
            sid, base_label, base_ref_or_sha
        );
    } else {
        println!(
            "aifo-coder: fork session {} on base {} ({})",
            sid, base_label, base_ref_or_sha
        );
    }
    println!();
    if use_color_out {
        println!(
            "created \x1b[36;1m{}\x1b[0m clones under \x1b[34;1m{}\x1b[0m",
            panes,
            session_dir.display()
        );
    } else {
        println!("created {} clones under {}", panes, session_dir.display());
    }
    if let Some(ref snap) = snapshot_sha {
        if use_color_out {
            println!(
                "\x1b[32mincluded dirty working tree via snapshot {}\x1b[0m",
                snap
            );
        } else {
            println!("included dirty working tree via snapshot {}", snap);
        }
    } else if cli.fork_include_dirty {
        if use_color_out {
            println!("\x1b[33mwarning:\x1b[0m requested --fork-include-dirty, but snapshot failed; dirty changes not included.");
        } else {
            println!("warning: requested --fork-include-dirty, but snapshot failed; dirty changes not included.");
        }
    }
    if !dissoc {
        if use_color_out {
            println!("\x1b[90mnote: clones reference the base repo’s object store; avoid pruning base objects until done.\x1b[0m");
        } else {
            println!("note: clones reference the base repo’s object store; avoid pruning base objects until done.");
        }
    }
    println!();

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
        let out = Command::new("git")
            .arg("-C")
            .arg(&repo_root)
            .arg("rev-parse")
            .arg("--verify")
            .arg(&base_ref_or_sha)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output()
            .ok();
        out.and_then(|o| {
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
    for (idx, (pane_dir, branch)) in clones.iter().enumerate() {
        let i = idx + 1;
        let cname = crate::fork::env::pane_container_name(agent, &sid, i);
        let state_dir = crate::fork::env::pane_state_dir(&state_base, &sid, i);
        let _ = fs::create_dir_all(state_dir.join(".aider"));
        let _ = fs::create_dir_all(state_dir.join(".codex"));
        let _ = fs::create_dir_all(state_dir.join(".crush"));
        if use_color_out {
            println!(
                "[\x1b[36;1m{}\x1b[0m] folder=\x1b[34m{}\x1b[0m",
                i,
                pane_dir.display()
            );
            println!("    branch=\x1b[32m{}\x1b[0m", branch);
            println!("    state=\x1b[90m{}\x1b[0m", state_dir.display());
            println!("    container=\x1b[35m{}\x1b[0m", cname);
            println!();
        } else {
            println!("[{}] folder={}", i, pane_dir.display());
            println!("    branch={}", branch);
            println!("    state={}", state_dir.display());
            println!("    container={}", cname);
            println!();
        }
    }

    // Orchestrate panes (Windows uses Windows Terminal or PowerShell; Unix-like uses tmux)
    if cfg!(target_os = "windows") {
        // Helper to PowerShell-quote a single token
        let ps_quote = |s: &str| -> String {
            let esc = s.replace('\'', "''");
            format!("'{}'", esc)
        };
        // Build inner PowerShell command string setting env per pane, then invoking aifo-coder with args
        let build_ps_inner =
            |i: usize, pane_dir: &std::path::Path, pane_state_dir: &PathBuf| -> String {
                let cname = format!("aifo-coder-{}-{}-{}", agent, sid, i);
                let kv = [
                    ("AIFO_CODER_SKIP_LOCK", "1".to_string()),
                    ("AIFO_CODER_CONTAINER_NAME", cname.clone()),
                    ("AIFO_CODER_HOSTNAME", cname),
                    ("AIFO_CODER_FORK_SESSION", sid.clone()),
                    ("AIFO_CODER_FORK_INDEX", i.to_string()),
                    (
                        "AIFO_CODER_FORK_STATE_DIR",
                        pane_state_dir.display().to_string(),
                    ),
                    ("AIFO_CODER_SUPPRESS_TOOLCHAIN_WARNING", "1".to_string()),
                ];
                let mut assigns: Vec<String> = Vec::new();
                for (k, v) in kv {
                    assigns.push(format!("$env:{}={}", k, ps_quote(&v)));
                }
                let mut words: Vec<String> = vec!["aifo-coder".to_string()];
                words.extend(child_args.clone());
                let cmd = words
                    .iter()
                    .map(|w| ps_quote(w))
                    .collect::<Vec<_>>()
                    .join(" ");
                let setloc = format!("Set-Location {}", ps_quote(&pane_dir.display().to_string()));
                format!("{}; {}; {}", setloc, assigns.join("; "), cmd)
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
                    let session = crate::fork::types::ForkSession {
                        sid: sid.clone(),
                        session_name: session_name.clone(),
                        base_label: base_label.clone(),
                        base_ref_or_sha: base_ref_or_sha.clone(),
                        base_commit_sha: base_commit_sha.clone(),
                        created_at,
                        layout: layout.clone(),
                        agent: agent.to_string(),
                        session_dir: session_dir.clone(),
                    };
                    let container_name = crate::fork::env::pane_container_name(agent, &sid, i);
                    let pane = crate::fork::types::Pane {
                        index: i,
                        dir: pane_dir.clone(),
                        branch: _b.clone(),
                        state_dir: pane_state_dir.clone(),
                        container_name,
                    };
                    let exec_shell_tail =
                        matches!(cli.fork_merging_strategy, aifo_coder::MergingStrategy::None);
                    let inner = crate::fork::inner::build_inner_gitbash(
                        &session,
                        &pane,
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
                            "Clones remain under {} for recovery.",
                            session_dir.display()
                        );
                    }
                    // Update metadata with panes_created
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

                // Apply post-fork merging if requested, then print guidance
                if !matches!(cli.fork_merging_strategy, aifo_coder::MergingStrategy::None) {
                    let strat = match cli.fork_merging_strategy {
                        aifo_coder::MergingStrategy::None => "none",
                        aifo_coder::MergingStrategy::Fetch => "fetch",
                        aifo_coder::MergingStrategy::Octopus => "octopus",
                    };
                    {
                        let use_err = aifo_coder::color_enabled_stderr();
                        eprintln!(
                            "{}",
                            aifo_coder::paint(
                                use_err,
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
                                {
                                    let use_err = aifo_coder::color_enabled_stderr();
                                    eprintln!(
                                        "{}",
                                        aifo_coder::paint(
                                            use_err,
                                            "\x1b[36;1m",
                                            &format!(
                                                "aifo-coder: octopus merge succeeded; disposing fork session {} ...",
                                                sid
                                            )
                                        )
                                    );
                                }
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
                                        let use_err = aifo_coder::color_enabled_stderr();
                                        eprintln!(
                                            "{}",
                                            aifo_coder::paint(
                                                use_err,
                                                "\x1b[32;1m",
                                                &format!(
                                                    "aifo-coder: disposed fork session {}.",
                                                    sid
                                                )
                                            )
                                        );
                                    }
                                    Err(e) => {
                                        let use_err = aifo_coder::color_enabled_stderr();
                                        eprintln!(
                                            "{}",
                                            aifo_coder::paint(
                                                use_err,
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
                            let use_err = aifo_coder::color_enabled_stderr();
                            eprintln!(
                                "{}",
                                aifo_coder::paint(
                                    use_err,
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
                    let session = crate::fork::types::ForkSession {
                        sid: sid.clone(),
                        session_name: session_name.clone(),
                        base_label: base_label.clone(),
                        base_ref_or_sha: base_ref_or_sha.clone(),
                        base_commit_sha: base_commit_sha.clone(),
                        created_at,
                        layout: layout.clone(),
                        agent: agent.to_string(),
                        session_dir: session_dir.clone(),
                    };
                    let container_name = crate::fork::env::pane_container_name(agent, &sid, i);
                    let pane = crate::fork::types::Pane {
                        index: i,
                        dir: pane_dir.clone(),
                        branch: _b.clone(),
                        state_dir: pane_state_dir.clone(),
                        container_name,
                    };
                    let exec_shell_tail =
                        matches!(cli.fork_merging_strategy, aifo_coder::MergingStrategy::None);
                    let inner = crate::fork::inner::build_inner_gitbash(
                        &session,
                        &pane,
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
                            "Clones remain under {} for recovery.",
                            session_dir.display()
                        );
                    }
                    // Update metadata with panes_created
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

                // Apply post-fork merging if requested, then print guidance
                if !matches!(cli.fork_merging_strategy, aifo_coder::MergingStrategy::None) {
                    let strat = match cli.fork_merging_strategy {
                        aifo_coder::MergingStrategy::None => "none",
                        aifo_coder::MergingStrategy::Fetch => "fetch",
                        aifo_coder::MergingStrategy::Octopus => "octopus",
                    };
                    {
                        let use_err = aifo_coder::color_enabled_stderr();
                        eprintln!(
                            "{}",
                            aifo_coder::paint(
                                use_err,
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
                                {
                                    let use_err = aifo_coder::color_enabled_stderr();
                                    eprintln!(
                                        "{}",
                                        aifo_coder::paint(
                                            use_err,
                                            "\x1b[36;1m",
                                            &format!(
                                                "aifo-coder: octopus merge succeeded; disposing fork session {} ...",
                                                sid
                                            )
                                        )
                                    );
                                }
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
                                        let use_err = aifo_coder::color_enabled_stderr();
                                        eprintln!(
                                            "{}",
                                            aifo_coder::paint(
                                                use_err,
                                                "\x1b[32;1m",
                                                &format!(
                                                    "aifo-coder: disposed fork session {}.",
                                                    sid
                                                )
                                            )
                                        );
                                    }
                                    Err(e) => {
                                        let use_err = aifo_coder::color_enabled_stderr();
                                        eprintln!(
                                            "{}",
                                            aifo_coder::paint(
                                                use_err,
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
                            let use_err = aifo_coder::color_enabled_stderr();
                            eprintln!(
                                "{}",
                                aifo_coder::paint(
                                    use_err,
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
                    let use_err = aifo_coder::color_enabled_stderr();
                    eprintln!(
                        "{}",
                        aifo_coder::paint(
                            use_err,
                            "\x1b[33m",
                            "aifo-coder: using PowerShell windows to enable post-fork merging (--fork-merge-strategy)."
                        )
                    );
                }
            } else {
                if clones.is_empty() {
                    eprintln!("aifo-coder: no panes to create.");
                    return ExitCode::from(1);
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
                            if i % 2 == 0 {
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
                    let session = crate::fork::types::ForkSession {
                        sid: sid.clone(),
                        session_name: session_name.clone(),
                        base_label: base_label.clone(),
                        base_ref_or_sha: base_ref_or_sha.clone(),
                        base_commit_sha: base_commit_sha.clone(),
                        created_at,
                        layout: layout.clone(),
                        agent: agent.to_string(),
                        session_dir: session_dir.clone(),
                    };
                    let container_name = crate::fork::env::pane_container_name(agent, &sid, 1);
                    let pane = crate::fork::types::Pane {
                        index: 1,
                        dir: pane1_dir.clone(),
                        branch: _b.clone(),
                        state_dir: pane_state_dir.clone(),
                        container_name,
                    };
                    let inner =
                        crate::fork::inner::build_inner_powershell(&session, &pane, &child_args);
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
                                    "Clones remain under {} for recovery.",
                                    session_dir.display()
                                );
                            }
                            // Update metadata with panes_created
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
                        Err(e) => {
                            eprintln!(
                                "aifo-coder: Windows Terminal failed to start first pane: {}",
                                e
                            );
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
                                    "Clones remain under {} for recovery.",
                                    session_dir.display()
                                );
                            }
                            // Update metadata with panes_created
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
                    }
                }

                // Additional panes: split-pane
                let mut split_failed = false;
                for (idx, (pane_dir, _b)) in clones.iter().enumerate().skip(1) {
                    let i = idx + 1;
                    let pane_state_dir = state_base.join(&sid).join(format!("pane-{}", i));
                    let session = crate::fork::types::ForkSession {
                        sid: sid.clone(),
                        session_name: session_name.clone(),
                        base_label: base_label.clone(),
                        base_ref_or_sha: base_ref_or_sha.clone(),
                        base_commit_sha: base_commit_sha.clone(),
                        created_at,
                        layout: layout.clone(),
                        agent: agent.to_string(),
                        session_dir: session_dir.clone(),
                    };
                    let container_name = crate::fork::env::pane_container_name(agent, &sid, i);
                    let pane = crate::fork::types::Pane {
                        index: i,
                        dir: pane_dir.clone(),
                        branch: _b.clone(),
                        state_dir: pane_state_dir.clone(),
                        container_name,
                    };
                    let inner =
                        crate::fork::inner::build_inner_powershell(&session, &pane, &child_args);
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
                            "Clones remain under {} for recovery.",
                            session_dir.display()
                        );
                        if let Some((first_dir, first_branch)) = clones.first() {
                            println!("Example recovery:");
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
                    // Update metadata
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
                    let session = crate::fork::types::ForkSession {
                        sid: sid.clone(),
                        session_name: session_name.clone(),
                        base_label: base_label.clone(),
                        base_ref_or_sha: base_ref_or_sha.clone(),
                        base_commit_sha: base_commit_sha.clone(),
                        created_at,
                        layout: layout.clone(),
                        agent: agent.to_string(),
                        session_dir: session_dir.clone(),
                    };
                    let container_name = crate::fork::env::pane_container_name(agent, &sid, i);
                    let pane = crate::fork::types::Pane {
                        index: i,
                        dir: pane_dir.clone(),
                        branch: _b.clone(),
                        state_dir: pane_state_dir.clone(),
                        container_name,
                    };
                    let inner = crate::fork::inner::build_inner_gitbash(
                        &session,
                        &pane,
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
                            "Clones remain under {} for recovery.",
                            session_dir.display()
                        );
                    }
                    // Update metadata with panes_created
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

                // Apply post-fork merging if requested, then print guidance
                if !matches!(cli.fork_merging_strategy, aifo_coder::MergingStrategy::None) {
                    let strat = match cli.fork_merging_strategy {
                        aifo_coder::MergingStrategy::None => "none",
                        aifo_coder::MergingStrategy::Fetch => "fetch",
                        aifo_coder::MergingStrategy::Octopus => "octopus",
                    };
                    {
                        let use_err = aifo_coder::color_enabled_stderr();
                        eprintln!(
                            "{}",
                            aifo_coder::paint(
                                use_err,
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
                                        let use_err = aifo_coder::color_enabled_stderr();
                                        eprintln!(
                                            "{}",
                                            aifo_coder::paint(
                                                use_err,
                                                "\x1b[32;1m",
                                                &format!(
                                                    "aifo-coder: disposed fork session {}.",
                                                    sid
                                                )
                                            )
                                        );
                                    }
                                    Err(e) => {
                                        let use_err = aifo_coder::color_enabled_stderr();
                                        eprintln!(
                                            "{}",
                                            aifo_coder::paint(
                                                use_err,
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
                            let use_err = aifo_coder::color_enabled_stderr();
                            eprintln!(
                                "{}",
                                aifo_coder::paint(
                                    use_err,
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
                    let session = crate::fork::types::ForkSession {
                        sid: sid.clone(),
                        session_name: session_name.clone(),
                        base_label: base_label.clone(),
                        base_ref_or_sha: base_ref_or_sha.clone(),
                        base_commit_sha: base_commit_sha.clone(),
                        created_at,
                        layout: layout.clone(),
                        agent: agent.to_string(),
                        session_dir: session_dir.clone(),
                    };
                    let container_name = crate::fork::env::pane_container_name(agent, &sid, i);
                    let pane = crate::fork::types::Pane {
                        index: i,
                        dir: pane_dir.clone(),
                        branch: _b.clone(),
                        state_dir: pane_state_dir.clone(),
                        container_name,
                    };
                    let inner = crate::fork::inner::build_inner_gitbash(
                        &session,
                        &pane,
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
                            "Clones remain under {} for recovery.",
                            session_dir.display()
                        );
                    }
                    // Update metadata with panes_created
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

                // Apply post-fork merging if requested, then print guidance
                if !matches!(cli.fork_merging_strategy, aifo_coder::MergingStrategy::None) {
                    let strat = match cli.fork_merging_strategy {
                        aifo_coder::MergingStrategy::None => "none",
                        aifo_coder::MergingStrategy::Fetch => "fetch",
                        aifo_coder::MergingStrategy::Octopus => "octopus",
                    };
                    {
                        let use_err = aifo_coder::color_enabled_stderr();
                        eprintln!(
                            "{}",
                            aifo_coder::paint(
                                use_err,
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
                            eprintln!("aifo-coder: merge strategy '{}' completed.", strat);
                            if matches!(
                                cli.fork_merging_strategy,
                                aifo_coder::MergingStrategy::Octopus
                            ) && cli.fork_merging_autoclean
                                && !cli.dry_run
                            {
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
                                        let use_err = aifo_coder::color_enabled_stderr();
                                        eprintln!(
                                            "{}",
                                            aifo_coder::paint(
                                                use_err,
                                                "\x1b[32;1m",
                                                &format!(
                                                    "aifo-coder: disposed fork session {}.",
                                                    sid
                                                )
                                            )
                                        );
                                    }
                                    Err(e) => {
                                        let use_err = aifo_coder::color_enabled_stderr();
                                        eprintln!(
                                            "{}",
                                            aifo_coder::paint(
                                                use_err,
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
                            eprintln!("aifo-coder: merge strategy '{}' failed: {}", strat, e);
                        }
                    }
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
                    if clones.is_empty() {
                        eprintln!("aifo-coder: no panes to create.");
                        return ExitCode::from(1);
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
                                if i % 2 == 0 {
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
                        let session = crate::fork::types::ForkSession {
                            sid: sid.clone(),
                            session_name: session_name.clone(),
                            base_label: base_label.clone(),
                            base_ref_or_sha: base_ref_or_sha.clone(),
                            base_commit_sha: base_commit_sha.clone(),
                            created_at,
                            layout: layout.clone(),
                            agent: agent.to_string(),
                            session_dir: session_dir.clone(),
                        };
                        let container_name = crate::fork::env::pane_container_name(agent, &sid, 1);
                        let pane = crate::fork::types::Pane {
                            index: 1,
                            dir: pane1_dir.clone(),
                            branch: _b.clone(),
                            state_dir: pane_state_dir.clone(),
                            container_name,
                        };
                        let inner = crate::fork::inner::build_inner_powershell(
                            &session,
                            &pane,
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
                        let session = crate::fork::types::ForkSession {
                            sid: sid.clone(),
                            session_name: session_name.clone(),
                            base_label: base_label.clone(),
                            base_ref_or_sha: base_ref_or_sha.clone(),
                            base_commit_sha: base_commit_sha.clone(),
                            created_at,
                            layout: layout.clone(),
                            agent: agent.to_string(),
                            session_dir: session_dir.clone(),
                        };
                        let container_name = crate::fork::env::pane_container_name(agent, &sid, i);
                        let pane = crate::fork::types::Pane {
                            index: i,
                            dir: pane_dir.clone(),
                            branch: _b.clone(),
                            state_dir: pane_state_dir.clone(),
                            container_name,
                        };
                        let inner = crate::fork::inner::build_inner_powershell(
                            &session,
                            &pane,
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
                            let use_err = aifo_coder::color_enabled_stderr();
                            eprintln!(
                                "{}",
                                aifo_coder::paint(
                                    use_err,
                                    "\x1b[33m",
                                    &format!("aifo-coder: note: no waitable orchestrator found; automatic post-fork merging ({}) is unavailable.", strat)
                                )
                            );
                            eprintln!(
                                "{}",
                                aifo_coder::paint(
                                    use_err,
                                    "\x1b[33m",
                                    &format!(
                                        "aifo-coder: after you close all panes, run: aifo-coder fork merge --session {} --strategy {}",
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
            let inner = build_ps_inner(i, pane_dir.as_path(), &pane_state_dir);

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
                    "Clones remain under {} for recovery.",
                    session_dir.display()
                );
            }
            // Update metadata with panes_created
            let existing: Vec<(PathBuf, String)> = clones
                .iter()
                .filter(|(p, _)| p.exists())
                .map(|(p, b)| (p.clone(), b.clone()))
                .collect();
            let panes_created = existing.len();
            let pane_dirs_vec: Vec<String> = existing
                .iter()
                .map(|(p, _)| p.display().to_string())
                .collect();
            let branches_vec: Vec<String> = existing.iter().map(|(_, b)| b.clone()).collect();
            let mut meta2 = format!(
                "{{ \"created_at\": {}, \"base_label\": {}, \"base_ref_or_sha\": {}, \"base_commit_sha\": {}, \"panes\": {}, \"panes_created\": {}, \"pane_dirs\": [{}], \"branches\": [{}], \"layout\": {}",
                created_at,
                aifo_coder::json_escape(&base_label),
                aifo_coder::json_escape(&base_ref_or_sha),
                aifo_coder::json_escape(&base_commit_sha),
                panes,
                panes_created,
                pane_dirs_vec.iter().map(|s| aifo_coder::json_escape(s).to_string()).collect::<Vec<_>>().join(", "),
                branches_vec.iter().map(|s| aifo_coder::json_escape(s).to_string()).collect::<Vec<_>>().join(", "),
                aifo_coder::json_escape(&layout)
            );
            if let Some(ref snap) = snapshot_sha {
                meta2.push_str(&format!(
                    ", \"snapshot_sha\": {}",
                    aifo_coder::json_escape(snap)
                ));
            }
            meta2.push_str(" }");
            let _ = fs::write(session_dir.join(".meta.json"), meta2);
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
            let strat = match cli.fork_merging_strategy {
                aifo_coder::MergingStrategy::None => "none",
                aifo_coder::MergingStrategy::Fetch => "fetch",
                aifo_coder::MergingStrategy::Octopus => "octopus",
            };
            {
                let use_err = aifo_coder::color_enabled_stderr();
                eprintln!(
                    "{}",
                    aifo_coder::paint(
                        use_err,
                        "\x1b[36;1m",
                        &format!("aifo-coder: applying post-fork merge strategy: {}", strat)
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
                                &format!("aifo-coder: merge strategy '{}' completed.", strat)
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
                        {
                            let use_err = aifo_coder::color_enabled_stderr();
                            eprintln!(
                                "{}",
                                aifo_coder::paint(
                                    use_err,
                                    "\x1b[36;1m",
                                    &format!(
                                        "aifo-coder: octopus merge succeeded; disposing fork session {} ...",
                                        sid
                                    )
                                )
                            );
                        }
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
                                let use_err = aifo_coder::color_enabled_stderr();
                                eprintln!(
                                    "{}",
                                    aifo_coder::paint(
                                        use_err,
                                        "\x1b[32;1m",
                                        &format!("aifo-coder: disposed fork session {}.", sid)
                                    )
                                );
                            }
                            Err(e) => {
                                let use_err = aifo_coder::color_enabled_stderr();
                                eprintln!(
                                    "{}",
                                    aifo_coder::paint(
                                        use_err,
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
                    let use_err = aifo_coder::color_enabled_stderr();
                    eprintln!(
                        "{}",
                        aifo_coder::paint(
                            use_err,
                            "\x1b[31;1m",
                            &format!("aifo-coder: merge strategy '{}' failed: {}", strat, e)
                        )
                    );
                }
            }
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
        if clones.is_empty() {
            eprintln!("aifo-coder: no panes to create.");
            return ExitCode::from(1);
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
        let session = crate::fork::types::ForkSession {
            sid: sid.clone(),
            session_name: session_name.clone(),
            base_label: base_label.clone(),
            base_ref_or_sha: base_ref_or_sha.clone(),
            base_commit_sha: base_commit_sha.clone(),
            created_at,
            layout: layout.clone(),
            agent: agent.to_string(),
            session_dir: session_dir.clone(),
        };
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
                        "Clones remain under {} for recovery.",
                        session_dir.display()
                    );
                }
                // Update metadata
                let existing: Vec<(PathBuf, String)> = clones
                    .iter()
                    .filter(|(p, _)| p.exists())
                    .map(|(p, b)| (p.clone(), b.clone()))
                    .collect();
                let panes_created = existing.len();
                let pane_dirs_vec: Vec<String> = existing
                    .iter()
                    .map(|(p, _)| p.display().to_string())
                    .collect();
                let branches_vec: Vec<String> = existing.iter().map(|(_, b)| b.clone()).collect();
                let mut meta2 = format!(
                    "{{ \"created_at\": {}, \"base_label\": {}, \"base_ref_or_sha\": {}, \"base_commit_sha\": {}, \"panes\": {}, \"panes_created\": {}, \"pane_dirs\": [{}], \"branches\": [{}], \"layout\": {}",
                    created_at,
                    aifo_coder::json_escape(&base_label),
                    aifo_coder::json_escape(&base_ref_or_sha),
                    aifo_coder::json_escape(&base_commit_sha),
                    panes,
                    panes_created,
                    pane_dirs_vec.iter().map(|s| aifo_coder::json_escape(s).to_string()).collect::<Vec<_>>().join(", "),
                    branches_vec.iter().map(|s| aifo_coder::json_escape(s).to_string()).collect::<Vec<_>>().join(", "),
                    aifo_coder::json_escape(&layout)
                );
                if let Some(ref snap) = snapshot_sha {
                    meta2.push_str(&format!(
                        ", \"snapshot_sha\": {}",
                        aifo_coder::json_escape(snap)
                    ));
                }
                meta2.push_str(" }");
                let _ = fs::write(session_dir.join(".meta.json"), meta2);
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
                    "Clones remain under {} for recovery.",
                    session_dir.display()
                );
                if let Some((first_dir, first_branch)) = clones.first() {
                    println!("Example recovery:");
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
            let pane = crate::fork::types::Pane {
                index: i,
                dir: clones[idx].0.clone(),
                branch: clones[idx].1.clone(),
                state_dir: pane_state_dir.clone(),
                container_name,
            };
            // Touch fields so clippy sees them as read on this target too
            let _ = (&pane.dir, &pane.branch);
            let inner = crate::fork::inner::build_tmux_launch_script(
                &session,
                &pane,
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
                    let use_err = aifo_coder::color_enabled_stderr();
                    eprintln!(
                        "{}",
                        aifo_coder::paint(
                            use_err,
                            "\x1b[36;1m",
                            &format!("aifo-coder: applying post-fork merge strategy: {}", strat)
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
                                    &format!("aifo-coder: merge strategy '{}' completed.", strat)
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
                            {
                                let use_err = aifo_coder::color_enabled_stderr();
                                eprintln!(
                                    "{}",
                                    aifo_coder::paint(
                                        use_err,
                                        "\x1b[36;1m",
                                        &format!(
                                            "aifo-coder: octopus merge succeeded; disposing fork session {} ...",
                                            sid
                                        )
                                    )
                                );
                            }
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
                                    let use_err = aifo_coder::color_enabled_stderr();
                                    eprintln!(
                                        "{}",
                                        aifo_coder::paint(
                                            use_err,
                                            "\x1b[32;1m",
                                            &format!("aifo-coder: disposed fork session {}.", sid)
                                        )
                                    );
                                }
                                Err(e) => {
                                    let use_err = aifo_coder::color_enabled_stderr();
                                    eprintln!(
                                        "{}",
                                        aifo_coder::paint(
                                            use_err,
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
                        let use_err = aifo_coder::color_enabled_stderr();
                        eprintln!(
                            "{}",
                            aifo_coder::paint(
                                use_err,
                                "\x1b[31;1m",
                                &format!("aifo-coder: merge strategy '{}' failed: {}", strat, e)
                            )
                        );
                    }
                }
            }
            ExitCode::from(0)
        }
    }
}

struct OutputNewlineGuard;

impl Drop for OutputNewlineGuard {
    fn drop(&mut self) {
        // Ensure a trailing blank line on stdout at process end
        println!();
    }
}

fn main() -> ExitCode {
    // Leading blank line at program start
    println!();
    let _aifo_output_newline_guard = OutputNewlineGuard;
    // Load environment variables from .env if present (no error if missing)
    dotenvy::dotenv().ok();
    let cli = Cli::parse();
    // Configure color mode as early as possible (only when explicitly provided on CLI)
    if let Some(mode) = cli.color {
        aifo_coder::set_color_mode(mode);
    }

    // Optional: invalidate on-disk registry cache before any probes
    if cli.invalidate_registry_cache {
        aifo_coder::invalidate_registry_cache();
    }

    // Apply CLI flavor override by setting the environment variable the launcher uses
    if let Some(flavor) = cli.flavor {
        match flavor {
            Flavor::Full => std::env::set_var("AIFO_CODER_IMAGE_FLAVOR", "full"),
            Flavor::Slim => std::env::set_var("AIFO_CODER_IMAGE_FLAVOR", "slim"),
        }
    }

    // Fork orchestrator (Phase 3): run early if requested
    if let Some(n) = cli.fork {
        if n >= 2 {
            return fork_run(&cli, n);
        }
    }
    // Optional auto-clean of stale fork sessions and stale notice (Phase 6)
    // Suppress stale notice here when running 'doctor' (doctor prints its own notice).
    if !matches!(cli.command, Agent::Fork { .. }) && !matches!(cli.command, Agent::Doctor) {
        aifo_coder::fork_autoclean_if_enabled();
        // Stale sessions notice (Phase 6): print suggestions for old fork sessions on normal runs
        aifo_coder::fork_print_stale_notice();
    }

    // Fork maintenance subcommands (Phase 6): operate without starting agents or acquiring locks
    if let Agent::Fork { cmd } = &cli.command {
        match cmd {
            ForkCmd::List {
                json,
                all_repos,
                color,
            } => {
                if let Some(mode) = color {
                    aifo_coder::set_color_mode(*mode);
                }
                if *all_repos {
                    // In all-repos mode, do not require being inside a Git repo; workspace root is taken from env
                    let dummy = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
                    let code = aifo_coder::fork_list(&dummy, *json, true).unwrap_or(1);
                    return ExitCode::from(code as u8);
                } else {
                    let repo_root = match aifo_coder::repo_root() {
                        Some(p) => p,
                        None => {
                            eprintln!("aifo-coder: error: fork maintenance commands must be run inside a Git repository.");
                            return ExitCode::from(1);
                        }
                    };
                    let code = aifo_coder::fork_list(&repo_root, *json, false).unwrap_or(1);
                    return ExitCode::from(code as u8);
                }
            }
            ForkCmd::Clean {
                session,
                older_than,
                all,
                dry_run,
                yes,
                force,
                keep_dirty,
                json,
            } => {
                let repo_root = match aifo_coder::repo_root() {
                    Some(p) => p,
                    None => {
                        eprintln!("aifo-coder: error: fork maintenance commands must be run inside a Git repository.");
                        return ExitCode::from(1);
                    }
                };
                let opts = aifo_coder::ForkCleanOpts {
                    session: session.clone(),
                    older_than_days: *older_than,
                    all: *all,
                    dry_run: *dry_run,
                    yes: *yes,
                    force: *force,
                    keep_dirty: *keep_dirty,
                    json: *json,
                };
                let code = aifo_coder::fork_clean(&repo_root, &opts).unwrap_or(1);
                return ExitCode::from(code as u8);
            }
            ForkCmd::Merge {
                session,
                strategy,
                autoclean,
                dry_run,
            } => {
                let repo_root = match aifo_coder::repo_root() {
                    Some(p) => p,
                    None => {
                        eprintln!("aifo-coder: error: fork maintenance commands must be run inside a Git repository.");
                        return ExitCode::from(1);
                    }
                };
                match aifo_coder::fork_merge_branches_by_session(
                    &repo_root,
                    session,
                    *strategy,
                    cli.verbose,
                    *dry_run,
                ) {
                    Ok(()) => {
                        if matches!(strategy, aifo_coder::MergingStrategy::Octopus)
                            && *autoclean
                            && !*dry_run
                        {
                            let use_err = aifo_coder::color_enabled_stderr();
                            eprintln!(
                                "{}",
                                aifo_coder::paint(
                                    use_err,
                                    "\x1b[36;1m",
                                    &format!(
                                        "aifo-coder: octopus merge succeeded; disposing fork session {} ...",
                                        session
                                    )
                                )
                            );
                            let opts = aifo_coder::ForkCleanOpts {
                                session: Some(session.clone()),
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
                                    let use_err = aifo_coder::color_enabled_stderr();
                                    eprintln!(
                                        "{}",
                                        aifo_coder::paint(
                                            use_err,
                                            "\x1b[32;1m",
                                            &format!(
                                                "aifo-coder: disposed fork session {}.",
                                                session
                                            )
                                        )
                                    );
                                }
                                Err(e) => {
                                    let use_err = aifo_coder::color_enabled_stderr();
                                    eprintln!(
                                        "{}",
                                        aifo_coder::paint(
                                            use_err,
                                            "\x1b[33m",
                                            &format!(
                                                "aifo-coder: warning: failed to dispose fork session {}: {}",
                                                session, e
                                            )
                                        )
                                    );
                                }
                            }
                        }
                        return ExitCode::from(0);
                    }
                    Err(e) => {
                        {
                            let use_err = aifo_coder::color_enabled_stderr();
                            eprintln!(
                                "{}",
                                aifo_coder::paint(
                                    use_err,
                                    "\x1b[31;1m",
                                    &format!("aifo-coder: fork merge failed: {}", e)
                                )
                            );
                        }
                        return ExitCode::from(1);
                    }
                }
            }
        }
    }

    // Doctor subcommand runs diagnostics without acquiring a lock
    if let Agent::Doctor = &cli.command {
        return crate::commands::run_doctor_command(&cli);
    } else if let Agent::Images = &cli.command {
        return crate::commands::run_images(&cli);
    } else if let Agent::CacheClear = &cli.command {
        return crate::commands::run_cache_clear(&cli);
    } else if let Agent::ToolchainCacheClear = &cli.command {
        return crate::commands::run_toolchain_cache_clear(&cli);
    } else if let Agent::Toolchain {
        kind,
        image,
        no_cache,
        args,
    } = &cli.command
    {
        return crate::commands::run_toolchain(&cli, *kind, image.clone(), *no_cache, args.clone());
    }

    // Build docker command and run it
    let (agent, args) = match &cli.command {
        Agent::Codex { args } => ("codex", args.clone()),
        Agent::Crush { args } => ("crush", args.clone()),
        Agent::Aider { args } => ("aider", args.clone()),
        Agent::Doctor => {
            unreachable!("Doctor subcommand is handled earlier and returns immediately")
        }
        Agent::Images => {
            unreachable!("Images subcommand is handled earlier and returns immediately")
        }
        Agent::CacheClear => {
            unreachable!("CacheClear subcommand is handled earlier and returns immediately")
        }
        Agent::ToolchainCacheClear => unreachable!(
            "ToolchainCacheClear subcommand is handled earlier and returns immediately"
        ),
        Agent::Toolchain { .. } => {
            unreachable!("Toolchain subcommand is handled earlier and returns immediately")
        }
        Agent::Fork { .. } => {
            unreachable!("Fork maintenance subcommands are handled earlier and return immediately")
        }
    };

    // Print startup banner before any further diagnostics
    print_startup_banner();
    maybe_warn_missing_toolchain_agent(&cli, agent);
    if !warn_if_tmp_workspace(true) {
        eprintln!("aborted.");
        return ExitCode::from(1);
    }

    // Phase 3: Toolchain session RAII
    let mut toolchain_session: Option<crate::toolchain_session::ToolchainSession> = None;

    if !cli.toolchain.is_empty() || !cli.toolchain_spec.is_empty() {
        // Reconstruct kinds and overrides (for dry-run previews)
        let mut kinds: Vec<String> = cli
            .toolchain
            .iter()
            .map(|k| k.as_str().to_string())
            .collect();

        fn parse_spec(s: &str) -> (String, Option<String>) {
            let t = s.trim();
            if let Some((k, v)) = t.split_once('@') {
                (k.trim().to_string(), Some(v.trim().to_string()))
            } else {
                (t.to_string(), None)
            }
        }

        let mut spec_versions: Vec<(String, String)> = Vec::new();
        for s in &cli.toolchain_spec {
            let (k, v) = parse_spec(s);
            if !k.is_empty() {
                kinds.push(k.clone());
                if let Some(ver) = v {
                    spec_versions.push((k, ver));
                }
            }
        }
        use std::collections::BTreeSet;
        let mut set = BTreeSet::new();
        let mut kinds_norm: Vec<String> = Vec::new();
        for k in kinds {
            let norm = aifo_coder::normalize_toolchain_kind(&k);
            if set.insert(norm.clone()) {
                kinds_norm.push(norm);
            }
        }
        let kinds = kinds_norm;

        let mut overrides: Vec<(String, String)> = Vec::new();
        for s in &cli.toolchain_image {
            if let Some((k, v)) = s.split_once('=') {
                if !k.trim().is_empty() && !v.trim().is_empty() {
                    overrides.push((
                        aifo_coder::normalize_toolchain_kind(k),
                        v.trim().to_string(),
                    ));
                }
            }
        }
        for (k, ver) in spec_versions {
            let kind = aifo_coder::normalize_toolchain_kind(&k);
            if !overrides.iter().any(|(kk, _)| kk == &kind) {
                let img = aifo_coder::default_toolchain_image_for_version(&kind, &ver);
                overrides.push((kind, img));
            }
        }

        if cli.dry_run {
            if cli.verbose {
                eprintln!("aifo-coder: would attach toolchains: {:?}", kinds);
                if !overrides.is_empty() {
                    eprintln!("aifo-coder: would use image overrides: {:?}", overrides);
                }
                if cli.no_toolchain_cache {
                    eprintln!("aifo-coder: would disable toolchain caches");
                }
                if cfg!(target_os = "linux") && cli.toolchain_unix_socket {
                    eprintln!("aifo-coder: would use unix:/// socket transport for proxy and mount /run/aifo");
                }
                if !cli.toolchain_bootstrap.is_empty() {
                    eprintln!("aifo-coder: would bootstrap: {:?}", cli.toolchain_bootstrap);
                }
                eprintln!("aifo-coder: would prepare and mount /opt/aifo/bin shims; set AIFO_TOOLEEXEC_URL/TOKEN; join aifo-net-<id>");
            }
        } else {
            match crate::toolchain_session::ToolchainSession::start_if_requested(&cli) {
                Ok(Some(ts)) => {
                    toolchain_session = Some(ts);
                }
                Ok(None) => { /* no-op */ }
                Err(_) => {
                    // Errors are already printed inside start_if_requested() with exact strings
                    return ExitCode::from(1);
                }
            }
        }
    }

    let image = cli
        .image
        .clone()
        .unwrap_or_else(|| default_image_for(agent));

    println!();

    let apparmor_profile = desired_apparmor_profile();
    match build_docker_cmd(agent, &args, &image, apparmor_profile.as_deref()) {
        Ok((mut cmd, preview)) => {
            if cli.verbose {
                eprintln!(
                    "aifo-coder: effective apparmor profile: {}",
                    apparmor_profile.as_deref().unwrap_or("(disabled)")
                );
                // Show chosen registry and source for transparency
                let rp = aifo_coder::preferred_registry_prefix_quiet();
                let reg_display = if rp.is_empty() {
                    "Docker Hub".to_string()
                } else {
                    rp.trim_end_matches('/').to_string()
                };
                let reg_src = aifo_coder::preferred_registry_source();
                eprintln!("aifo-coder: registry: {reg_display} (source: {reg_src})");
                eprintln!("aifo-coder: image: {image}");
                eprintln!("aifo-coder: agent: {agent}");
            }
            if cli.verbose || cli.dry_run {
                eprintln!("aifo-coder: docker: {preview}");
            }
            if cli.dry_run {
                eprintln!("aifo-coder: dry-run requested; not executing Docker.");
                return ExitCode::from(0);
            }
            // Acquire lock only for real execution; honor AIFO_CODER_SKIP_LOCK=1 for child panes
            let skip_lock = std::env::var("AIFO_CODER_SKIP_LOCK").ok().as_deref() == Some("1");
            let maybe_lock = if skip_lock {
                None
            } else {
                match acquire_lock() {
                    Ok(f) => Some(f),
                    Err(e) => {
                        eprintln!("{e}");
                        return ExitCode::from(1);
                    }
                }
            };
            let status = cmd.status().expect("failed to start docker");
            // Release lock before exiting (if held)
            if let Some(lock) = maybe_lock {
                drop(lock);
            }

            // Toolchain session cleanup (RAII)
            let in_fork_pane = std::env::var("AIFO_CODER_FORK_SESSION")
                .ok()
                .filter(|s| !s.trim().is_empty())
                .is_some();
            if let Some(ts) = toolchain_session.take() {
                ts.cleanup(cli.verbose, in_fork_pane);
            }

            ExitCode::from(status.code().unwrap_or(1) as u8)
        }
        Err(e) => {
            eprintln!("{e}");
            // Toolchain session cleanup on error (RAII)
            let in_fork_pane = std::env::var("AIFO_CODER_FORK_SESSION")
                .ok()
                .filter(|s| !s.trim().is_empty())
                .is_some();
            if let Some(ts) = toolchain_session.take() {
                ts.cleanup(cli.verbose, in_fork_pane);
            }
            if e.kind() == io::ErrorKind::NotFound {
                return ExitCode::from(127);
            }
            ExitCode::from(1)
        }
    }
}
