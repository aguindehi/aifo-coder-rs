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
- Completed (Phase 1): helpers extracted + repo-root helper + unreachable removed
  - Added apply_cli_globals, handle_fork_maintenance, handle_misc_subcommands,
    resolve_agent_and_args, require_repo_root in main.rs.
  - Landed in commit 5df8929.
- Completed (Phase 2): DRY toolchain planning
  - Added plan_from_cli in toolchain_session.rs; reused in main.rs dry-run.
  - Landed in commit 7dada1c.
- Completed (Phase 3): RAII cleanup for ToolchainSession
  - Added verbose/in_fork_pane fields and Drop-based cleanup; removed manual calls.
  - Landed in commits 9d63790 and 7376049.
- Completed (Phase 4): Centralized verbose diagnostics
  - Added print_verbose_run_info; replaced scattered prints.
  - Landed in commit 0b97d9c.
- In progress (Phase 5, optional): ergonomics and line-length tidy; see remaining work.

Overview of remaining work
- Phase 5 (optional, partially complete): Minor line-length tidy and ergonomics
  - Done: precompute and reuse stderr color flag in main.rs and src/fork/runner.rs.
  - Done: wrap long banner strings and a long dry-run string in main.rs.
  - Remaining (optional): wrap a few long literals in src/fork/runner.rs; run cargo clippy.
  - No functional changes.

Phase 5 — Minor ergonomics and line-length tidy (optional)
Goals
- Small correctness/readability improvements; adhere to 100-char lines.

Tasks
- Wrap long strings via format!/concat!/split calls where needed.
- Prefer computing and reusing local flags when they recur in a scope.

Acceptance criteria
- cargo build succeeds.
- No behavior/output changes; lines generally <=100 chars.

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
  - Already mitigated via RAII and scoping.

Validation checklist
- cargo build
- aifo-coder images > out.txt; out.txt has no banner; stderr shows banner.
- aifo-coder doctor; messages identical; banner on stderr.
- aifo-coder --fork 2 aider -- --help (quick spawn and close).
- aifo-coder --toolchain rust aider -- --version (dry-run on/off).
- Simulate docker not found; exit code is 127.
- Run with --verbose; content on stderr matches current output.

Rollout
- Land optional tidy as a separate commit for easy review/revert.

Revert strategy
- Changes are cosmetic only; revert single commit if needed.
