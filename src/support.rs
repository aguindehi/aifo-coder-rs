#![doc = r#"
Support matrix command: randomized, fast, non-blocking exploration with TTY-only animation.

Usage
- Run: aifo-coder support
- Animation is enabled only on TTY stderr; non-TTY prints a static matrix after checks complete.
- Order of checked cells is randomized with a seeded RNG; worker never sleeps.

Environment controls (AIFO_SUPPORT_*)
- AIFO_SUPPORT_AGENTS: CSV override of agents (default: aider,crush,codex,openhands,opencode,plandex)
- AIFO_SUPPORT_TOOLCHAINS: CSV override of toolchains (default: rust,node,typescript,python,c-cpp,go)
- AIFO_SUPPORT_NO_PULL=1: inspect image first; mark FAIL if not present locally (no pull).
- AIFO_SUPPORT_TIMEOUT_SECS: soft per-check timeout (default: none, best-effort).
- AIFO_SUPPORT_ANIMATE=0: disable animation even if TTY.
- AIFO_SUPPORT_ASCII=1: ASCII spinner frames (-\|/).
- AIFO_SUPPORT_ANIMATE_RATE_MS: spinner tick interval (default 80; clamp to [40, 250]).
- AIFO_SUPPORT_RAND_SEED: u64 seed for deterministic shuffle (printed in verbose mode).

Layout and tokens
- Agent column ~16 chars; cell ~6 chars; compresses to single-letter tokens on narrow terminals.
- Status tokens: PASS (green), WARN (yellow), FAIL (red), PENDING/spinner (dim gray).
"#]
use std::io::Write as _;
use std::process::ExitCode;
use std::time::{Duration, SystemTime};

use crate::banner::print_startup_banner;

struct CursorGuard {
    hide: bool,
}
impl CursorGuard {
    fn new(hide: bool) -> Self {
        if hide {
            eprint!("\x1b[?25l");
            let _ = std::io::stderr().flush();
        }
        CursorGuard { hide }
    }
}
impl Drop for CursorGuard {
    fn drop(&mut self) {
        if self.hide {
            eprint!("\x1b[?25h");
            let _ = std::io::stderr().flush();
        }
    }
}

/// Default agent list
fn agents_default() -> Vec<&'static str> {
    vec![
        "aider",
        "crush",
        "codex",
        "openhands",
        "opencode",
        "plandex",
    ]
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
    let agent_col = 16usize;
    let mut cell_col = 6usize;
    // Minimal spacing: one leading space between columns
    let row_width = agent_col
        .saturating_add(1)
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
    // Count character columns, not UTF-8 byte length, to keep Unicode spinners aligned.
    let mut out = String::new();
    let mut used = 0usize;
    for ch in s.chars() {
        if used + 1 > width {
            break;
        }
        out.push(ch);
        used += 1;
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

/// Capitalize a label for headers (TitleCase each hyphen-separated segment)
fn capitalize_label(s: &str) -> String {
    if s.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    for (i, part) in s.split('-').enumerate() {
        if i > 0 {
            out.push('-');
        }
        let mut chars = part.chars();
        if let Some(first) = chars.next() {
            out.push(first.to_ascii_uppercase());
            for c in chars {
                out.push(c.to_ascii_lowercase());
            }
        }
    }
    out
}

fn image_present(rt: &std::path::Path, image: &str) -> bool {
    use std::process::{Command, Stdio};
    Command::new(rt)
        .arg("image")
        .arg("inspect")
        .arg(image)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Spinner frames for pending cells
fn pending_spinner_frames(ascii: bool) -> &'static [&'static str] {
    if ascii {
        &["-", "\\", "|", "/"]
    } else {
        &["⠋", "⠙", "⠸", "⠴", "⠦", "⠇"]
    }
}

/// Support mode event channel messages
enum Event {
    AgentCached {
        agent: String,
        ok: bool,
        reason: Option<String>,
    },
    CellDone {
        agent: String,
        kind: String,
        status: String,
        reason: Option<String>,
    },
}

/// Map agent name to its absolute CLI path used for --version checks (inside container).
fn agent_cli_for(agent: &str) -> String {
    match agent {
        "aider" => "/opt/venv/bin/aider".to_string(),
        "codex" => "/usr/local/bin/codex".to_string(),
        "crush" => "/usr/local/bin/crush".to_string(),
        "openhands" => "/opt/venv-openhands/bin/openhands".to_string(),
        "opencode" => "/usr/local/bin/opencode".to_string(),
        "plandex" => "/usr/local/bin/plandex".to_string(),
        _ => agent.to_string(),
    }
}

/// PATH to use for agent probes, mirroring runtime PATH composition.
fn agent_path_for(agent: &str) -> &'static str {
    match agent {
        "aider" => "/opt/aifo/bin:/opt/venv/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:$PATH",
        "openhands" => "/opt/venv-openhands/bin:/opt/venv/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:/opt/aifo/bin:$PATH",
        "codex" | "crush" => "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:/opt/aifo/bin:$PATH",
        // Include /opt/aifo/bin for shims; otherwise a standard UNIX PATH
        _ => "/opt/aifo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:$PATH",
    }
}

