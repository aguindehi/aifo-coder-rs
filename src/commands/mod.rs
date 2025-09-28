use crate::agent_images::default_image_for;
use crate::banner::print_startup_banner;
use crate::cli::{Cli, ToolchainKind};
use crate::doctor::run_doctor;
use crate::warnings::warn_if_tmp_workspace;

pub fn run_images(cli: &Cli) -> std::process::ExitCode {
    let _ = cli; // silence unused for future extensions
    print_startup_banner();
    let _ = warn_if_tmp_workspace(false);
    let use_err = aifo_coder::color_enabled_stderr();
    aifo_coder::log_info_stderr(use_err, "aifo-coder images");
    eprintln!();

    // Flavor and registry display
    let flavor_env = std::env::var("AIFO_CODER_IMAGE_FLAVOR").unwrap_or_default();
    let flavor = if flavor_env.trim().eq_ignore_ascii_case("slim") {
        "slim"
    } else {
        "full"
    };
    let rp = aifo_coder::preferred_registry_prefix_quiet();
    let reg_display = if rp.is_empty() {
        "Docker Hub".to_string()
    } else {
        rp.trim_end_matches('/').to_string()
    };

    let use_color = atty::is(atty::Stream::Stderr);
    let flavor_val = if use_color {
        format!("\x1b[34;1m{}\x1b[0m", flavor)
    } else {
        flavor.to_string()
    };
    let reg_val = if use_color {
        format!("\x1b[34;1m{}\x1b[0m", reg_display)
    } else {
        reg_display
    };

    eprintln!("  flavor:   {}", flavor_val);
    eprintln!("  registry: {}", reg_val);
    eprintln!();

    // Effective image references
    let codex_img = default_image_for("codex");
    let crush_img = default_image_for("crush");
    let aider_img = default_image_for("aider");
    let codex_val = if use_color {
        format!("\x1b[34;1m{}\x1b[0m", &codex_img)
    } else {
        codex_img.clone()
    };
    let crush_val = if use_color {
        format!("\x1b[34;1m{}\x1b[0m", &crush_img)
    } else {
        crush_img.clone()
    };
    let aider_val = if use_color {
        format!("\x1b[34;1m{}\x1b[0m", &aider_img)
    } else {
        aider_img.clone()
    };
    eprintln!("  codex: {}", codex_val);
    eprintln!("  crush: {}", crush_val);
    eprintln!("  aider: {}", aider_val);
    eprintln!();

    // stdout: machine-readable image list (no colors, no banner)
    println!("codex {}", codex_img);
    println!("crush {}", crush_img);
    println!("aider {}", aider_img);

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
    kind: ToolchainKind,
    image: Option<String>,
    no_cache: bool,
    args: Vec<String>,
) -> std::process::ExitCode {
    print_startup_banner();
    if !warn_if_tmp_workspace(true) {
        eprintln!("aborted.");
        return std::process::ExitCode::from(1);
    }
    let use_err = aifo_coder::color_enabled_stderr();
    if cli.verbose {
        aifo_coder::log_info_stderr(
            use_err,
            &format!("aifo-coder: toolchain kind: {}", kind.as_str()),
        );
        if let Some(img) = image.as_deref() {
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
            aifo_coder::toolchain_run(kind.as_str(), &args, image.as_deref(), no_cache, true, true);
        return std::process::ExitCode::from(0);
    }
    let code = match aifo_coder::toolchain_run(
        kind.as_str(),
        &args,
        image.as_deref(),
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
