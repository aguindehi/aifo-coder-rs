# Specification: aifo-coder support command (coder/toolchain matrix)

Summary
- Add a new subcommand "support" that prints a colorized matrix (like doctor)
  indicating the e2e compatibility of each coder agent with each toolchain.
- Each cell reports PASS/WARN/FAIL using green/amber/red tokens.
- Checks are fast and non-destructive: agent CLI responds, toolchain package
  manager responds. No builds or installs are performed.

Scope and goals
- Coders (agents): aider, crush, codex, openhands, opencode, plandex.
- Toolchains: rust, node, typescript, python, c-cpp, go.
- Matrix cell status:
  - PASS: agent startup OK and toolchain PM OK.
  - WARN: only one of the two checks OK.
  - FAIL: both checks fail, runtime missing, or container run fails.
- Coloring matches doctor: green PASS, yellow WARN, red FAIL. Use color only
  when stderr is a TTY (aifo_coder::color_enabled_stderr + paint).

Runtime and images
- Detect docker via aifo_coder::container_runtime_path().
- Agent images: src/agent_images::default_image_for_quiet(agent).
- Toolchain images: aifo_coder::default_toolchain_image(kind).
- Honor flavor/prefix/tag overrides via existing env (AIFO_CODER_IMAGE_*).
- Do not pull unless required by docker run; optional NO_PULL behavior below.

Checks (commands executed inside containers)
- Agent startup:
  - aider: aider --version
  - crush: crush --version
  - codex: codex --version
  - openhands: openhands --version
  - opencode: opencode --version
  - plandex: plandex --version
- Toolchain package manager:
  - rust: cargo --version
  - node: npm --version
  - typescript: npm --version (relies on Node PM)
  - python: pip --version || python3 -m pip --version
  - c-cpp: gcc --version || cc --version || make --version
  - go: go version
- Use: docker run --rm --entrypoint sh <image> -lc '<cmd>'.

Configuration (environment)
- AIFO_SUPPORT_AGENTS: comma-separated list (default: all supported agents).
- AIFO_SUPPORT_TOOLCHAINS: comma-separated list (default: all supported kinds).
- AIFO_SUPPORT_NO_PULL=1: best-effort to avoid pulling (inspect first). If
  image is missing locally, mark FAIL and print hint when verbose.
- AIFO_SUPPORT_TIMEOUT_SECS: optional per-check soft timeout (default: none).
  Initial implementation omits host-level enforcement; all chosen commands are
  expected to return quickly. Timeout may be added later if needed.
- AIFO_TOOLCHAIN_VERBOSE: unchanged; not used by support for now.