/// Build a robust agent probe: export PATH, check absolute path and basename, tolerate odd --version exits.
/// OpenHands may be installed in different venv prefixes across images; try both.
fn agent_check_cmd(agent: &str) -> String {
    let pathv = agent_path_for(agent);
    match agent {
        "openhands" => {
            let a = "/opt/venv-openhands/bin/openhands";
            let b = "/opt/venv/bin/openhands";
            // 1) Prefer absolute a, then b; 2) fallback to basename on PATH; 3) Python package presence as offline fallback.
            format!(
                "export PATH=\"{pathv}\"; \
                 if [ -x {a} ]; then {a} --version >/dev/null 2>&1 || true; exit 0; \
                 elif [ -x {b} ]; then {b} --version >/dev/null 2>&1 || true; exit 0; \
                 elif command -v openhands >/dev/null 2>&1; then openhands --version >/dev/null 2>&1 || true; exit 0; \
                 elif command -v python3 >/dev/null 2>&1 && python3 -c \"import importlib.util, sys; sys.exit(0 if importlib.util.find_spec('openhands') else 1)\" >/dev/null 2>&1; then exit 0; \
                 else exit 1; fi"
            )
        }
        _ => {
            let abs = agent_cli_for(agent);
            let base = match agent {
                "aider" | "codex" | "crush" | "opencode" | "plandex" => agent,
                _ => abs.rsplit('/').next().unwrap_or(agent),
            };
            format!(
                "export PATH=\"{pathv}\"; \
                 if [ -x {abs} ]; then {abs} --version >/dev/null 2>&1 || true; exit 0; \
                 elif command -v {base} >/dev/null 2>&1; then {base} --version >/dev/null 2>&1 || true; exit 0; \
                 else exit 1; fi"
            )
        }
    }
}

/// Colorize a status token (TTY-only)
fn color_token(use_color: bool, status: &str) -> String {
    let key = status.trim();
    let code = match key {
        "PASS" | "G" => "\x1b[1;32m", // green
        "WARN" | "Y" => "\x1b[1;33m", // yellow
        "FAIL" | "R" => "\x1b[1;31m", // red
        "NA" | "N" => "\x1b[90m",     // dim gray
        _ => "",
    };
    if code.is_empty() {
        status.to_string()
    } else {
        aifo_coder::paint(use_color, code, status)
    }
}

/// Repaint only the affected agent row using ANSI cursor movement when available.
fn repaint_row(row_idx: usize, line: &str, use_ansi: bool, total_rows: usize) {
    if use_ansi {
        // Baseline is the line after the summary. Row i is at offset (total_rows + 2 - i) lines up.
        let up = total_rows.saturating_add(2).saturating_sub(row_idx);
        eprint!("\x1b[{}A", up);
        eprint!("\r{}\x1b[K", line);
        eprint!("\x1b[{}B", up);
        let _ = std::io::stderr().flush();
    } else {
        eprintln!("{}", line);
    }
}

