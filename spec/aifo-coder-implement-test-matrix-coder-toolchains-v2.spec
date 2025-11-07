# Specification: aifo-coder support command (coder/toolchain matrix) v2
# Adds animated, doctor-like colorized matrix rendering while checks run.

Summary
- Implement the "support" subcommand to animate a colorized matrix (doctor style)
  showing compatibility of each coder agent with each toolchain while checks
  execute. Each cell transitions from a pending spinner to PASS/WARN/FAIL.
- Animation is enabled only on TTY stderr; non-TTY environments render a static
  matrix after checks complete. Behavior remains fast and non-destructive.

Scope and goals
- Coders (agents): aider, crush, codex, openhands, opencode, plandex.
- Toolchains: rust, node, typescript, python, c-cpp, go.
- Checks: agent startup (CLI --version) and toolchain PM (--version).
- Status tokens:
  - PASS: agent OK and PM OK.
  - WARN: exactly one OK.
  - FAIL: both fail or runtime/image errors.
  - Doctor-style color rules and alignment; smooth animated updates at randomized
    positions in the matrix when possible.

Runtime and images
- Detect docker via aifo_coder::container_runtime_path().
- Agent images: src/agent_images::default_image_for_quiet(agent).
- Toolchain images: aifo_coder::default_toolchain_image(kind).
- Honor flavor/prefix/tag env (AIFO_CODER_IMAGE_*). No pulling unless "docker
  run" requires; optional NO_PULL policy handled via image inspect.

Animation design
- Enable animation when stderr is a TTY and AIFO_SUPPORT_ANIMATE != "0".
- Each cell starts as PENDING and renders a spinner. On completion, it
  transitions to PASS/WARN/FAIL with color. Spinner frames:
  - Unicode: "⠋⠙⠸⠴⠦⠇" (default), ASCII fallback: "-\\|/" when
    AIFO_SUPPORT_ASCII=1.
- Per-cell animation strategy (sequential checks):
  - Spawn the docker "version" command and poll child.try_wait() at an interval
    AIFO_SUPPORT_ANIMATE_RATE_MS (default 80 ms). On each tick, update the
    cell with the next spinner frame.
  - When the process exits, compute PASS/WARN/FAIL and repaint the cell.
- Row repaint approach:
  - Print header and initial rows with pending cells.
  - Repaint only the current agent row when a cell changes (ANSI cursor move
    up/down + clear-to-EOL). Fallback: reprint the row without cursor moves
    when ANSI is unavailable.
- Non-TTY fallback:
  - Disable animation/color; run checks and print a static matrix. Add a short
    progress line per row when verbose.

Configuration (environment)
- AIFO_SUPPORT_AGENTS: CSV to override agent list (default: all).
- AIFO_SUPPORT_TOOLCHAINS: CSV to override toolchain kinds (default: all).
- AIFO_SUPPORT_NO_PULL=1: inspect first; if missing locally, mark FAIL and log
  a hint when verbose.
- AIFO_SUPPORT_TIMEOUT_SECS: soft per-check timeout (default: none). Initial
  implementation omits host enforcement; commands are expected to be quick.
- AIFO_SUPPORT_ANIMATE=0: disable animation (even if TTY).
- AIFO_SUPPORT_ASCII=1: force ASCII spinner frames.
- AIFO_SUPPORT_ANIMATE_RATE_MS: spinner tick interval; default 80 (bounded to
  [40, 250]).
- AIFO_SUPPORT_MAX_PAR: reserved for future concurrency; v2 stays sequential.
- AIFO_TOOLCHAIN_VERBOSE: unchanged; support does not use it.

Output formatting (doctor-like)
- Header: version/host lines, blank line, then "support matrix:".
- First row: toolchain names across columns (strong blue value color).
- One row per agent:
  - Agent label column ~16 chars (truncated with ellipsis if needed).
  - Cells aligned with 1 space padding; spinner during pending; token after.
