Phased refactor plan for src/main.rs and related modules (v2, compressed)

Scope and guarantees
- No behavior changes unless explicitly stated.
- Preserve stdout/stderr policy (banner and cosmetics on stderr).
- Keep exit codes and CLI semantics identical.
- Prefer <=100-char lines; allow longer only when unavoidable.
- No new dependencies.

Status
- Completed (Phase 0): stdout/stderr hygiene
  - Removed OutputNewlineGuard, moved banner and spacing to stderr.
  - Landed in commit a1839c5.

Overview of compressed phases
- Phase 1: Extract helpers and remove unreachable code (main.rs only).
- Phase 2: DRY toolchain planning shared with ToolchainSession.
- Phase 3: RAII cleanup for ToolchainSession.
- Phase 4: Centralize verbose diagnostics and tidy lines.

Phase 1 — Extract helpers + repo-root helper + remove unreachable
Goals
- Make main() a small orchestration of well-named helpers.
- Eliminate duplicate repo_root() error handling.
- Remove unreachable! arms for already-handled subcommands.

Tasks
- Add private helpers in src/main.rs:
  - fn apply_cli_globals(cli: &Cli)
    - Color, registry cache invalidation, flavor env.
  - fn handle_fork_maintenance(cli: &Cli) -> Option<ExitCode>
    - Early-return for ForkCmd::{List,Clean,Merge}.
  - fn handle_misc_subcommands(cli: &Cli) -> Option<ExitCode>
    - Early-return for Doctor, Images, CacheClear, Toolchain*, Toolchain.
  - fn resolve_agent_and_args(cli: &Cli)
      -> Option<(&'static str, Vec<String>)>
    - None if handled by prior helpers.
  - fn require_repo_root() -> Result<PathBuf, ExitCode>
    - Shared “must be run inside a Git repository.” path.
- Replace inline blocks in main() with calls to the helpers.
- Delete the final match unreachable! arms; use resolve_agent_and_args.

Acceptance criteria
- cargo build succeeds.
- Typical commands produce identical output and exit codes.
- No unreachable! remains for already-handled subcommands.

Files
- src/main.rs (helpers + call sites only)

Phase 2 — DRY toolchain planning (shared with ToolchainSession)
Goals
- A single source of truth for computing kinds and overrides.
- Remove duplicated parse_spec/normalize/overrides logic.

Tasks
- In src/toolchain_session.rs, add:
  - pub(crate) fn plan_from_cli(cli: &Cli)
      -> (Vec<String>, Vec<(String, String)>)
- Use plan_from_cli in:
  - ToolchainSession::start_if_requested (internal).
  - main.rs dry-run preview (no behavior change).
- Ensure version-derived default images match current behavior.

Acceptance criteria
- Dry-run shows the same “would attach/use image overrides” lines.
- ToolchainSession spawns identical sidecars/proxy as before.
- No duplicated parsing logic remains in main.rs.

Files
- src/toolchain_session.rs (new helper + internal reuse)
- src/main.rs (reuse planner for dry-run)

Phase 3 — RAII cleanup for ToolchainSession
Goals
- Ensure cleanup runs exactly once on all paths.
- Remove manual duplication of cleanup in main.rs.

Tasks
- Extend ToolchainSession with:
  - verbose: bool, in_fork_pane: bool (fields).
- Set fields in start_if_requested from cli.verbose and
  env AIFO_CODER_FORK_SESSION presence.
- Implement Drop for ToolchainSession that calls
  cleanup(self.verbose, self.in_fork_pane).
- In main.rs, remove explicit ts.cleanup calls; rely on Drop.

Acceptance criteria
- No double-cleanup; no leaks (proxy thread stops; network cleaned
  unless in a fork pane).
- Output and exit codes unchanged.

Files
- src/toolchain_session.rs (struct fields + Drop)
- src/main.rs (remove explicit cleanup calls)

Phase 4 — Centralize verbose diagnostics and tidy
Goals
- Keep main uncluttered and respect line-length guidance.

Tasks
- Add in src/main.rs:
  - fn print_verbose_run_info(agent: &str, image: &str,
                              apparmor_opt: Option<&str>,
                              preview: &str, cli_verbose: bool)
    - Prints registry, agent, image, profile, and docker preview.
- Replace scattered verbose prints with the helper.
- Compute in_fork_pane once and reuse.
- Wrap strings >100 chars via format!/concat! or split calls.

Acceptance criteria
- With --verbose, identical lines appear on stderr as before.
- Dry-run keeps printing the docker preview on stderr.
- Lines adhere to <=100 chars where practical.

Files
- src/main.rs
- src/fork/runner.rs (tidy only if needed; no stdout/stderr policy change)

Out of scope / Non-goals
- Changing fork orchestrators’ stdout/stderr behavior.
- Altering CLI flags, toolchain semantics, or registry logic.

Risks and mitigations
- Behavior drift from code motion:
  - Keep edits as pure moves and helper calls; compare strings manually.
- Drop timing differences:
  - Keep Option<ToolchainSession> scoped so Drop runs before exit.

Validation checklist
- cargo build
- aifo-coder images > out.txt; out.txt has no banner; stderr shows banner.
- aifo-coder doctor; messages identical; banner on stderr.
- aifo-coder --fork 2 aider -- --help (quick spawn and close).
- aifo-coder --toolchain rust aider -- --version (dry-run on/off).
- Simulate docker not found; exit code is 127.
- Run with --verbose; content on stderr matches current output.

Rollout
- Land Phase 1 -> 2 -> 3 in sequence; Phase 4 can be folded into 3
  if review stays clear. Each phase should be a separate commit.

Revert strategy
- Phases are isolated; revert any single commit if regressions appear.
