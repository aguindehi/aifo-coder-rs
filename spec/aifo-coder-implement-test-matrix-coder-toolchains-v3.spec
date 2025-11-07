# Specification: aifo-coder support command (coder/toolchain matrix) v3
# Fast, non-blocking randomized cell exploration with smooth animation.

Summary
- Render a colorized support matrix without delaying overall exploration for animation.
- Iterate as fast as possible through all coder/toolchain pairs; animation is purely cosmetic.
- Randomize tested cells so updates are scattered across the matrix for a pleasant look.
- Animate only on TTY stderr; non-TTY renders a static matrix after checks complete.

Key changes from v2
- Randomized worklist: build a flat list of (agent, toolchain) pairs and shuffle it. This avoids
  long runs in a single row and maintains scattered visual updates.
- Worker/painter split: decouple checking from rendering via a channel. The worker runs checks
  back-to-back. The painter advances spinners only on timeout ticks and never blocks the worker.
- Agent check caching: agent CLI --version executes at most once per agent and is cached. Each
  cell combines cached agent_ok with pm_ok to compute PASS/WARN/FAIL, eliminating N× agent checks.
- Immediate cell repaint: finalize and repaint the affected row as soon as a cell completes; do not
  wait for an entire row or any spinner cycle.
- Tightened tick policy: animation advances only at the configured cadence. No extra sleeps are
  introduced anywhere that would delay matrix exploration.

Scope and goals
- Agents: aider, crush, codex, openhands, opencode, plandex.
- Toolchains: rust, node, typescript, python, c-cpp, go.
- Checks: agent startup (CLI --version) and toolchain PM (--version or equivalent).
- Status tokens:
  - PASS: agent OK and PM OK.
  - WARN: exactly one OK.
  - FAIL: both fail or runtime/image errors.
- Doctor-style color rules and alignment; smooth scattered updates when animation is enabled.

Runtime and images
- Detect docker via aifo_coder::container_runtime_path().
- Agent images: src/agent_images::default_image_for_quiet(agent).
- Toolchain images: aifo_coder::default_toolchain_image(kind).
- Honor prefix/tag/flavor env (AIFO_CODER_IMAGE_*). No pulling unless “docker run” requires.
- NO_PULL policy handled via “image inspect” before runs.

Fast randomized iteration design
- Worklist:
  - Build Vec<(agent, kind)> across all agents and toolchains.
  - Shuffle with a seeded RNG from AIFO_SUPPORT_RAND_SEED (u64). If unset, derive a seed from time.
  - When verbose, print the effective seed once.
- Agent --version cache:
  - HashMap<String, bool> populated lazily. First cell for an agent triggers a single agent check.
  - When verbose, optionally annotate agent rows with a small “(agent ok)” or “(agent fail)” suffix.
- Worker/painter split:
  - Worker thread:
    - For each (agent, kind) in shuffled order:
      - Ensure agent_ok is cached; if computed, send AgentCached event.
      - Run PM check for (agent, kind). Send CellDone with pm_ok and derived PASS/WARN/FAIL.
    - Never sleeps; runs checks back-to-back respecting optional per-check timeouts.
  - Main thread (TTY):
    - Draw header and initial matrix with PENDING cells.
    - Maintain one “active pending” cell for spinner; choose it at random from still-pending cells.
    - recv_timeout(tick_ms):
      - On timeout: advance spinner on the active cell and repaint only that row.
      - On event: finalize that cell, repaint that row, pick a new active pending cell at random.
    - If no pending cells remain, stop animating and print summary.
  - Main thread (non-TTY):
    - Disable animation/colors. Consume events and update states. Print static matrix at the end.