- Colors (TTY-only):
  - PASS: \x1b[32m PASS
  - WARN: \x1b[33m WARN
  - FAIL: \x1b[31m FAIL
  - PENDING: dim gray " … " or spinner (use \x1b[90m for pending/spinner).
- Line length: target ≤100 chars. When many toolchains, truncate columns:
  - Agent column fixed width; cells may elide leading space or compress token
    to a single-colored letter ("G"/"Y"/"R") when needed and verbose is off.

Status rules and tips
- docker missing: print a prominent red line and exit nonzero (1). The matrix
  may be skipped; provide a single-line summary instead.
- Agent failure and PM success → WARN; PM failure and agent success → WARN.
- Both failure → FAIL. With NO_PULL=1 and image missing locally, hint to pull
  or rerun without NO_PULL (verbose only).
- Verbose hints on failures: show compact reason (exit code, stderr size > 0)
  based on docker output status. Avoid noisy logs.

Performance
- Sequential checks; short version commands only.
- Animation tick updates a single row at a time; minimal repaint to avoid
  flicker. Rate-limit ticks via AIFO_SUPPORT_ANIMATE_RATE_MS.
- Future: AIFO_SUPPORT_MAX_PAR=N to bound concurrency with a small thread pool.

Exit codes
- docker missing → 1.
- Otherwise → 0, regardless of matrix content (so CI can parse lines).
- Future option: --fail-on-red to exit nonzero on any FAIL.

Security considerations
- No writes; run only --version commands inside containers.
- Honor environment filtering; do not modify host state; no interactivity.

Implementation details

Data structures and helpers
- fn agents_default() -> Vec<&'static str>
- fn toolchains_default() -> Vec<&'static str>
- fn parse_csv_env(name: &str, default: Vec<&str>) -> Vec<String>
- fn color_token(use_color: bool, status: &str) -> String
  - "PASS"/"WARN"/"FAIL" with doctor-style coloring; PENDING spinner dim gray.
- fn pm_cmd_for(kind: &str) -> String
- fn run_version_check(rt: &Path, image: &str, cmd: &str, no_pull: bool)
  -> Result<(), String>
  - If no_pull: "docker image inspect" first; Err("not-present") if missing.
  - Spawn "docker run --rm --entrypoint sh <image> -lc '<cmd>'" and poll with
    child.try_wait() to support animation.
- fn repaint_row(row_idx: usize, line: &str, use_ansi: bool)
  - Move cursor up/down and clear-to-EOL when ANSI; otherwise print the line.
- fn pending_spinner_frames(ascii: bool) -> &'static [&'static str]
- fn agent_cli_for(agent: &str) -> &'static str
  - aider/crush/codex/openhands/opencode/plandex → "<cli> --version".

Rendering and alignment
- Compute column widths from terminal size or defaults:
  - Agent col: 16 chars; cell col: 6 chars (token plus padding).
  - Clamp total width; when exceeded, compress tokens to single-letter colored
    form (G/Y/R) and drop padding spaces.
- Detect tty via aifo_coder::color_enabled_stderr(); color only when tty.
- Spinner: dim gray token " ⠋ " or ASCII. On each tick, rebuild the row and
  repaint only that row. Preserve alignment.

Verbose info
- At the top (once): effective registry prefix (quiet), like doctor.
- Per agent row:
  - Before checks, print the resolved agent image (blue value color).
  - On WARN/FAIL cells, print compact reason lines below the matrix when
    verbose (bounded to 1–2 lines per agent to avoid noise).
- With NO_PULL=1 and missing image: concise tip to pull or disable NO_PULL.

Phased implementation plan

Phase 1: CLI wiring
- src/cli.rs: add Agent::Support (doc: "Run support matrix for coder/toolchains").
- src/main.rs: mod support; handle_misc_subcommands adds
  Agent::Support => Some(crate::support::run_support(cli.verbose)).

Phase 2: Module scaffolding
- src/support.rs (new):
  - pub fn run_support(verbose: bool) -> std::process::ExitCode
  - Detect docker path; on error, print a red line and return 1.
  - Print header: version/host lines; blank line; "support matrix:".

Phase 3: Lists and image resolution
- Build agents list via agents_default() and AIFO_SUPPORT_AGENTS.
- Build toolchains list via toolchains_default() and AIFO_SUPPORT_TOOLCHAINS.
- Resolve images via default_image_for_quiet(agent) and default_toolchain_image(kind).

Phase 4: Static matrix layout
- Compute agent/toolchain label widths; render header row with toolchain names.
- Render initial rows with PENDING cells (spinner frame 0). Record the starting
  cursor position and row offsets for repaint.

Phase 5: Animated checks (sequential)
- For each agent:
  - agent_ok = run_version_check(rt, agent_image, "<cli> --version", no_pull).is_ok().
  - Repaint agent row with updated spinner or final token for the agent column
    (this column is conceptual; for the matrix, the first toolchain cell will
    get the spinner while the agent check runs, then final tokens per cell).
  - For each toolchain:
    - pm_ok = run_version_check(rt, toolchain_image, pm_cmd_for(kind), no_pull).is_ok().
    - Determine cell status and repaint the row to reflect the cell change.
    - When verbose, capture concise reasons for WARN/FAIL for later printing.
- Tick loop:
  - child.try_wait() with sleep of AIFO_SUPPORT_ANIMATE_RATE_MS per tick.
  - On each tick, advance spinner frame and repaint the row.

Phase 6: Verbose diagnostics and summary
- After finishing all rows, if verbose:
  - Print effective registry prefix (blue value color).
  - Print compact per-failure hints (e.g., not-present, exit code).
- Print a short summary line: totals PASS/WARN/FAIL.

Phase 7: Tests (smoke) and CI
- If docker missing: ensure the command returns nonzero and prints a clear line.
- If docker present: run with
  AIFO_SUPPORT_AGENTS=crush AIFO_SUPPORT_TOOLCHAINS=node AIFO_SUPPORT_ANIMATE=0
  to avoid TTY-only animation in CI; assert tokens contain PASS|WARN|FAIL.
- Use make check per CONVENTIONS.md.

Phase 8: Docs and UX
- Update README to mention "aifo-coder support" and animation behavior.
- Document environment controls under AIFO_SUPPORT_*.
- Add an output snippet (matrix) and note TTY-only animation.

Phase 9: Release hygiene (AGENT.md guidance)
- Insert CHANGES.md entry (current date) summarizing the new command.
- Move SCORE.md to SCORE-before.md (no -f) and write new SCORE.md with
  comprehensive scoring and next steps.
- Ask: "Shall I proceed with these next steps?" after committing.

Consistency validation and corrections
- Matches doctor color policy via color_enabled_stderr() and paint().
- Uses default_image_for_quiet for agents and default_toolchain_image for
  toolchains; honors prefix/tag/flavor env.
- Typescript maps to Node PM correctly; c-cpp tries gcc/cc/make robustly.
- NO_PULL handling: inspect first; mark missing as FAIL with verbose tip.
- Exit code policy unchanged; matrix length within 100 chars by compressing
  tokens when many toolchains or narrow terminals.

Future extensions (non-goals now)
- Concurrency with a bounded thread pool via AIFO_SUPPORT_MAX_PAR=N.
- Enforced host-level timeouts per cell.
- Historical run persistence and trend rendering.
