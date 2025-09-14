Phased refactor plan for src/main.rs and related modules

Scope
- Keep behavior and user-visible messages identical unless stated.
- Preserve exit codes and existing CLI semantics.
- Maintain color and line-length conventions (<=100 chars preferred).
- Avoid new dependencies.

Completed (Phase 0) — stdout/stderr hygiene
- Remove OutputNewlineGuard and the implicit trailing stdout newline.
- Move banner and cosmetic spacing to stderr.
- Ensure previews/diagnostics print to stderr.
- Landed in commit a1839c5.

Phase 1 — Extract small helpers from main.rs (no behavior change)
Goals
- Make main() a short orchestration that delegates to helpers.
- Improve readability and testability without moving logic across crates.

Tasks
- Add private helpers in src/main.rs:
  - fn apply_cli_globals(cli: &Cli)
  - fn handle_fork_maintenance(cli: &Cli) -> Option<ExitCode>
  - fn handle_misc_subcommands(cli: &Cli) -> Option<ExitCode>
  - fn resolve_agent_and_args(cli: &Cli) -> Option<(&'static str, Vec<String>)>
  - fn setup_toolchains(cli: &Cli) -> io::Result<Option<ToolchainSession>>
  - fn run_agent(cli: &Cli, agent: &str, args: &[String],
                 ts: Option<ToolchainSession>) -> ExitCode
- Replace inline blocks with calls to these helpers.

Acceptance criteria
- cargo build succeeds.
- cargo run with typical commands yields identical output and exit codes.
- main.rs shrinks and contains no unreachable! arms for already-handled subcommands.

Files
- src/main.rs (helpers + call sites only)

Phase 2 — DRY toolchain planning (shared between main and ToolchainSession)
Goals
- Single source of truth for toolchain kinds/overrides planning.
- Eliminate duplicate parse_spec/normalize logic.

Tasks
- In src/toolchain_session.rs, add:
  - pub(crate) fn plan_from_cli(cli: &Cli)
      -> (Vec<String>, Vec<(String, String)>)
    - Returns (kinds, overrides) where overrides include spec-derived images.
- Refactor ToolchainSession::start_if_requested to use plan_from_cli.
- Refactor main.rs dry-run preview to call plan_from_cli.

Acceptance criteria
- Dry-run prints the same “would attach/use image overrides” info as before.
- start_if_requested starts identical sidecars/proxy as before.
- No duplicated parse_spec/normalize blocks remain.

Files
- src/toolchain_session.rs (new helper + internal use)
- src/main.rs (reuse planner for dry-run)

Phase 3 — RAII cleanup for ToolchainSession
Goals
- Ensure cleanup runs exactly once in success and error paths.
- Reduce manual cleanup duplication in main.rs.

Tasks
- Add fields to ToolchainSession: verbose: bool, in_fork_pane: bool.
- Set them in start_if_requested from cli.verbose and env
  AIFO_CODER_FORK_SESSION presence.
- Implement Drop for ToolchainSession calling cleanup(self.verbose,
  self.in_fork_pane).
- In main.rs, remove explicit ts.cleanup calls; rely on drop semantics.

Acceptance criteria
- No double-cleanup; no resource leaks (proxy thread stops, network
  cleaned unless in fork pane).
- Behavior (output, exit codes) unchanged.

Files
- src/toolchain_session.rs (struct fields + Drop)
- src/main.rs (remove explicit cleanup calls)

Phase 4 — Repo-root helper for fork maintenance branches
Goals
- Remove repetition and guarantee identical error text.

Tasks
- Add private fn require_repo_root() -> Result<PathBuf, ExitCode> in main.rs.
- Use it in ForkCmd::List (non all-repos), ::Clean, ::Merge branches.

Acceptance criteria
- Same error string “must be run inside a Git repository.” printed.
- No duplication of aifo_coder::repo_root() match blocks.

Files
- src/main.rs

Phase 5 — Simplify agent resolution and remove unreachable! arms
Goals
- Avoid unreachable! arms for already-handled subcommands.
- Keep agent/args resolution clear and robust to new variants.

Tasks
- Implement resolve_agent_and_args(cli) returning Option<(&str, Vec<String>)>.
- Use it to choose the agent path; None indicates earlier branch handled.

Acceptance criteria
- No unreachable! remains for handled subcommands.
- Agent and args passed through unchanged.

Files
- src/main.rs

Phase 6 — Centralize verbose diagnostics
Goals
- Keep main uncluttered; make it easier to evolve diagnostics.

Tasks
- Add private fn print_verbose_run_info(agent, image, apparmor_opt, preview,
  cli_verbose) that prints all verbose lines (registry, agent, image, profile,
  and docker preview).
- Call it from main.rs where appropriate.

Acceptance criteria
- When --verbose is set, identical lines appear on stderr.
- No change in dry-run behavior: preview still printed in dry-run.

Files
- src/main.rs

Phase 7 — Minor ergonomics and line-length tidy
Goals
- Small correctness/readability improvements; adhere to 100-char lines.

Tasks
- Compute in_fork_pane once and reuse.
- Wrap lines >100 chars in format!/concat! blocks where necessary.
- No functional changes.

Files
- src/main.rs
- src/fork/runner.rs (only if needed for tidy; no stdout/stderr policy change)

Out of scope / Non-goals
- Changing fork orchestration output streams in src/fork/runner.rs.
- Altering CLI flags or semantics.
- Changing image selection logic or registry behavior.

Risks and mitigations
- Risk: Behavior drift from code motion.
  - Mitigation: Pure moves, helper calls, and planner reuse with string
    comparisons in manual tests.
- Risk: Drop-based cleanup timing differences.
  - Mitigation: Keep Option<ToolchainSession> in narrow scope so Drop runs
    right after docker status collection and before ExitCode return.

Validation checklist
- cargo build
- aifo-coder images > out.txt; out.txt has no banner; stderr shows banner.
- aifo-coder doctor; messages identical, banner on stderr.
- aifo-coder --fork 2 aider -- --help (quick spawn and manual close).
- aifo-coder --toolchain rust aider -- --version (with and without dry-run).
- Simulate docker not found; exit code is 127.
- Run with --verbose and confirm identical content on stderr.

Rollout
- Land each phase as a separate commit for easier review and bisect.
- Prefer Phase 1 -> 2 -> 3 order; remaining phases can be grouped.

Revert strategy
- Each phase is isolated; revert single commit if regressions appear.
