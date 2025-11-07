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

/// Terminal width detection (best-effort)
fn terminal_width_or_default() -> usize {
    // Prefer COLUMNS when set; otherwise target ≤100
    if let Ok(cols) = std::env::var("COLUMNS") {
        if let Ok(n) = cols.trim().parse::<usize>() {
            if n >= 40 {
                return n.min(200);
            }
        }
    }
    100
}

/// Compute layout columns; compress cells if too wide
fn compute_layout(toolchains_len: usize, term_width: usize) -> (usize, usize, bool) {
    let mut agent_col = 16usize;
    let mut cell_col = 6usize;
    // Minimal spacing: one leading space between columns
    let row_width = agent_col.saturating_add(1)
        .saturating_add(toolchains_len.saturating_mul(cell_col.saturating_add(1)));
    if row_width > term_width || row_width > 100 {
        // Compress to single-letter/spinner-only cells
        cell_col = 2;
        (agent_col, cell_col, true)
    } else {
        (agent_col, cell_col, false)
    }
}

/// Fit a string to exactly width columns (truncate or pad with spaces)
fn fit(s: &str, width: usize) -> String {
    let mut out = String::new();
    let mut used = 0usize;
    for ch in s.chars() {
        if used + ch.len_utf8() > width {
            break;
        }
        out.push(ch);
        used += ch.len_utf8();
        if used >= width {
            break;
        }
    }
    while used < width {
        out.push(' ');
        used += 1;
    }
    out
}

/// Spinner frames for pending cells
fn pending_spinner_frames(ascii: bool) -> &'static [&'static str] {
    if ascii {
        &["-", "\\", "|", "/"]
    } else {
        &["⠋", "⠙", "⠸", "⠴", "⠦", "⠇"]
    }
}

/// Phase 2+3+4: scaffolding + lists/images/RNG seed + static layout & initial render.
/// - Detect docker path; on error, print a red line and return 1.
/// - Print header: version/host lines via banner; blank line; then "support matrix:".
/// - Build agents/toolchains via defaults and AIFO_SUPPORT_* overrides.
/// - Resolve images via default_image_for_quiet/default_toolchain_image.
/// - Initialize RNG seed from env or time; log effective seed when verbose.
/// - Compute widths and render initial matrix with PENDING cells (TTY only when animation enabled).
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

    // Phase 4: Static layout and initial render (TTY + animation enabled)
    let tty = atty::is(atty::Stream::Stderr);
    let animate_disabled = std::env::var("AIFO_SUPPORT_ANIMATE").ok().as_deref() == Some("0");
    let animate = tty && !animate_disabled;

    if animate {
        let term_width = terminal_width_or_default();
        let (agent_col, cell_col, _compressed) = compute_layout(toolchains.len(), term_width);
        let ascii = std::env::var("AIFO_SUPPORT_ASCII").ok().as_deref() == Some("1");
        let frames = pending_spinner_frames(ascii);
        let frame0 = frames[0];

        // Header row: toolchain names across columns (strong blue value color)
        let mut header_line = String::new();
        header_line.push_str(&" ".repeat(agent_col));
        for k in &toolchains {
            header_line.push(' ');
            let name = fit(k, cell_col);
            let painted = aifo_coder::paint(use_err, "\x1b[34;1m", &name);
            header_line.push_str(&painted);
        }
        eprintln!("{}", header_line);

        // Initial rows: each agent row with pending cells (dim gray)
        let pending_token = fit(frame0, cell_col);
        let pending_colored = aifo_coder::paint(use_err, "\x1b[90m", &pending_token);
        for a in &agents {
            let mut line = String::new();
            let label = fit(a, agent_col);
            line.push_str(&label);
            for _ in &toolchains {
                line.push(' ');
                line.push_str(&pending_colored);
            }
            eprintln!("{}", line);
        }
    }

    ExitCode::from(0)
}