Output formatting (doctor-like)
- Header: version/host, blank line, then "support matrix:".
- First row: toolchain names across columns.
- Each agent row: cells aligned; each cell shows PASS/WARN/FAIL token.
- Colors:
  - PASS: \x1b[32m PASS
  - WARN: \x1b[33m WARN
  - FAIL: \x1b[31m FAIL
- Use the same boundary/color policy as doctor (only color when tty).
- Keep lines within 100 chars; truncate columns gracefully when many toolchains.

Status rules and tips
- docker missing: print a prominent red message and render a single-line
  summary; exit nonzero (86 or 1). Matrix may be skipped in that case.
- Agent failure and PM success → WARN with a short hint when verbose.
- Agent success and PM failure → WARN with a short hint when verbose.
- Both failure → FAIL. If NO_PULL=1 and image missing locally, hint to pull or
  rerun without NO_PULL in verbose mode.
- When verbose:
  - Show the effective agent and toolchain images once per row/column section.
  - Show a compact reason string on failures (exit code, stderr size > 0).

Performance
- Default: sequential checks; keep commands minimal (version prints).
- Optional future: environment AIFO_SUPPORT_MAX_PAR=N to bound concurrency via
  a small thread pool. Initial implementation sticks to sequential for
  simplicity and predictability.

Exit codes
- docker missing → nonzero (1).
- Otherwise → 0, regardless of matrix contents, so CI can parse lines rather
  than fail fast. Future: optional --fail-on-red CLI flag if desired.

Security considerations
- No writes; only read-only "version" checks inside containers.
- Honor existing environment filtering; do not modify host state.
- No interactivity.

Implementation details

Data structures and helpers
- fn agents_default() -> Vec<&'static str>
- fn toolchains_default() -> Vec<&'static str>
- fn parse_csv_env(name: &str, default: Vec<&str>) -> Vec<String>
- fn color_token(use_color: bool, status: &str) -> String
  - "PASS"/"WARN"/"FAIL" with doctor-like coloring.
- fn run_version_check(rt: &Path, image: &str, cmd: &str, no_pull: bool)
  -> Result<(), String>
  - If no_pull: docker image inspect first; if not present, Err("not-present").
  - Run docker + sh -lc cmd; Ok on status.success(), else Err(reason).
- fn pm_cmd_for(kind: &str) -> String

Rendering and alignment
- Header row: pad labels to a fixed width (agent column ~16 chars). Each cell
  token rendered with padding space between columns.
- Detect tty via aifo_coder::color_enabled_stderr(); paint tokens only.

Verbose info
- When verbose, show:
  - Effective registry prefix (quiet): like doctor.
  - Effective images: agents and toolchains (blue value color).
  - Per-failure hints (not noisy in non-verbose mode).

Phased implementation plan

Phase 1: CLI and dispatch
- src/cli.rs: add Agent::Support (doc: "Run support matrix for coder/toolchains").
- src/main.rs:
  - mod support;
  - handle_misc_subcommands: match Agent::Support => Some(crate::support::run_support(cli.verbose)).
- No changes to existing commands module needed.

Phase 2: New module and scaffolding
- src/support.rs (new):
  - pub fn run_support(verbose: bool) -> std::process::ExitCode
  - Detect docker path; when missing, print a prominent red line and return 1.
  - Print header with version/host lines (like doctor style and spacing).

Phase 3: Image resolution and checks
- Build agent list via agents_default() and AIFO_SUPPORT_AGENTS.
- Build toolchain list via toolchains_default() and AIFO_SUPPORT_TOOLCHAINS.
- For each agent:
  - Resolve image via default_image_for_quiet(agent).
  - agent_ok = run_version_check(rt, image, "<cli> --version", no_pull).is_ok().
- For each toolchain:
  - Resolve image via default_toolchain_image(kind).
  - pm_ok = run_version_check(rt, image, pm_cmd_for(kind), no_pull).is_ok().

Phase 4: Matrix rendering
- Print a header row with toolchain names (blue value color, doctor-like).
- For each agent row, compute cell status:
  - PASS if agent_ok && pm_ok.
  - WARN if agent_ok ^ pm_ok.
  - FAIL otherwise.
- Colorize tokens with color_token(use_color, status).
- Keep alignment similar to doctor; avoid exceeding 100 chars.

Phase 5: Verbose diagnostics
- When verbose:
  - Print effective registry: prefix + source (doctor style).
  - On cell failures, print a compact hint (e.g., not-present, exit code).
  - If NO_PULL=1 and image missing locally, print a tip to pull or disable NO_PULL.

Phase 6: Tests and CI
- Add a smoke test guarded by docker availability:
  - If docker missing, ensure the command returns nonzero and prints a clear line.
  - If docker present, run with AIFO_SUPPORT_AGENTS=crush and
    AIFO_SUPPORT_TOOLCHAINS=node to keep fast and assert tokens exist.
- Use make check per CONVENTIONS.md.

Phase 7: Docs and UX
- Update README to mention "aifo-coder support".
- Document environment controls: AIFO_SUPPORT_*.
- Add a short example output snippet (matrix).

Phase 8: Release hygiene (AGENT.md guidance)
- Insert CHANGES.md entry (current date) summarizing the new command.
- Score the source code:
  - Move SCORE.md to SCORE-before.md (no -f) and write new SCORE.md with
    comprehensive scoring and proposed next steps.
- Ask: "Shall I proceed with these next steps?" after committing.

Future extensions (non-goals now)
- Per-cell timed runs with enforced host-level timeouts.
- Concurrency with a bounded thread pool.
- Optional CLI flags to select agents/toolchains (env-only for initial version).
- Status persistence and historical tracking.

Consistency validation and corrections
- The plan aligns with existing modules: uses default_image_for_quiet for agents,
  default_toolchain_image for toolchains, and doctor-style coloring rules via
  aifo_coder::color_enabled_stderr and paint.
- Typescript maps to Node package manager correctly; c-cpp uses a robust chain.
- NO_PULL handling requires a pre-run inspect; add "docker image inspect" check
  before running the container when AIFO_SUPPORT_NO_PULL=1.
- Exit code policy clarified: nonzero only when docker is missing; otherwise 0.
- Output length limits respected by short tokens and fixed label width.
