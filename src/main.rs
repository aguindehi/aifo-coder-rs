use clap::Parser;
use std::io;
use std::path::PathBuf;
use std::process::ExitCode;
// Internal modules
mod agent_images;
mod banner;
mod cli;
mod commands;
mod doctor;
mod fork_args;
mod guidance;
mod toolchain_session;
mod warnings;
// Fork orchestration modules
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

fn apply_cli_globals(cli: &Cli) {
    if let Some(mode) = cli.color {
        aifo_coder::set_color_mode(mode);
    }
    if cli.invalidate_registry_cache {
        aifo_coder::invalidate_registry_cache();
    }
    if let Some(flavor) = cli.flavor {
        match flavor {
            Flavor::Full => std::env::set_var("AIFO_CODER_IMAGE_FLAVOR", "full"),
            Flavor::Slim => std::env::set_var("AIFO_CODER_IMAGE_FLAVOR", "slim"),
        }
    }
}

fn require_repo_root() -> Result<PathBuf, ExitCode> {
    match aifo_coder::repo_root() {
        Some(p) => Ok(p),
        None => {
            eprintln!(
                "aifo-coder: error: fork maintenance commands must be run inside a Git repository."
            );
            Err(ExitCode::from(1))
        }
    }
}

fn handle_fork_maintenance(cli: &Cli) -> Option<ExitCode> {
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
                    let dummy = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
                    let code = aifo_coder::fork_list(&dummy, *json, true).unwrap_or(1);
                    return Some(ExitCode::from(code as u8));
                } else {
                    let repo_root = match require_repo_root() {
                        Ok(p) => p,
                        Err(code) => return Some(code),
                    };
                    let code = aifo_coder::fork_list(&repo_root, *json, false).unwrap_or(1);
                    return Some(ExitCode::from(code as u8));
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
                let repo_root = match require_repo_root() {
                    Ok(p) => p,
                    Err(code) => return Some(code),
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
                return Some(ExitCode::from(code as u8));
            }
            ForkCmd::Merge {
                session,
                strategy,
                autoclean,
                dry_run,
            } => {
                let repo_root = match require_repo_root() {
                    Ok(p) => p,
                    Err(code) => return Some(code),
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
                        return Some(ExitCode::from(0));
                    }
                    Err(e) => {
                        let use_err = aifo_coder::color_enabled_stderr();
                        eprintln!(
                            "{}",
                            aifo_coder::paint(
                                use_err,
                                "\x1b[31;1m",
                                &format!("aifo-coder: fork merge failed: {}", e)
                            )
                        );
                        return Some(ExitCode::from(1));
                    }
                }
            }
        }
    }
    None
}

fn handle_misc_subcommands(cli: &Cli) -> Option<ExitCode> {
    match &cli.command {
        Agent::Doctor => Some(crate::commands::run_doctor_command(cli)),
        Agent::Images => Some(crate::commands::run_images(cli)),
        Agent::CacheClear => Some(crate::commands::run_cache_clear(cli)),
        Agent::ToolchainCacheClear => Some(crate::commands::run_toolchain_cache_clear(cli)),
        Agent::Toolchain {
            kind,
            image,
            no_cache,
            args,
        } => Some(crate::commands::run_toolchain(
            cli,
            *kind,
            image.clone(),
            *no_cache,
            args.clone(),
        )),
        _ => None,
    }
}

fn resolve_agent_and_args(cli: &Cli) -> Option<(&'static str, Vec<String>)> {
    match &cli.command {
        Agent::Codex { args } => ("codex", args.clone()).into(),
        Agent::Crush { args } => ("crush", args.clone()).into(),
        Agent::Aider { args } => ("aider", args.clone()).into(),
        _ => None,
    }
}