/// Repaint the summary line (relative to the baseline: one line above).
fn repaint_summary(
    pass: usize,
    warn: usize,
    fail: usize,
    na: usize,
    use_ansi: bool,
    use_color: bool,
) {
    let pass_tok = color_token(use_color, "PASS");
    let warn_tok = color_token(use_color, "WARN");
    let fail_tok = color_token(use_color, "FAIL");
    let na_tok = color_token(use_color, "NA");
    let line = format!(
        "Summary: {}={} {}={} {}={} {}={}",
        pass_tok, pass, warn_tok, warn, fail_tok, fail, na_tok, na
    );
    if use_ansi {
        // Baseline is the line after the summary; summary line is 1 up from baseline.
        eprint!("\x1b[1A");
        eprint!("\r{}\x1b[K", line);
        eprint!("\x1b[1B");
        let _ = std::io::stderr().flush();
    } else {
        eprintln!("{}", line);
    }
}

/// Render a single agent row given current statuses and spinner state (TTY-aware colors).
#[allow(clippy::too_many_arguments)]
fn render_row_line(
    agents: &[String],
    toolchains: &[String],
    statuses: &[Vec<Option<String>>],
    ai: usize,
    _spin_cell: Option<usize>,
    agent_col: usize,
    cell_col: usize,
    compressed: bool,
    frames: &[&str],
    spinner_idx: usize,
    use_err: bool,
    skip_green: bool,
) -> String {
    let mut line = String::new();
    let cap = capitalize_label(&agents[ai]);
    let label_raw = fit(&cap, agent_col);
    let label = aifo_coder::paint(use_err, "\x1b[34;1m", &label_raw);
    line.push_str(&label);
    for (ki, _k) in toolchains.iter().enumerate() {
        line.push(' ');
        match &statuses[ai][ki] {
            Some(st) => {
                let is_pass = st == "PASS";
                if skip_green && is_pass {
                    let tok = fit("", cell_col);
                    line.push_str(&tok);
                } else {
                    let src = if compressed {
                        match st.as_str() {
                            "PASS" => "G",
                            "WARN" => "Y",
                            "FAIL" => "R",
                            "NA" => "N",
                            _ => st.as_str(),
                        }
                    } else {
                        st.as_str()
                    };
                    let tok = fit(src, cell_col);
                    line.push_str(&color_token(use_err, &tok));
                }
            }
            None => {
                let frame = frames[spinner_idx % frames.len()];
                let tok = fit(frame, cell_col);
                line.push_str(&aifo_coder::paint(use_err, "\x1b[90m", &tok));
            }
        }
    }
    line
}

/// Minimal deterministic RNG (xorshift64*) for seeded shuffle
struct XorShift64 {
    state: u64,
}
impl XorShift64 {
    fn new(seed: u64) -> Self {
        let s = if seed == 0 { 0x9e3779b97f4a7c15 } else { seed };
        Self { state: s }
    }
    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }
    fn next_usize(&mut self, bound: usize) -> usize {
        if bound <= 1 {
            0
        } else {
            (self.next_u64() as usize) % bound
        }
    }
}

/// Shuffle a vector of (row,col) pairs using Fisher–Yates with the seeded RNG
fn shuffle_pairs(pairs: &mut [(usize, usize)], seed: u64) {
    let mut rng = XorShift64::new(seed);
    let n = pairs.len();
    for i in (1..n).rev() {
        let j = rng.next_usize(i + 1);
        pairs.swap(i, j);
    }
}

/// PM command mapping for toolchain kinds (offline, deterministic)
fn pm_cmd_for(kind: &str) -> String {
    match kind {
        "rust" => "rustc --version".to_string(),
        // Some distros expose node as nodejs
        "node" => "node --version || nodejs --version".to_string(),
        // TypeScript viability: tsc OR tsserver OR (node AND any package manager)
        "typescript" => r#"(tsc --version >/dev/null 2>&1) || (tsserver --version >/dev/null 2>&1) || ((node --version >/dev/null 2>&1 || nodejs --version >/dev/null 2>&1) && (npm -v >/dev/null 2>&1 || corepack -v >/dev/null 2>&1 || pnpm -v >/dev/null 2>&1 || yarn -v >/dev/null 2>&1))"#.to_string(),
        // Some images only have python; accept either
        "python" => "python3 --version || python --version".to_string(),
        // Accept gcc or clang or cc or make present in the image.
        "c-cpp" => "gcc --version || clang --version || cc --version || make --version".to_string(),
        "go" => "go version".to_string(),
        _ => "true".to_string(),
    }
}

