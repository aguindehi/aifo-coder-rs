AIFO Coder main.rs refactoring and cleanup v4 — Orchestrator extraction, runner module, and duplication removal

Summary
- Goal: Reduce src/main.rs to a thin, testable entrypoint while preserving all observable behavior (CLI, outputs, exit codes, environment semantics, colorization, stdout/stderr ordering).
- Approach: Move fork_run into src/fork/runner.rs; implement Windows orchestrators in src/fork/orchestrators/*; reuse existing helpers (inner builders, wt_* helpers, ps_wait_process_cmd, meta writers, preflights, summary). Optionally extract the agent Docker path into commands::agent.rs.
- Constraints: No public CLI changes; no changes to aifo_coder library APIs; no functional drift in fork/merge flows or Docker command shapes. Preserve exact text, ordering, and exit codes.

Behavioral invariants (must remain exactly)
- CLI flags, aliases, defaults, and environment semantics unchanged.
- Log text and ordering remain 1:1:
  - Startup banner and summaries on stdout.
  - Warnings/notes/merge progress and errors on stderr; use eprintln where main.rs currently does.
- Exit codes unchanged for all success/error paths (including 127 for NotFound cases).
- Dry-run behavior unchanged (previews printed, execution skipped).
- Color/TTY semantics via aifo_coder::color_* helpers unchanged; banner color unchanged.
- Environment variables read/set and their timings remain identical (e.g., AIFO_CODER_SKIP_LOCK, AIFO_CODER_FORK_SESSION, AIFO_TOOLEEXEC_*).
- Metadata JSON files contain the same keys/values; preserve key order to minimize diffs where tests compare textually.

Key corrections and refactor directives vs v3
1) Windows orchestrator preference env precedence (same as v3)
   - Honor AIFO_CODER_FORK_ORCH before probing Windows Terminal:
     - AIFO_CODER_FORK_ORCH=gitbash → use Git Bash or mintty if present; else error with current exact text.
     - AIFO_CODER_FORK_ORCH=powershell → prefer PowerShell if available.
     - Else: prefer wt.exe; if post-merge requested and wt selected, automatically fall back to PowerShell and emit the same warning text as today.
   - Keep all message strings and exit codes verbatim.

2) Library inner builders + required extra env (reuse)
   - Reuse aifo_coder::fork_ps_inner_string and fork_bash_inner_string.
   - Inject AIFO_CODER_SUPPRESS_TOOLCHAIN_WARNING=1 without changing lib APIs:
     - PowerShell: insert "$env:AIFO_CODER_SUPPRESS_TOOLCHAIN_WARNING='1';" immediately after the Set-Location prefix (first “; ”).
     - Git Bash: inject "export AIFO_CODER_SUPPRESS_TOOLCHAIN_WARNING=1; " immediately before "aifo-coder …".
   - Git Bash “exec bash” tail:
     - Keep tail only when no post-merge is requested; trim trailing "; exec bash" when a post-merge strategy is requested.

3) wt helpers argv[0] (unchanged)
   - wt_build_new_tab_args and wt_build_split_args include "wt" as argv[0]. When executing via Command::new(wt_path), drop the first element; keep the full vector for preview strings to remain identical to current output.

4) Base commit SHA for metadata (unchanged)
   - Use the exact current algorithm when writing initial meta:
     - If snapshot created: base_commit_sha = snapshot sha.
     - Else: try "git rev-parse --verify <base_ref_or_sha>" → if success, use that sha.
     - Else: fall back to the HEAD SHA from fork_base_info().

5) Metadata updates (unchanged)
   - update_panes_created must read current .meta.json, update panes_created and two arrays (pane_dirs, branches) from on-disk existing panes, and preserve all other keys (including optional snapshot_sha) and key order. Do not re-derive base_* fields here.

6) Guidance call patterns (unchanged)
   - Keep print_inspect_merge_guidance flags exact per orchestrator:
     - Git Bash enforced: include_remote_examples=true, extra_spacing_before_wrapper=false.
     - Windows Terminal: include_remote_examples=false, extra_spacing_before_wrapper=true.
     - PowerShell: include_remote_examples=false, extra_spacing_before_wrapper=true.
     - Tmux: include_remote_examples=false, extra_spacing_before_wrapper=true; with colored header based on stdout TTY.

7) Duplicate removal and helper usage (new in v4)
   - Move fork_run from main.rs to src/fork/runner.rs.
   - Windows: move all launch logic into src/fork/orchestrators/{windows_terminal,powershell,gitbash_mintty}.rs.
   - Unify Git Bash/mintty launch loops; reuse inner::build_inner_gitbash and session::make_session/make_pane everywhere.
   - Unify Windows Terminal flows into “waitable” (strict) and “non-waitable best-effort” functions; share preview/execute helpers and orientation.
   - Replace ad-hoc WT orientation closures with aifo_coder::wt_orient_for_layout(layout, i).
   - Replace ad-hoc PowerShell wait string with aifo_coder::ps_wait_process_cmd(&pids).
   - Centralize post-merge via fork::post_merge::apply_post_merge in all orchestrator paths (remove any inline merge logs).

8) Banner and stdout/stderr (unchanged)
   - Keep banner prints exactly as today (stdout).
   - Continue using aifo_coder::paint and color_enabled_* only where current code already does (stderr/info flows).

9) Exact messages and exit codes (unchanged)
   - Preserve all preflight and error strings verbatim (e.g., tmux not found message, Windows orchestrator not found messages, manual post-merge guidance lines).
   - Return ExitCode::from(127) for NotFound paths; otherwise unchanged codes.

Target bin crate module layout (v4)
- src/cli.rs: unchanged.
- src/banner.rs: unchanged.
- src/agent_images.rs: unchanged.
- src/fork/mod.rs
  - pub mod runner (new file: src/fork/runner.rs)
  - pub mod types (existing)
  - pub mod env (existing)
  - pub mod inner (existing; keep SUPPRESS injection and Git Bash tail trimming)
  - pub mod meta (existing)
  - pub mod preflight (existing)
  - pub mod session (existing; use make_session/make_pane throughout; optional small helpers like pane_state_dir)
  - pub mod orchestrators
    - mod.rs (selection logic with tests; reuse have()/have_any() testing shims)
    - windows_terminal.rs (implement waitable and best-effort)
    - powershell.rs (implement Start-Process launcher and wait)
    - gitbash_mintty.rs (implement git-bash.exe and mintty.exe launchers)
  - pub mod summary (existing; header + per-pane outputs)
  - pub mod post_merge (existing; apply_post_merge)

- src/fork_args.rs: unchanged (existing child args builder; keep unit tests where they are today).
- src/commands/mod.rs: unchanged.
- src/commands/agent.rs (optional, new): move the Docker agent run path from main.rs for readability; preserve behavior.

Streams and color
- stdout: banner and informational summaries (as today).
- stderr: warnings/notes/progress and errors (as today).
- Use aifo_coder::paint and color_enabled_* where current code already does; do not change banner color behavior.

Platform gating and portability
- cfg(not(windows)) for tmux orchestrator.
- cfg(windows) for Windows orchestrators and helpers.
- Avoid compiling Windows-only code on Unix and vice versa. Orchestrators mod should compile cross-platform by gating individual variants.

Error handling boundaries
- Internal helpers return Result<T, String/io::Error> as appropriate.
- Convert to ExitCode only in the thin command handlers and main entrypoint/runner.
- Preserve 127 for NotFound paths and all current exit codes.

Testing strategy
- Keep existing library tests unchanged; ensure cargo test passes on supported platforms.
- Maintain/select orchestrator unit tests under src/fork/orchestrators/mod.rs (already in place).
- Ensure inner tests validate SUPPRESS injection and Git Bash tail trimming (already present in src/fork/inner.rs).
- Metadata tests remain in src/fork/meta.rs (already present).
- Add or confirm tests:
  - PowerShell wait command uses aifo_coder::ps_wait_process_cmd (unit test under powershell.rs).
  - WT preview args retain argv[0] for preview but drop argv[0] on execution (unit tests under windows_terminal.rs, guarded by cfg(windows)).
  - runner::fork_run remains a thin dispatcher; any new unit tests should mock select_orchestrator behavior where feasible.

Risk mitigation
- Metadata drift: continue writing JSON manually in meta helpers to preserve key order; existing tests enforce order.
- Windows orchestrator fallthrough: select_orchestrator mirrors current fallbacks and emits exact messages; tests already cover presence/absence permutations.
- stdout/stderr ordering: keep banner/warnings/notes on the same streams; use eprintln where the original does.
- TTY/color behavior: do not alter banner color; use aifo_coder::color_* where current code already does.
- Library reuse: avoid duplicating builders; only apply minimal string augmentation for SUPPRESS var and Git Bash tail.

Compact phased plan (ready for implementation)
Phase 0 — Baseline (already done)
- Inventory user-visible strings and verify helpers (docs/phase0-*.md).

Phase 1 — Runner extraction and session helpers
- Add src/fork/runner.rs and move fork_run from main.rs verbatim.
- Replace call site in main.rs with crate::fork::runner::fork_run(&cli, n).
- Ensure session::make_session/make_pane are used instead of ad-hoc struct literals in runner; keep tmux path intact.

Phase 2 — Windows orchestrators implementation and selection
- Implement src/fork/orchestrators/{windows_terminal,powershell,gitbash_mintty}.rs by moving code from runner (former main.rs) into these modules.
- Use inner builders (build_inner_powershell/build_inner_gitbash); use wt_orient_for_layout; use ps_wait_process_cmd; keep previews identical; drop argv[0] when executing wt.
- Wire runner::fork_run to select_orchestrator(cli, layout) and dispatch to appropriate launcher.
- Keep exact messages and exit codes; reuse cleanup::cleanup_and_update_meta on errors.

Phase 3 — Post-merge unification and duplication cleanup
- Ensure all orchestrator paths call post_merge::apply_post_merge with correct “plain” flag; remove any inline merge logic.
- Unify Git Bash/mintty loops into their respective modules and remove duplicate loops from runner.

Phase 4 — Optional: Extract Docker agent run path
- Add src/commands/agent.rs; move the Docker build/exec logic from main.rs with lock handling and ToolchainSession RAII cleanup.
- Replace block in main.rs with commands::run_agent(&cli, agent, args). Preserve exact messages and exit codes.

Phase 5 — Tests and polish
- Ensure orchestrators/mod.rs selection tests pass on Windows (with test shims).
- Confirm inner/meta tests pass.
- Add small unit tests in orchestrator modules as needed for preview/argv0 and wait script.
- Run all tests and verify no string drift (textual comparisons remain identical).

Acceptance criteria
- cargo test passes locally on supported platforms (including integration tests under tests/).
- Manual smoke tests behave identically (outputs, exit codes):
  - aifo-coder aider --dry-run --verbose
  - aifo-coder --fork 2 aider -- --help
  - aifo-coder images
  - aifo-coder toolchain --help
  - Doctor and Fork maintenance subcommands unchanged.
- Fork metadata files contain the same keys/values as before; order preserved.
- Orchestrator selection, warnings, and guidance match current behavior exactly (Windows and Unix-like).
- src/main.rs reduced to a thin dispatcher: CLI parse, color/env setup, early fork maintenance handlers, fork_run dispatch, doctor/images/cache/toolchain handlers, agent run dispatch.