fn print_verbose_run_info(
    agent: &str,
    image: &str,
    apparmor_opt: Option<&str>,
    preview: &str,
    cli_verbose: bool,
    dry_run: bool,
) {
    if cli_verbose {
        eprintln!(
            "aifo-coder: effective apparmor profile: {}",
            apparmor_opt.unwrap_or("(disabled)")
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
    if cli_verbose || dry_run {
        eprintln!("aifo-coder: docker: {preview}");
    }
}

fn main() -> ExitCode {
    // Leading blank line at program start
    eprintln!();
    // Load environment variables from .env if present (no error if missing)
    dotenvy::dotenv().ok();
    // Parse command-line arguments into structured CLI options
    let cli = Cli::parse();
    apply_cli_globals(&cli);

    // Fork orchestrator: run early if requested
    if let Some(n) = cli.fork {
        if n >= 2 {
            return crate::fork::runner::fork_run(&cli, n);
        }
    }
    // Optional auto-clean of stale fork sessions and stale session notice
    // Suppress stale notice here when running 'doctor' (doctor prints its own notice).
    if !matches!(cli.command, Agent::Fork { .. }) && !matches!(cli.command, Agent::Doctor) {
        aifo_coder::fork_autoclean_if_enabled();
        // Print suggestions for old fork sessions on normal runs
        aifo_coder::fork_print_stale_notice();
    }

    // Fork maintenance via helper
    if let Some(code) = handle_fork_maintenance(&cli) {
        return code;
    }

    // Misc subcommands via helper
    if let Some(code) = handle_misc_subcommands(&cli) {
        return code;
    }

    // Resolve agent and args for container run
    let (agent, args) = match resolve_agent_and_args(&cli) {
        Some(v) => v,
        None => return ExitCode::from(0),
    };

    // Print startup banner before any further diagnostics
    print_startup_banner();
    // Print agent-specific environment/toolchain hints when appropriate
    maybe_warn_missing_toolchain_agent(&cli, agent);
    // Abort early when working in a temp directory and the user declines
    if !warn_if_tmp_workspace(true) {
        eprintln!("aborted.");
        return ExitCode::from(1);
    }

    // Toolchain session RAII
    let mut _toolchain_session: Option<crate::toolchain_session::ToolchainSession> = None;

    if !cli.toolchain.is_empty() || !cli.toolchain_spec.is_empty() {
        let (kinds, overrides) = crate::toolchain_session::plan_from_cli(&cli);

        if cli.dry_run {
            // Dry-run: print detailed previews and skip starting sidecars/proxy
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
                    // Toolchain sidecars and proxy started
                    _toolchain_session = Some(ts);
                }
                Ok(None) => { /* no-op: no toolchains requested or dry-run */ }
                Err(_) => {
                    // Errors are already printed inside start_if_requested() with exact strings
                    return ExitCode::from(1);
                }
            }
        }
    }

    // Resolve effective image reference (CLI override > environment > computed default)
    let image = cli
        .image
        .clone()
        .unwrap_or_else(|| default_image_for(agent));

    // Visual separation before Docker info and previews
    eprintln!();

    // Determine desired AppArmor profile (may be disabled on non-Linux)
    let apparmor_profile = aifo_coder::desired_apparmor_profile();
    match aifo_coder::build_docker_cmd(agent, &args, &image, apparmor_profile.as_deref()) {
        Ok((mut cmd, preview)) => {
            print_verbose_run_info(
                agent,
                &image,
                apparmor_profile.as_deref(),
                &preview,
                cli.verbose,
                cli.dry_run,
            );
            if cli.dry_run {
                // Skip actual Docker execution in dry-run mode
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
            // Execute Docker and capture its exit status for propagation
            let status = cmd.status().expect("failed to start docker");
            // Release lock before exiting (if held)
            if let Some(lock) = maybe_lock {
                drop(lock);
            }

            // Toolchain session cleanup handled by Drop on ToolchainSession

            ExitCode::from(status.code().unwrap_or(1) as u8)
        }
        Err(e) => {
            eprintln!("{e}");
            // Toolchain session cleanup handled by Drop on ToolchainSession (also on error)
            // Map docker-not-found to exit status 127 (command not found)
            if e.kind() == io::ErrorKind::NotFound {
                return ExitCode::from(127);
            }
            ExitCode::from(1)
        }
    }
}
