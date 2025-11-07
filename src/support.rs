use std::process::ExitCode;
use std::time::{Duration, SystemTime};

use crate::banner::print_startup_banner;

/// Default agent list
fn agents_default() -> Vec<&'static str> {
    vec!["aider", "crush", "codex", "openhands", "opencode", "plandex"]
}

/// Default toolchain kinds
fn toolchains_default() -> Vec<&'static str> {
    vec!["rust", "node", "typescript", "python", "c-cpp", "go"]
}

/// Parse CSV environment override or return defaults.
fn parse_csv_env(name: &str, default: Vec<&str>) -> Vec<String> {
    match std::env::var(name) {
        Ok(v) => {
            let s = v.trim();
            if s.is_empty() {
                default.into_iter().map(|x| x.to_string()).collect()
            } else {
                s.split(',')
                    .map(|t| t.trim().to_string())
                    .filter(|t| !t.is_empty())
                    .collect()
            }
        }
        Err(_) => default.into_iter().map(|x| x.to_string()).collect(),
    }
}

/// Derive a u64 seed from env or from time
fn support_rand_seed() -> u64 {
    if let Ok(v) = std::env::var("AIFO_SUPPORT_RAND_SEED") {
        if let Ok(n) = v.trim().parse::<u64>() {
            return n;
        }
    }
    // Fallback: derive from time ^ pid for good-enough randomness
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_nanos();
    let pid = std::process::id() as u128;
    (now ^ pid) as u64
}

/// Phase 2+3: scaffolding + lists/images/RNG seed.
/// - Detect docker path; on error, print a red line and return 1.
/// - Print header: version/host lines via banner; blank line; then "support matrix:".
/// - Build agents/toolchains via defaults and AIFO_SUPPORT_* overrides.
/// - Resolve images via default_image_for_quiet/default_toolchain_image.
/// - Initialize RNG seed from env or time; log effective seed when verbose.
pub fn run_support(verbose: bool) -> ExitCode {
    // Print startup header (version/host lines)
    print_startup_banner();

    // Require docker runtime; print prominent red line and exit 1 on missing
    if let Err(e) = aifo_coder::container_runtime_path() {
        let use_err = aifo_coder::color_enabled_stderr();
        aifo_coder::log_error_stderr(use_err, &format!("aifo-coder: {}", e));
        return ExitCode::from(1);
    }

    // Header line for the matrix
    eprintln!();
    let use_err = aifo_coder::color_enabled_stderr();
    aifo_coder::log_info_stderr(use_err, "support matrix:");

    // Phase 3: lists, images and RNG
    let agents = parse_csv_env("AIFO_SUPPORT_AGENTS", agents_default());
    let toolchains = parse_csv_env("AIFO_SUPPORT_TOOLCHAINS", toolchains_default());

    // Resolve images
    let _agent_images: Vec<(String, String)> = agents
        .iter()
        .map(|a| (a.clone(), crate::agent_images::default_image_for_quiet(a)))
        .collect();
    let _toolchain_images: Vec<(String, String)> = toolchains
        .iter()
        .map(|k| (k.clone(), aifo_coder::default_toolchain_image(k)))
        .collect();

    // Initialize RNG seed and log when verbose
    let seed = support_rand_seed();
    if verbose {
        aifo_coder::log_info_stderr(use_err, &format!("aifo-coder: support rand-seed: {}", seed));
    }

    ExitCode::from(0)
}
