use clap::Parser;
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
mod support;
mod toolchain_session;
mod warnings;
// Fork orchestration modules
mod fork {
    pub mod cleanup;
    pub mod env;
    pub mod inner;
    pub mod meta {
        pub use aifo_coder::fork_meta::*;
    }
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
    // Propagate verbosity to runtime so image pulls can stream progress/output.
    if cli.verbose {
        std::env::set_var("AIFO_CODER_VERBOSE", "1");
    }
}

fn require_repo_root() -> Result<PathBuf, ExitCode> {
    match aifo_coder::repo_root() {
        Some(p) => Ok(p),
        None => {
            let use_err = aifo_coder::color_enabled_stderr();
            aifo_coder::log_error_stderr(
                use_err,
                "aifo-coder: error: fork maintenance commands must be run inside a Git repository.",
            );
            Err(ExitCode::from(1))
        }
    }
}

fn handle_fork_maintenance(cli: &Cli) -> Option<ExitCode> {
    let use_err_color = aifo_coder::color_enabled_stderr();
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
                            eprintln!(
                                "{}",
                                aifo_coder::paint(
                                    use_err_color,
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
                                    eprintln!(
                                        "{}",
                                        aifo_coder::paint(
                                            use_err_color,
                                            "\x1b[32;1m",
                                            &format!(
                                                "aifo-coder: disposed fork session {}.",
                                                session
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
                        aifo_coder::log_error_stderr(
                            use_err_color,
                            &format!("aifo-coder: fork merge failed: {}", e),
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
        Agent::Support {
            all,
            base,
            deep,
            combo,
        } => {
            // Default to --all when no specific mode flags are provided
            let run_all = *all || (!*base && !*deep && !*combo);
            if run_all {
                Some(crate::support::run_support_all(cli.verbose, cli.quiet))
            } else {
                let mut any_fail = false;
                if *base {
                    // Baseline: no deep/combo
                    std::env::set_var("AIFO_SUPPORT_DEEP", "0");
                    std::env::set_var("AIFO_SUPPORT_COMBO", "0");
                    let code = crate::support::run_support(cli.verbose, cli.quiet, None);
                    if code != ExitCode::SUCCESS {
                        any_fail = true;
                    }
                }
                if *deep {
                    // Deep-only
                    std::env::set_var("AIFO_SUPPORT_DEEP", "1");
                    std::env::set_var("AIFO_SUPPORT_COMBO", "0");
                    let code = crate::support::run_support(
                        cli.verbose,
                        true, /* suppress banner */
                        None,
                    );
                    if code != ExitCode::SUCCESS {
                        any_fail = true;
                    }
                }
                if *combo {
                    // Combo-only
                    std::env::set_var("AIFO_SUPPORT_DEEP", "0");
                    std::env::set_var("AIFO_SUPPORT_COMBO", "1");
                    let code = crate::support::run_support(
                        cli.verbose,
                        true, /* suppress banner */
                        None,
                    );
                    if code != ExitCode::SUCCESS {
                        any_fail = true;
                    }
                }
                Some(if any_fail {
                    ExitCode::FAILURE
                } else {
                    ExitCode::SUCCESS
                })
            }
        }
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
        Agent::OpenHands { args } => ("openhands", args.clone()).into(),
        Agent::OpenCode { args } => ("opencode", args.clone()).into(),
        Agent::Plandex { args } => ("plandex", args.clone()).into(),
        _ => None,
    }
}

fn print_verbose_run_info(
    agent: &str,
    image_display: &str,
    apparmor_opt: Option<&str>,
    preview: &str,
    cli_verbose: bool,
    dry_run: bool,
) {
    let use_err = aifo_coder::color_enabled_stderr();
    if cli_verbose {
        aifo_coder::log_info_stderr(
            use_err,
            &format!(
                "aifo-coder: effective apparmor profile: {}",
                apparmor_opt.unwrap_or("(disabled)")
            ),
        );
        // Show internal and mirror registries independently (quiet MR probe; IR from env only)
        // Use autodetect for internal registry prefix to reflect resolution in verbose output.
        let irp = aifo_coder::preferred_internal_registry_prefix_autodetect();
        let ir_display = if irp.is_empty() {
            "(none)".to_string()
        } else {
            irp.trim_end_matches('/').to_string()
        };
        // Derive source label: env/env-empty when set, otherwise autodetect/unset.
        let ir_src_raw = aifo_coder::preferred_internal_registry_source();
        let ir_src = if irp.is_empty() {
            "unset".to_string()
        } else if ir_src_raw == "env" || ir_src_raw == "env-empty" {
            ir_src_raw
        } else {
            "autodetect".to_string()
        };

        let mrp = aifo_coder::preferred_mirror_registry_prefix_quiet();
        let mr_display = if mrp.is_empty() {
            "(none)".to_string()
        } else {
            mrp.trim_end_matches('/').to_string()
        };
        let mr_src = aifo_coder::preferred_mirror_registry_source();

        aifo_coder::log_info_stderr(
            use_err,
            &format!(
                "aifo-coder: internal registry: {} (source: internal:{})",
                ir_display, ir_src
            ),
        );
        aifo_coder::log_info_stderr(
            use_err,
            &format!(
                "aifo-coder: mirror registry: {} (source: mirror:{})",
                mr_display, mr_src
            ),
        );
        aifo_coder::log_info_stderr(
            use_err,
            &format!("aifo-coder: agent image [{}]: {}", agent, image_display),
        );
    }
    if cli_verbose || dry_run {
        aifo_coder::log_info_stderr(use_err, &format!("aifo-coder: docker: {}", preview));
    }
}

fn main() -> ExitCode {
    // Leading blank line at program start
    eprintln!();
    // Load environment variables from .env if present (no error if missing)
    dotenvy::dotenv().ok();

    // Parse command-line arguments into structured CLI options
    let cli = Cli::parse();

    // Propagate CLI verbosity to telemetry so init can emit concise OTEL logs when requested.
    if cli.verbose {
        std::env::set_var("AIFO_CODER_OTEL_VERBOSE", "1");
    }
    // If requested, force metrics debug exporter (stderr/file) before telemetry_init.
    if cli.debug_otel_otlp {
        std::env::set_var("AIFO_CODER_OTEL_DEBUG_OTLP", "1");
    }

    

    // Record a single run metric when telemetry+metrics are enabled (agent known later).
    #[cfg(feature = "otel")]
    {
        // Tag will be refined once the concrete agent is known; this initial run has no agent label.
        // Call sites with agent label live in docker.rs metrics hooks.
    }
    if std::env::var("AIFO_GLOBAL_TAG")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .is_some()
        && std::env::var("AIFO_TAG")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .is_none()
        && std::env::var("AIFO_CODER_SUPPRESS_DEPRECATION")
            .ok()
            .as_deref()
            != Some("1")
    {
        let use_err = aifo_coder::color_enabled_stderr();
        aifo_coder::log_warn_stderr(
            use_err,
            "aifo-coder: warning: AIFO_GLOBAL_TAG is no longer supported; use AIFO_TAG instead.",
        );
    }
    // Honor --non-interactive by suppressing the LLM credentials prompt
    if cli.non_interactive {
        std::env::set_var("AIFO_CODER_SUPPRESS_LLM_WARNING", "1");
    }
    apply_cli_globals(&cli);
    let use_err = aifo_coder::color_enabled_stderr();

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
    if !cli.quiet {
        print_startup_banner();
    }
    // Initialize optional OpenTelemetry telemetry if compiled and enabled via env.
    // This is fully best-effort and must not change exit codes or stdout/stderr defaults.
    let _telemetry_guard = aifo_coder::telemetry_init();
    // Print agent-specific environment/toolchain hints when appropriate
    maybe_warn_missing_toolchain_agent(&cli, agent);
    // Abort early when working in a temp directory and the user declines
    if !warn_if_tmp_workspace(true) {
        aifo_coder::log_error_stderr(use_err, "aborted.");
        return ExitCode::from(1);
    }
    // Warn and optionally block if LLM credentials are missing
    if !crate::warnings::warn_if_missing_llm_credentials(true) {
        aifo_coder::log_error_stderr(use_err, "aborted.");
        return ExitCode::from(1);
    }

    // Toolchain session RAII
    let mut _toolchain_session: Option<crate::toolchain_session::ToolchainSession> = None;

    if !cli.toolchain.is_empty() || !cli.toolchain_spec.is_empty() {
        let (kinds, overrides) = crate::toolchain_session::plan_from_cli(&cli);

        if cli.dry_run {
            // Dry-run: print detailed previews and skip starting sidecars/proxy
            if cli.verbose {
                let use_err = aifo_coder::color_enabled_stderr();
                aifo_coder::log_info_stderr(
                    use_err,
                    &format!("aifo-coder: would attach toolchains: {:?}", kinds),
                );
                if !overrides.is_empty() {
                    aifo_coder::log_info_stderr(
                        use_err,
                        &format!("aifo-coder: would use image overrides: {:?}", overrides),
                    );
                }
                if cli.no_toolchain_cache {
                    aifo_coder::log_info_stderr(
                        use_err,
                        "aifo-coder: would disable toolchain caches",
                    );
                }
                if cfg!(target_os = "linux") && cli.toolchain_unix_socket {
                    aifo_coder::log_info_stderr(
                        use_err,
                        "aifo-coder: would use unix:/// socket transport for proxy and mount /run/aifo",
                    );
                }
                if !cli.toolchain_bootstrap.is_empty() {
                    aifo_coder::log_info_stderr(
                        use_err,
                        &format!("aifo-coder: would bootstrap: {:?}", cli.toolchain_bootstrap),
                    );
                }
                aifo_coder::log_info_stderr(
                    use_err,
                    concat!(
                        "aifo-coder: would prepare and mount /opt/aifo/bin shims; set ",
                        "AIFO_TOOLEEXEC_URL/TOKEN; join aifo-net-<id>"
                    ),
                );
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
    // Apply global/agent tag overrides for run when CLI didn't provide an explicit image.
    // Also resolve registry prefix when the tagged image isn't present locally.
    let run_image = if cli.image.is_none() {
        // Prefer AIFO_CODER_IMAGE_TAG over AIFO_TAG
        let tag = std::env::var("AIFO_CODER_IMAGE_TAG")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                std::env::var("AIFO_TAG")
                    .ok()
                    .filter(|s| !s.trim().is_empty())
            });
        if let Some(t) = tag {
            // Retag by removing any existing ':tag' suffix (after the last slash) and appending new tag
            let s = image
                .split_once('@')
                .map(|(n, _)| n.to_string())
                .unwrap_or_else(|| image.clone());
            let last_slash = s.rfind('/');
            let last_colon = s.rfind(':');
            let without_tag = match (last_slash, last_colon) {
                (Some(slash), Some(colon)) if colon > slash => s[..colon].to_string(),
                (None, Some(_colon)) => s.split(':').next().unwrap_or(&s).to_string(),
                _ => s,
            };
            let retagged = format!("{}:{}", without_tag, t.trim());
            aifo_coder::resolve_image(&retagged)
        } else {
            aifo_coder::resolve_image(&image)
        }
    } else {
        image.clone()
    };

    // Determine if a tag override is present in environment (affects agent run image selection).
    let tag_env_present = std::env::var("AIFO_CODER_IMAGE_TAG")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            std::env::var("AIFO_TAG")
                .ok()
                .filter(|s| !s.trim().is_empty())
        })
        .is_some();

    // Finalize run image: CLI override wins; else respect tag env; else prefer local ':latest' when present.
    let run_image_final = if cli.image.is_some() {
        image.clone()
    } else if tag_env_present {
        run_image.clone()
    } else {
        match aifo_coder::compute_effective_agent_image_for_run(&run_image) {
            Ok(s) => s,
            Err(_) => run_image.clone(),
        }
    };

    // Visual separation before Docker info and previews
    eprintln!();

    // Determine desired AppArmor profile (may be disabled on non-Linux)
    let apparmor_profile = aifo_coder::desired_apparmor_profile();

    // Choose image to display in logs:
    // - Dry-run: show CLI verbatim (if set) or resolved image (no Docker check).
    // - Real run: show the effective image (prefers local :latest when present), unless CLI overrides.
    let image_display = if cli.dry_run {
        if cli.image.is_some() {
            image.clone()
        } else {
            aifo_coder::resolve_agent_image_log_display(&image)
        }
    } else if cli.image.is_some() {
        image.clone()
    } else {
        // Use the final image we computed (matches actual run selection)
        run_image_final.clone()
    };

    // In dry-run, render a preview without requiring docker to be present
    if cli.dry_run {
        let preview = aifo_coder::build_docker_preview_only(
            agent,
            &args,
            &image,
            apparmor_profile.as_deref(),
        );
        print_verbose_run_info(
            agent,
            &image_display,
            apparmor_profile.as_deref(),
            &preview,
            cli.verbose,
            true,
        );
        let use_err = aifo_coder::color_enabled_stderr();
        aifo_coder::log_info_stderr(
            use_err,
            "aifo-coder: dry-run requested; not executing Docker.",
        );
        aifo_coder::cleanup_aider_staging_from_env();
        return ExitCode::from(0);
    }

    // Real execution path: require docker runtime
    match aifo_coder::build_docker_cmd(agent, &args, &run_image_final, apparmor_profile.as_deref())
    {
        Ok((mut cmd, preview)) => {
            print_verbose_run_info(
                agent,
                &image_display,
                apparmor_profile.as_deref(),
                &preview,
                cli.verbose,
                false,
            );
            // Acquire lock only for real execution; honor AIFO_CODER_SKIP_LOCK=1 for child panes
            let skip_lock = std::env::var("AIFO_CODER_SKIP_LOCK").ok().as_deref() == Some("1");
            let maybe_lock = if skip_lock {
                None
            } else {
                match aifo_coder::acquire_lock() {
                    Ok(f) => Some(f),
                    Err(e) => {
                        {
                            let use_err = aifo_coder::color_enabled_stderr();
                            aifo_coder::log_error_stderr(use_err, &e.to_string());
                        }
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
            // Remove per-run staged Aider configs (best effort)
            aifo_coder::cleanup_aider_staging_from_env();

            // Toolchain session cleanup handled by Drop on ToolchainSession

            ExitCode::from(status.code().unwrap_or(1) as u8)
        }
        Err(e) => {
            {
                let use_err = aifo_coder::color_enabled_stderr();
                aifo_coder::log_error_stderr(use_err, &e.to_string());
            }
            // Remove per-run staged Aider configs (best effort)
            aifo_coder::cleanup_aider_staging_from_env();
            // Toolchain session cleanup handled by Drop on ToolchainSession (also on error)
            let code = aifo_coder::exit_code_for_io_error(&e);
            ExitCode::from(code)
        }
    }
}