/// Run a version check inside an image; honor NO_PULL inspect first.
fn run_version_check(
    rt: &std::path::Path,
    image: &str,
    cmd: &str,
    no_pull: bool,
) -> Result<(), String> {
    use std::process::{Command, Stdio};
    if no_pull {
        let ok = Command::new(rt)
            .arg("image")
            .arg("inspect")
            .arg(image)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !ok {
            return Err("not-present".to_string());
        }
    }
    let mut child = Command::new(rt)
        .arg("run")
        .arg("--rm")
        .arg("--entrypoint")
        .arg("/bin/sh")
        .arg(image)
        .arg("-c")
        .arg(cmd)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    let st = child.wait().map_err(|e| e.to_string())?;
    if st.success() {
        Ok(())
    } else {
        Err(format!("exit={}", st.code().unwrap_or(1)))
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
    let runtime = match aifo_coder::container_runtime_path() {
        Ok(p) => p,
        Err(e) => {
            let use_err = aifo_coder::color_enabled_stderr();
            aifo_coder::log_error_stderr(use_err, &format!("aifo-coder: {}", e));
            return ExitCode::from(1);
        }
    };

    // Header line for the matrix
    eprintln!();
    let use_err = aifo_coder::color_enabled_stderr();
    aifo_coder::log_info_stderr(use_err, "Support matrix:");
    eprintln!();

    // Phase 3: lists, images and RNG
    let agents = parse_csv_env("AIFO_SUPPORT_AGENTS", agents_default());
    let toolchains = parse_csv_env("AIFO_SUPPORT_TOOLCHAINS", toolchains_default());

    // Resolve images
    let agent_images: Vec<String> = agents
        .iter()
        .map(|a| crate::agent_images::default_image_for_quiet(a))
        .collect();
    let toolchain_images: Vec<String> = toolchains
        .iter()
        .map(|k| aifo_coder::default_toolchain_image(k))
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
    let skip_green = std::env::var("AIFO_SUPPORT_SKIP_GREEN").ok().as_deref() == Some("1");
    let _cursor_guard = if animate {
        Some(CursorGuard::new(true))
    } else {
        None
    };
    let mut spinner_idx = 0usize;
    let term_width = terminal_width_or_default();
    let (agent_col, cell_col, compressed) = compute_layout(toolchains.len(), term_width);
    let ascii_env = std::env::var("AIFO_SUPPORT_ASCII").ok().as_deref() == Some("1");
    let frames = pending_spinner_frames(ascii_env || compressed);

    // Matrix state
    let total_rows = agents.len();
    let mut statuses: Vec<Vec<Option<String>>> = vec![vec![None; toolchains.len()]; total_rows];
    // Live summary counters (updated as cells finish)
    let mut pass_count: usize = 0;
    let mut warn_count: usize = 0;
    let mut fail_count: usize = 0;
    let mut na_count: usize = 0;

    if animate {
        // Draw header + initial rows
        let mut header_line = String::new();
        header_line.push_str(&" ".repeat(agent_col));
        for k in &toolchains {
            header_line.push(' ');
            let cap = capitalize_label(k);
            let name = fit(&cap, cell_col);
            let painted = aifo_coder::paint(use_err, "\x1b[34;1m", &name);
            header_line.push_str(&painted);
        }
        eprintln!("{}", header_line);
        // Empty line between column headers and the matrix
        eprintln!();

        // Initial rows: pending tokens in dim gray
        let pending_token0 = aifo_coder::paint(use_err, "\x1b[90m", &fit(frames[0], cell_col));
        for a in &agents {
            let mut line = String::new();
            let cap = capitalize_label(a);
            let label_raw = fit(&cap, agent_col);
            let label = aifo_coder::paint(use_err, "\x1b[34;1m", &label_raw);
            line.push_str(&label);
            for _ in &toolchains {
                line.push(' ');
                line.push_str(&pending_token0);
            }
            eprintln!("{}", line);
        }

        // Anchor saved earlier above the first row.

        // Spacer blank line between matrix rows and the summary
        eprintln!();
        // Print initial summary one line below the anchor
        let pass_tok0 = color_token(use_err, "PASS");
        let warn_tok0 = color_token(use_err, "WARN");
        let fail_tok0 = color_token(use_err, "FAIL");
        let na_tok0 = color_token(use_err, "NA");
        let init_summary = format!(
            "Summary: {}={} {}={} {}={} {}={}",
            pass_tok0, pass_count, warn_tok0, warn_count, fail_tok0, fail_count, na_tok0, 0
        );
        eprintln!("{}", init_summary);
    }

    // Phase 5: Worker/painter channel
    let no_pull = std::env::var("AIFO_SUPPORT_NO_PULL").ok().as_deref() == Some("1");
    let tick_ms: u64 = std::env::var("AIFO_SUPPORT_ANIMATE_RATE_MS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .map(|v| v.clamp(40, 250))
        .unwrap_or(80);

    // Build shuffled worklist of (agent_idx, kind_idx)
    let mut worklist: Vec<(usize, usize)> = Vec::new();
    for (ai, _) in agents.iter().enumerate() {
        for (ki, _) in toolchains.iter().enumerate() {
            worklist.push((ai, ki));
        }
    }
    shuffle_pairs(worklist.as_mut_slice(), seed);

    // Active pending set
    let mut pending: std::collections::HashSet<(usize, usize)> = worklist.iter().copied().collect();

    // Event channel
    let (tx, rx) = std::sync::mpsc::channel::<Event>();

    // Worker thread: cache agent_ok once per agent; run PM check; never sleeps.
    {
        let agents_cl = agents.clone();
        let kinds_cl = toolchains.clone();
        let agent_imgs = agent_images.clone();
        let tc_imgs = toolchain_images.clone();
        let rt = runtime.clone();
        let tx_cl = tx.clone();
        std::thread::spawn(move || {
            let mut agent_ok: Vec<Option<bool>> = vec![None; agents_cl.len()];
            for (ai, ki) in worklist.into_iter() {
                let agent_img = &agent_imgs[ai];
                let tc_img = &tc_imgs[ki];

                // If either required image is missing locally, mark N/A and skip checks.
                if !image_present(&rt, agent_img) || !image_present(&rt, tc_img) {
                    let _ = tx_cl.send(Event::CellDone {
                        agent: agents_cl[ai].clone(),
                        kind: kinds_cl[ki].clone(),
                        status: "NA".to_string(),
                        reason: Some("not-present".to_string()),
                    });
                    continue;
                }

                if agent_ok[ai].is_none() {
                    let cmd = agent_check_cmd(&agents_cl[ai]);
                    let res = run_version_check(&rt, agent_img, &cmd, no_pull);
                    let ok = res.is_ok();
                    agent_ok[ai] = Some(ok);
                    let _ = tx_cl.send(Event::AgentCached {
                        agent: agents_cl[ai].clone(),
                        ok,
                        reason: res.err(),
                    });
                }
                let cmd = pm_cmd_for(&kinds_cl[ki]);
                let pm_res = run_version_check(&rt, tc_img, &cmd, no_pull);
                let pm_ok = pm_res.is_ok();
                let aok = agent_ok[ai].unwrap_or(false);
                let status = if aok && pm_ok {
                    "PASS"
                } else if aok || pm_ok {
                    "WARN"
                } else {
                    "FAIL"
                }
                .to_string();
                let _ = tx_cl.send(Event::CellDone {
                    agent: agents_cl[ai].clone(),
                    kind: kinds_cl[ki].clone(),
                    status,
                    reason: pm_res.err(),
                });
            }
        });
    }

    // Name→index maps
    let mut agent_index: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    let mut kind_index: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for (i, a) in agents.iter().enumerate() {
        agent_index.insert(a.clone(), i);
    }
    for (i, k) in toolchains.iter().enumerate() {
        kind_index.insert(k.clone(), i);
    }

    // Row render helper moved to render_row_line() (module-level function)

    // Painter/consumer loop
    if animate {
        // TTY animation path
        let use_ansi = animate;
        while !pending.is_empty() {
            match rx.recv_timeout(std::time::Duration::from_millis(tick_ms)) {
                Ok(Event::AgentCached { .. }) => {
                    // Optional: could annotate rows in verbose mode; keep minimal for v3.
                }
                Ok(Event::CellDone {
                    agent,
                    kind,
                    status,
                    ..
                }) => {
                    let ai = *agent_index.get(&agent).unwrap_or(&0);
                    let ki = *kind_index.get(&kind).unwrap_or(&0);
                    statuses[ai][ki] = Some(status.clone());
                    pending.remove(&(ai, ki));

                    // Increment live summary counters
                    match status.as_str() {
                        "PASS" => pass_count = pass_count.saturating_add(1),
                        "WARN" => warn_count = warn_count.saturating_add(1),
                        "FAIL" => fail_count = fail_count.saturating_add(1),
                        "NA" => na_count = na_count.saturating_add(1),
                        _ => {}
                    }

                    let line = render_row_line(
                        &agents,
                        &toolchains,
                        &statuses,
                        ai,
                        None,
                        agent_col,
                        cell_col,
                        compressed,
                        frames,
                        spinner_idx,
                        use_err,
                        skip_green,
                    );
                    repaint_row(ai, &line, use_ansi, total_rows);

                    // No per-cell active selection; animate all pending rows each tick.

                    // Repaint summary after each completed cell
                    repaint_summary(
                        pass_count, warn_count, fail_count, na_count, use_ansi, use_err,
                    );
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    // Advance spinner and repaint all rows that still have pending cells
                    spinner_idx = (spinner_idx + 1) % frames.len();
                    for ai in 0..total_rows {
                        if statuses[ai].iter().any(|c| c.is_none()) {
                            let line = render_row_line(
                                &agents,
                                &toolchains,
                                &statuses,
                                ai,
                                None,
                                agent_col,
                                cell_col,
                                compressed,
                                frames,
                                spinner_idx,
                                use_err,
                                skip_green,
                            );
                            repaint_row(ai, &line, use_ansi, total_rows);
                        }
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    break;
                }
            }
        }
    } else {
        // Non-TTY or animation disabled: consume events, then render a static matrix.
        let mut remaining = pending.len();
        let mut agent_diag: std::collections::HashMap<String, (bool, Option<String>)> =
            std::collections::HashMap::new();
        let mut pm_diag: std::collections::HashMap<(String, String), Option<String>> =
            std::collections::HashMap::new();
        let mut seen_agent_progress: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        let use_err2 = aifo_coder::color_enabled_stderr();
        while remaining > 0 {
            match rx.recv() {
                Ok(Event::AgentCached { agent, ok, reason }) => {
                    if verbose && seen_agent_progress.insert(agent.clone()) {
                        aifo_coder::log_info_stderr(use_err2, &format!("checking {} ...", agent));
                    }
                    agent_diag.insert(agent, (ok, reason));
                }
                Ok(Event::CellDone {
                    agent,
                    kind,
                    status,
                    reason,
                    ..
                }) => {
                    if verbose && seen_agent_progress.insert(agent.clone()) {
                        aifo_coder::log_info_stderr(use_err2, &format!("checking {} ...", agent));
                    }
                    let ai = *agent_index.get(&agent).unwrap_or(&0);
                    let ki = *kind_index.get(&kind).unwrap_or(&0);
                    statuses[ai][ki] = Some(status);
                    pm_diag.insert((agent.clone(), kind.clone()), reason);
                    if pending.remove(&(ai, ki)) {
                        remaining = remaining.saturating_sub(1);
                    }
                }
                Err(_) => break,
            }
        }

        // Render header and final matrix (no spinner)
        let mut header_line = String::new();
        header_line.push_str(&" ".repeat(agent_col));
        for k in &toolchains {
            header_line.push(' ');
            let cap = capitalize_label(k);
            let name = fit(&cap, cell_col);
            let painted = aifo_coder::paint(false, "\x1b[34;1m", &name);
            header_line.push_str(&painted);
        }
        eprintln!("{}", header_line);
        // Empty line between column headers and the matrix
        eprintln!();
        for (ai, a) in agents.iter().enumerate() {
            let mut line = String::new();
            let cap = capitalize_label(a);
            let label_raw = fit(&cap, agent_col);
            // Non-TTY/static path: also paint row headers in bold blue for consistency
            let label = aifo_coder::paint(use_err2, "\x1b[34;1m", &label_raw);
            line.push_str(&label);
            for (ki, _k) in toolchains.iter().enumerate() {
                line.push(' ');
                let raw = statuses[ai][ki].as_deref().unwrap_or("FAIL");
                let disp = if compressed {
                    match raw {
                        "PASS" => "G",
                        "WARN" => "Y",
                        "FAIL" => "R",
                        "NA" => "N",
                        _ => raw,
                    }
                } else {
                    raw
                };
                let tokf = if skip_green && raw == "PASS" {
                    fit("", cell_col)
                } else {
                    fit(disp, cell_col)
                };
                // Use color when enabled, even in non-animated mode
                line.push_str(&color_token(use_err2, &tokf));
            }
            eprintln!("{}", line);
        }

        // Verbose hints per agent with WARN/FAIL
        if verbose {
            let use_err2 = aifo_coder::color_enabled_stderr();
            for (ai, a) in agents.iter().enumerate() {
                let mut bad: Vec<String> = Vec::new();
                for (ki, k) in toolchains.iter().enumerate() {
                    match statuses[ai][ki].as_deref() {
                        Some("PASS") => {}
                        Some(_) | None => {
                            let r = pm_diag
                                .get(&(a.clone(), k.clone()))
                                .and_then(|o| o.clone())
                                .unwrap_or_else(|| "err".to_string());
                            bad.push(format!("{}={}", k, r));
                        }
                    }
                }
                if !bad.is_empty() {
                    let (aok, areason) = agent_diag.get(a).cloned().unwrap_or((false, None));
                    let mut agent_part = format!("agent={}", if aok { "ok" } else { "fail" });
                    if !aok {
                        if let Some(r) = areason {
                            if !r.is_empty() {
                                agent_part.push_str(&format!("({})", r));
                            }
                        }
                    }
                    if bad.len() > 4 {
                        bad.truncate(4);
                        bad.push("...".to_string());
                    }
                    let msg = format!("{}: {}; pm {}", a, agent_part, bad.join(", "));
                    aifo_coder::log_info_stderr(use_err2, &msg);
                }
            }
        }
    }

    // Final summary
    let mut pass = 0usize;
    let mut warn = 0usize;
    let mut fail = 0usize;
    let mut na = 0usize;
    for row in &statuses {
        for cell in row {
            match cell.as_deref() {
                Some("PASS") => pass += 1,
                Some("WARN") => warn += 1,
                Some("FAIL") => fail += 1,
                Some("NA") => na += 1,
                _ => {}
            }
        }
    }
    let use_err = aifo_coder::color_enabled_stderr();
    if animate {
        // In TTY/animate mode, repaint the live summary line in-place (no extra lines).
        repaint_summary(pass, warn, fail, na, true, use_err);
        // We are at the baseline (line after summary); add one blank line.
        eprintln!();
        // Legend (TTY)
        let legend = format!(
            "Legend: {} ok, {} partial, {} unavailable, {} not-available (image missing)",
            color_token(use_err, "PASS"),
            color_token(use_err, "WARN"),
            color_token(use_err, "FAIL"),
            color_token(use_err, "NA"),
        );
        eprintln!("{}", legend);
        let _ = std::io::stderr().flush();
    } else {
        // Non-TTY/static: add a separating blank line and print a final colored summary line.
        eprintln!();
        let pass_tok = color_token(use_err, "PASS");
        let warn_tok = color_token(use_err, "WARN");
        let fail_tok = color_token(use_err, "FAIL");
        let na_tok = color_token(use_err, "NA");
        let summary = format!(
            "Summary: {}={} {}={} {}={} {}={}",
            pass_tok, pass, warn_tok, warn, fail_tok, fail, na_tok, na
        );
        aifo_coder::log_info_stderr(use_err, &summary);
        // Legend (non-TTY)
        let legend = format!(
            "Legend: {} ok, {} partial, {} unavailable, {} not-available (image missing)",
            pass_tok, warn_tok, fail_tok, na_tok
        );
        eprintln!("{}", legend);
    }
    if verbose {
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
        aifo_coder::log_info_stderr(use_err, &format!("internal registry: {}", ir_display));
        aifo_coder::log_info_stderr(use_err, &format!("mirror registry: {}", mr_display));
    }

    ExitCode::from(0)
}