Animation behavior
- Spinner frames:
  - Unicode default: “⠋⠙⠸⠴⠦⠇”.
  - ASCII fallback: “-\\|/” when AIFO_SUPPORT_ASCII=1.
  - Color: dim gray \x1b[90m for spinner/pending; reset with \x1b[0m.
- Row repaint:
  - Repaint only the affected row per event using ANSI cursor movement and clear-to-EOL when TTY.
  - Fallback to plain reprint when ANSI is unavailable.
- Column compression:
  - Agent column ~16 chars; cell col ~6 chars (token plus padding).
  - When width constrained or many toolchains, compress tokens to single-letter colored forms
    (“G/Y/R”) and elide padding.

Non-TTY fallback
- Disable animation/color; run checks and print a static matrix after completion.
- When verbose, add a short progress line per row (bounded).

Environment controls
- AIFO_SUPPORT_AGENTS: CSV to override agent list (default: all).
- AIFO_SUPPORT_TOOLCHAINS: CSV to override toolchain kinds (default: all).
- AIFO_SUPPORT_NO_PULL=1: inspect image first; if missing locally, mark FAIL and hint when verbose.
- AIFO_SUPPORT_TIMEOUT_SECS: soft per-check timeout (default: none). No host enforcement in v3;
  commands are expected to be quick.
- AIFO_SUPPORT_ANIMATE=0: disable animation (even if TTY).
- AIFO_SUPPORT_ASCII=1: force ASCII spinner frames.
- AIFO_SUPPORT_ANIMATE_RATE_MS: spinner tick interval; default 80; clamp to [40, 250].
- AIFO_SUPPORT_MAX_PAR: reserved for future concurrency; v3 stays single-threaded worker.
- AIFO_TOOLCHAIN_VERBOSE: unchanged; support does not use it.
- AIFO_SUPPORT_RAND_SEED: u64 seed for deterministic shuffle; printed in verbose mode.

Output formatting (doctor-like)
- Header: version/host lines, blank line, then “support matrix:”.
- First row: toolchain names across columns (strong blue value color).
- One row per agent:
  - Agent label column ~16 chars (ellipsis when needed).
  - Cells aligned with 1-space padding; spinner during pending; final tokens on completion.
- Colors (TTY-only):
  - PASS: \x1b[32m PASS
  - WARN: \x1b[33m WARN
  - FAIL: \x1b[31m FAIL
  - PENDING/spinner: \x1b[90m
- Line length target ≤100 chars; compress columns when needed.

Status rules and verbose tips
- docker missing: print a prominent red line and exit nonzero (1). Skip the matrix; provide a short
  summary line instead.
- WARN when exactly one of agent_ok or pm_ok succeeds; FAIL when both fail.
- NO_PULL: when image missing locally, mark FAIL. In verbose mode, hint to pull or rerun without
  NO_PULL.
- Verbose hints on failures: compact reason (exit code or “stderr>0”) based on docker run status.

Performance
- Worker thread runs checks with zero artificial sleeps; only per-check timeout applies if set.
- Painter uses recv_timeout for spinner ticks and does not delay worker processing.
- Randomized selection keeps animation lively while preserving fast overall completion.

Security considerations
- No writes; run only --version commands inside containers.
- Honor environment filtering; no host state modifications; no interactivity.

Implementation details

Data structures and helpers
- fn agents_default() -> Vec<&'static str>
- fn toolchains_default() -> Vec<&'static str>
- fn parse_csv_env(name: &str, default: Vec<&str>) -> Vec<String>
- fn pm_cmd_for(kind: &str) -> String
  - rust → "rustc --version"
  - node → "node --version"
  - typescript → "npx tsc --version || true" (uses node image; avoids hard failure)
  - python → "python3 --version"
  - c-cpp → "gcc --version || cc --version || make --version"
  - go → "go version"
- fn run_version_check(rt: &Path, image: &str, cmd: &str, no_pull: bool)
  -> Result<(), String>
  - If no_pull: "docker image inspect" first; Err("not-present") if missing locally.
  - Spawn "docker run --rm --entrypoint sh <image> -lc '<cmd>'" and poll child.try_wait().
  - Ok if exit status success; Err with compact reason otherwise.
- fn pending_spinner_frames(ascii: bool) -> &'static [&'static str]
- fn color_token(use_color: bool, status: &str) -> String
- fn repaint_row(row_idx: usize, line: &str, use_ansi: bool)
- fn agent_cli_for(agent: &str) -> &'static str
  - aider/crush/codex/openhands/opencode/plandex → "<cli> --version".
- enum Event {
    AgentCached { agent: String, ok: bool },
    CellDone { agent: String, kind: String, pm_ok: bool, status: String },
  }

Fast randomized execution
- Build agents/toolchains from env/defaults.
- Resolve images via default_image_for_quiet and default_toolchain_image.
- Construct worklist Vec<(agent, kind)>; shuffle with RNG; record seed when verbose.
- Spawn worker thread:
  - Maintain HashMap<String, Option<bool>> agent_ok, initially None.
  - For each (agent, kind):
    - If agent_ok[agent] is None: run agent --version, send AgentCached, cache result.
    - Run pm check; determine PASS/WARN/FAIL using cached agent_ok; send CellDone.
- Main thread (TTY, animate):
  - Print header and initial matrix with all PENDING cells; track row offsets and ANSI availability.
  - Maintain matrix cell states; set of pending cells; one active pending cell for spinner.
  - Loop while pending exists:
    - On recv_timeout(tick_ms): advance spinner for active pending cell and repaint that row.
    - On event: apply change, repaint that row, switch active pending to a random still-pending cell.
- Main thread (non-TTY or animation disabled):
  - Consume events and update matrix without spinner. Print static matrix and summary at the end.

Diagnostics and summary
- After finishing all rows:
  - Verbose: print effective registry prefix (blue), and compact failure hints per agent row
    (bounded to 1–2 lines to avoid noise).
  - Summary line: totals PASS/WARN/FAIL.
- Exit codes:
  - docker missing → 1.
  - Otherwise → 0 (CI parses lines). Future option: --fail-on-red.

Tests and CI
- docker missing: ensure the command returns nonzero and prints a clear red line.
- docker present: run with
  AIFO_SUPPORT_AGENTS=crush AIFO_SUPPORT_TOOLCHAINS=node AIFO_SUPPORT_ANIMATE=0
  to avoid TTY-only animation; assert tokens contain PASS|WARN|FAIL.
- Deterministic order test: set AIFO_SUPPORT_RAND_SEED=1 and validate the first few CellDone
  events match the expected sequence (via verbose logging hook).
- Agent check count: ensure agent CLI invocations equal number of agents.
- Use “make check” per CONVENTIONS.md.

Phased implementation plan

Phase 1: CLI wiring
- src/cli.rs: add Agent::Support ("Run support matrix for coder/toolchains").
- src/main.rs: mod support; handle_misc_subcommands adds
  Agent::Support => Some(crate::support::run_support(cli.verbose)).

Phase 2: Module scaffolding
- src/support.rs (new):
  - pub fn run_support(verbose: bool) -> std::process::ExitCode
  - Detect docker path; on error, print a red line and return 1.
  - Print header: version/host lines; blank line; "support matrix:".

Phase 3: Lists, images and RNG
- Build agents/toolchains via defaults and AIFO_SUPPORT_* CSV overrides.
- Resolve images via default_image_for_quiet/default_toolchain_image.
- Initialize RNG with AIFO_SUPPORT_RAND_SEED or time-derived seed; log seed when verbose.

Phase 4: Static layout and initial render
- Compute widths from terminal or defaults (agent col ~16, cell col ~6).
- Print header row and initial rows with PENDING cells (frame 0).
- Record cursor row offsets for repaint; detect TTY and ANSI capability; enable color when TTY.

Phase 5: Worker/painter channel
- Define Event enum and use std::sync::mpsc channel.
- Worker:
  - Cache agent_ok lazily; send AgentCached when first computed for an agent.
  - Run pm checks per shuffled worklist; send CellDone with final status.
- Painter:
  - Maintain matrix state and pending set; single active pending cell for spinner.
  - Loop: recv_timeout(tick_ms) to animate; on events, repaint affected row and switch active cell.

Phase 6: Non-TTY fallback and verbose hints
- If not TTY or animation disabled:
  - Run worker to completion; apply events; render static matrix and summary.
  - Emit concise verbose hints per agent on WARN/FAIL (exit codes, stderr>0, or not-present).

Phase 7: Tests and CI
- Add smoke tests for docker missing case, PASS/WARN/FAIL tokens, and deterministic shuffle via
  AIFO_SUPPORT_RAND_SEED.
- Ensure agent --version calls are at most once per agent.

Phase 8: Docs and UX
- Update README to mention "aifo-coder support", randomized animation, and TTY-only behavior.
- Document environment controls under AIFO_SUPPORT_* including RAND_SEED and ANIMATE_RATE_MS.
- Include an output snippet (matrix) and note token compression on narrow terminals.

Phase 9: Release hygiene (AGENT.md guidance)
- Insert CHANGES.md entry (current date) summarizing the fast randomized support mode.
- Move SCORE.md to SCORE-before.md; write new SCORE.md with grades and next steps.
- Ask: "Shall I proceed with these next steps?" after committing.

Consistency validation and corrections (vs v2)
- Replaced per-row sequential scanning with a single shuffled worklist to keep animation scattered.
- Ensured the worker never sleeps; spinner tick only via painter recv_timeout.
- Added agent_ok caching to eliminate repeated agent --version runs.
- Clarified pm_cmd_for mappings, including typescript via npx and python3 preference.
- Kept exit code and color policies consistent with existing modules and doctor-like output.
