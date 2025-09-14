use clap::Parser;
use std::io;
use std::path::PathBuf;
use std::process::ExitCode;
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
    pub mod cleanup;
    pub mod env;
    pub mod inner;
    pub mod meta;
    pub mod orchestrators;
    pub mod post_merge;
    pub mod preflight;
    pub mod runner;
    pub mod session;
    pub mod summary;
    pub mod types;
}
use crate::agent_images::default_image_for;
use crate::banner::print_startup_banner;
use crate::cli::{Agent, Cli, Flavor, ForkCmd};
use crate::warnings::{maybe_warn_missing_toolchain_agent, warn_if_tmp_workspace};

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
            return crate::fork::runner::fork_run(&cli, n);
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

    let apparmor_profile = aifo_coder::desired_apparmor_profile();
    match aifo_coder::build_docker_cmd(agent, &args, &image, apparmor_profile.as_deref()) {
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
                match aifo_coder::acquire_lock() {
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
