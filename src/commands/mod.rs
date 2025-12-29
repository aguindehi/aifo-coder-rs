use crate::agent_images::default_image_for;
use crate::banner::print_startup_banner;
use crate::cli::{Cli, ToolchainSpec};
use crate::doctor::run_doctor;
use crate::warnings::warn_if_tmp_workspace;

pub fn images_effective() -> Vec<(String, String)> {
    // Keep order consistent with docs and tests
    let agents = [
        "codex",
        "crush",
        "aider",
        "openhands",
        "opencode",
        "plandex",
    ];
    agents
        .iter()
        .map(|a| (a.to_string(), default_image_for(a)))
        .collect()
}

pub fn run_images(cli: &Cli) -> std::process::ExitCode {
    let _ = cli; // silence unused for future extensions
    print_startup_banner();
    let _ = warn_if_tmp_workspace(false);

    let use_err = aifo_coder::color_enabled_stderr();
    aifo_coder::log_info_stderr(use_err, "aifo-coder images");
    eprintln!();

    // Flavor and registries display
    let flavor_env = std::env::var("AIFO_CODER_IMAGE_FLAVOR").unwrap_or_default();
    let flavor = if flavor_env.trim().eq_ignore_ascii_case("slim") {
        "slim"
    } else {
        "full"
    };

    let irp = aifo_coder::preferred_internal_registry_prefix_quiet();
    let ir_display = if irp.is_empty() {
        "(none)".to_string()
    } else {
        irp.trim_end_matches('/').to_string()
    };

    let mrp = aifo_coder::preferred_mirror_registry_prefix_quiet();
    let mr_display = if mrp.is_empty() {
        "(none)".to_string()
    } else {
        mrp.trim_end_matches('/').to_string()
    };

    let use_color = atty::is(atty::Stream::Stderr);
    let flavor_val = if use_color {
        format!("\x1b[34;1m{}\x1b[0m", flavor)
    } else {
        flavor.to_string()
    };
    let ir_val = if use_color {
        format!("\x1b[34;1m{}\x1b[0m", ir_display)
    } else {
        ir_display
    };
    let mr_val = if use_color {
        format!("\x1b[34;1m{}\x1b[0m", mr_display)
    } else {
        mr_display
    };

    eprintln!("  flavor: {}", flavor_val);
    eprintln!("  internal registry: {}", ir_val);
    eprintln!("  mirror registry: {}", mr_val);
    eprintln!();

    // Effective image references
    let pairs = images_effective();

    for (agent, img) in &pairs {
        let val = if use_color {
            format!("\x1b[34;1m{}\x1b[0m", img)
        } else {
            img.clone()
        };
        eprintln!("  {}: {}", agent, val);
    }
    eprintln!();

    // stdout: machine-readable image list (no colors, no banner)
    for (agent, img) in pairs {
        println!("{} {}", agent, img);
    }

    std::process::ExitCode::from(0)
}

pub fn run_cache_clear(_cli: &Cli) -> std::process::ExitCode {
    aifo_coder::invalidate_registry_cache();
    let use_err = aifo_coder::color_enabled_stderr();
    aifo_coder::log_info_stderr(use_err, "aifo-coder: cleared on-disk registry cache.");
    std::process::ExitCode::from(0)
}

pub fn run_toolchain_cache_clear(cli: &Cli) -> std::process::ExitCode {
    print_startup_banner();
    let _ = warn_if_tmp_workspace(false);
    match aifo_coder::toolchain_purge_caches(cli.verbose) {
        Ok(()) => {
            let use_err = aifo_coder::color_enabled_stderr();
            aifo_coder::log_info_stderr(use_err, "aifo-coder: purged toolchain cache volumes.");
            std::process::ExitCode::from(0)
        }
        Err(e) => {
            let use_err = aifo_coder::color_enabled_stderr();
            aifo_coder::log_error_stderr(
                use_err,
                &format!("aifo-coder: failed to purge toolchain caches: {}", e),
            );
            std::process::ExitCode::from(aifo_coder::exit_code_for_io_error(&e))
        }
    }
}

pub fn run_toolchain(
    cli: &Cli,
    spec: ToolchainSpec,
    no_cache: bool,
    args: Vec<String>,
) -> std::process::ExitCode {
    print_startup_banner();
    let use_err = aifo_coder::color_enabled_stderr();
    if !warn_if_tmp_workspace(true) {
        aifo_coder::log_error_stderr(use_err, "aborted.");
        return std::process::ExitCode::from(1);
    }

    let kind = spec.kind.as_str();
    let image_override = spec.resolved_image_override();

    let use_err = aifo_coder::color_enabled_stderr();
    if cli.verbose {
        aifo_coder::log_info_stderr(use_err, &format!("aifo-coder: toolchain kind: {}", kind));
        if let Some(img) = image_override.as_deref() {
            aifo_coder::log_info_stderr(
                use_err,
                &format!("aifo-coder: toolchain image override: {}", img),
            );
        }
        if no_cache {
            aifo_coder::log_info_stderr(
                use_err,
                "aifo-coder: toolchain caches disabled for this run",
            );
        }
    }

    if cli.dry_run {
        let _ =
            aifo_coder::toolchain_run(kind, &args, image_override.as_deref(), no_cache, true, true);
        return std::process::ExitCode::from(0);
    }

    let code = match aifo_coder::toolchain_run(
        kind,
        &args,
        image_override.as_deref(),
        no_cache,
        cli.verbose,
        false,
    ) {
        Ok(c) => c,
        Err(e) => {
            let use_err = aifo_coder::color_enabled_stderr();
            aifo_coder::log_error_stderr(use_err, &e.to_string());
            aifo_coder::exit_code_for_io_error(&e) as i32
        }
    };
    std::process::ExitCode::from((code & 0xff) as u8)
}

pub fn run_doctor_command(cli: &Cli) -> std::process::ExitCode {
    print_startup_banner();
    let _ = warn_if_tmp_workspace(false);
    run_doctor(cli.verbose);
    std::process::ExitCode::from(0)
}
