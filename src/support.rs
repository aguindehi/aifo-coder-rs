use std::process::ExitCode;

use crate::banner::print_startup_banner;

/// Phase 2: Module scaffolding for support matrix.
/// - Detect docker path; on error, print a red line and return 1.
/// - Print header: version/host lines via banner; blank line; then "support matrix:".
pub fn run_support(_verbose: bool) -> ExitCode {
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

    ExitCode::from(0)
}
