AIFO Coder main.rs refactoring and cleanup v2 — Consolidated, library-aware plan

Context
- The current repository already exposes substantial helper functionality via the aifo_coder library (src/lib.rs and submodules: apparmor, color, docker, fork, lock, registry, toolchain, util).
- The bin entrypoint (src/main.rs) remains monolithic and still reimplements orchestration paths (tmux and multiple Windows launchers), metadata JSON assembly, toolchain lifecycle handling, banner printing, and CLI definitions.
- Some helpers already exist in the library that v1 intended to build anew in the bin crate (e.g., Windows inner command builders, orientation helpers). We should reuse these instead of duplicating them.
- There are existing unit tests in the library for fork flows, sidecar previews, and helpers, plus a child-args test currently embedded in main.rs.

Primary objectives (unchanged from v1, clarified)
- Reduce src/main.rs to a clear entrypoint that wires CLI parsing to cohesive internal modules.
- Eliminate duplicated orchestration logic across Windows variants and tmux; introduce a small Orchestrator trait with concrete implementations that reuse existing aifo_coder helpers where available.
- Centralize metadata writing via helpers that use serde_json, ensuring the JSON keys/shape remain identical to current hand-built strings.
- Improve testability by extracting pure builders and moving the child-args test into a dedicated module.
- Preserve behavior 1:1: CLI, outputs, exit codes, environment semantics, colorization, and stdout/stderr ordering.

Non-goals (v2)
- No changes to public CLI flags, default values, or environment variable semantics.
- No changes to aifo_coder library APIs or behavior; only read/consume existing functions. Do not move current lib functionality back into bin.
- No functional changes to fork/merge flows or Docker command shapes.

Behavioral invariants (must keep exactly)
- Log text and ordering remain the same (banner, notes, warnings, previews). Use eprintln where the current code does.
- Exit codes remain unchanged for all success/error paths (including 127 for NotFound cases).
- Dry-run behavior unchanged; still prints previews and skips execution.
- Environment variable semantics unchanged; same reads/sets for all flows (e.g., AIFO_CODER_SKIP_LOCK, AIFO_CODER_FORK_SESSION, AIFO_TOOLEEXEC_*).
- Metadata files contain the same keys and values where deterministically comparable. Key ordering differences are acceptable only if tests don’t depend on textual order (we will preserve order to minimize diffs).
- atty/TTY detection and color behavior via aifo_coder::color_* helpers remain unchanged.

Gaps and inconsistencies in v1, with corrections
1) Location of helpers vs library overlap
   - v1 proposed new bin modules for inner PowerShell/Git Bash builders and wt helpers. The library already provides:
     - fork_ps_inner_string, fork_bash_inner_string
     - wt_orient_for_layout, wt_build_new_tab_args, wt_build_split_args, ps_wait_process_cmd
   - v2: Reuse these library helpers directly from orchestrators; do not reintroduce duplicate builders in the bin crate.

2) Metadata writers placement and shape
   - v1 placed meta helpers in bin (src/fork/meta.rs). The library’s fork_merge_* also writes meta directly today, and tests assert on keys.
   - v2: Add bin-side meta helpers for the bin’s own fork session lifecycle (initial write and “panes_created” updates) using serde_json::json!. Keep the exact key set currently emitted by main.rs skeleton:
     created_at, base_label, base_ref_or_sha, base_commit_sha, panes, panes_created (Option), pane_dirs, branches, layout, snapshot_sha (Option)
   - Do not alter library code that writes meta; the bin helpers must match existing JSON shape to minimize diffs and keep tests passing.

3) Orchestrator selection and post-merge behavior on Windows (clarify)
   - v1 called for selection rules but lacked precise conditions.
   - v2 rules:
     - If env AIFO_CODER_FORK_ORCH=gitbash and Git Bash or mintty is present: use Git Bash/mintty. If not found: error with the current exact message text.
     - If env AIFO_CODER_FORK_ORCH=powershell and PowerShell is available: use PowerShell windows (waitable).
     - Else, prefer Windows Terminal (wt.exe) when present.
       - If a post-merge strategy is requested (Fetch/Octopus), and Windows Terminal is selected, fall back to PowerShell windows to support waiting for panes. Emit the same warning text as current code.
       - If neither PowerShell nor Git Bash/mintty is available, but wt.exe exists, allow non-waiting launch and print the current manual-merge guidance text.
     - If wt.exe is absent: try PowerShell. If PowerShell absent: use Git Bash or mintty. If none are present: error with current message.
     - Orchestrator.supports_post_merge() returns true for PowerShell; false for Windows Terminal (non-waitable) and Git Bash/mintty. Tmux supports post-merge (triggered after session end).

4) Tmux inner script behavior (keep “press 's' to open a shell”)
   - Ensure tmux launch script includes the post-exit prompt with AIFO_CODER_FORK_SHELL_PROMPT_SECS honored, and the same shell kill-pane/exec logic. No message text changes.

5) Toolchain lifecycle RAII (scope and cleanup)
   - v1 proposed a RAII wrapper; v2 retains that goal but must reuse existing library functions:
     - start_if_requested(&Cli) performs:
       - Kind/spec parsing and normalization via aifo_coder helpers.
       - Overrides derivation from versions via default_toolchain_image_for_version.
       - Linux unix socket transport via env AIFO_TOOLEEXEC_USE_UNIX when requested.
       - Optional TypeScript bootstrap (typescript=global) when node is present.
       - Start proxy via toolexec_start_proxy and export AIFO_TOOLEEXEC_URL/TOKEN (+ AIFO_TOOLCHAIN_VERBOSE when verbose).
     - cleanup(self, verbose, in_fork_pane) stops proxy and sidecars unless in a fork pane (detected via AIFO_CODER_FORK_SESSION env), to avoid interfering with sibling panes.

6) Unit tests relocation (child args)
   - v1 suggested moving tests from main.rs to src/fork/args.rs. v2: Place fork_build_child_args in src/fork/args.rs (or src/fork/mod.rs) and move the existing test into a #[cfg(test)] mod args_tests within the same module. Keep test logic and expected strings identical.

7) CLI, banner, and images separation
   - v1 asked to move CLI types and banner out of main.rs. v2 keeps this, with the following:
     - src/cli.rs: Cli, Flavor, ToolchainKind, Agent, ForkCmd, validate_layout (unchanged flags/aliases).
     - src/banner.rs: print_startup_banner() and a private StartupInfo struct; reuse current string formatting and docker info parsing exactly.
     - src/agent_images.rs: default_image_for() and default_image_for_quiet() for agent images only, reusing aifo_coder::preferred_registry_prefix[_quiet]. Leave toolchain images in aifo_coder::toolchain::images.rs (library).

8) Streams and color
   - Maintain stdout vs stderr usage exactly as today:
     - Banner and informational summaries on stdout.
     - Warnings/notes/merge progress and errors on stderr as in current flows.
   - Use aifo_coder::paint and color_enabled_* consistently.

9) cfg gating and portability
   - Gate tmux orchestrator with cfg(not(windows)).
   - Gate Windows orchestrators with cfg(windows).
   - On Unix, don’t compile Windows-only files; on Windows, don’t compile tmux.

10) Error handling boundaries
   - Internal helpers return Result<T, E> with string errors where appropriate.
   - Convert to ExitCode only in the thin command handlers and main entrypoint.
   - Preserve current exit codes (1 for generic errors; 127 for NotFound paths).

Target module layout (bin crate)
- src/cli.rs
  - Cli, Flavor, ToolchainKind, Agent, ForkCmd, validate_layout (moved from main.rs). Keep all ValueEnum derives, aliases, and defaults.
- src/banner.rs
  - print_startup_banner() and internal StartupInfo with current heuristics (docker info parsing unchanged).
- src/agent_images.rs
  - default_image_for(), default_image_for_quiet() for agent images. Use aifo_coder::preferred_registry_prefix[_quiet] and environment overrides exactly as current code.
- src/toolchain_session.rs
  - ToolchainSession RAII wrapper:
    - start_if_requested(&Cli) -> Result<Option<ToolchainSession>, io::Error>
    - cleanup(self, verbose: bool, in_fork_pane: bool)
  - Internals reuse aifo_coder library functions (normalize, start session, bootstrap TS, start proxy).
- src/fork/mod.rs
  - fork_run(cli: &Cli, panes: usize) -> ExitCode (top-level orchestrator).
  - fork_build_child_args(cli: &Cli) -> Vec<String> (extracted from main.rs).
  - select_agent_str(cli) -> &'static str ("aider" | "crush" | "codex") for fork env.
- src/fork/types.rs
  - ForkSession { sid, session_name, base_label, base_ref_or_sha, base_commit_sha, created_at, layout, agent }
  - Pane { index, dir, branch, state_dir, container_name }
  - ForkOptions { verbose, keep_on_failure, merge_strategy, autoclean, dry_run, include_dirty, dissociate }
- src/fork/meta.rs
  - SessionMeta (serde_json) with keys:
    created_at, base_label, base_ref_or_sha, base_commit_sha, panes, panes_created (Option<usize>), pane_dirs, branches, layout, snapshot_sha (Option<String>)
  - write_initial_meta(dir, &SessionMeta) -> io::Result<()>
  - update_panes_created(dir, created_count, existing: &[(PathBuf, String)], snapshot_sha: Option<&str>, layout: &str) -> io::Result<()>
  - Guarantee key names, presence, and values match current manual JSON assembly from main.rs; preserve order.
- src/fork/env.rs
  - fork_env_for_pane(session: &ForkSession, pane: &Pane) -> Vec<(String, String)>
  - Always add: AIFO_CODER_SUPPRESS_TOOLCHAIN_WARNING=1, AIFO_CODER_SKIP_LOCK=1, and pane/session keys (AIFO_CODER_CONTAINER_NAME, AIFO_CODER_HOSTNAME, AIFO_CODER_FORK_SESSION, AIFO_CODER_FORK_INDEX, AIFO_CODER_FORK_STATE_DIR).
- src/fork/inner.rs
  - build_inner_powershell(session, pane, child_args) -> String (reuse aifo_coder::fork_ps_inner_string; append NoExit control via orchestrator flags, not by modifying inner).
  - build_inner_gitbash(session, pane, child_args, exec_shell_tail: bool) -> String (reuse aifo_coder::fork_bash_inner_string, optionally remove tail when post-merge requested).
  - build_tmux_launch_script(session, pane, child_args, launcher_path: &str) -> String (includes unchanged “press ‘s’ to open a shell” logic).
- src/fork/orchestrators/mod.rs
  - trait Orchestrator {
      fn launch(&self, session: &ForkSession, panes: &[Pane], child_args: &[String]) -> Result<(), String>;
      fn supports_post_merge(&self) -> bool;
    }
  - enum Selected { Tmux { reason: String }, WindowsTerminal { reason: String }, PowerShell { reason: String }, GitBashMintty { reason: String } }
  - fn select_orchestrator(cli: &Cli, layout_requested: &str) -> Selected (implements v2 selection rules above).
- src/fork/orchestrators/tmux.rs (cfg(not(windows)))
  - Implements tmux session creation, split/layout, send-keys, attach/switch using inner builders and meta helpers; prints exact messages currently used by main.rs (stdout/stderr).
- src/fork/orchestrators/windows_terminal.rs (cfg(windows))
  - wt.exe launcher. If a post-merge strategy is requested, automatically prefer PowerShell orchestrator with a clear warning; otherwise proceed and print manual merge guidance as in current code.
- src/fork/orchestrators/powershell.rs (cfg(windows))
  - Start-Process -PassThru PID capture, optional -NoExit, Wait-Process for post-merge; uses inner PS builder from library; prints per-pane PID lines exactly as current code.
- src/fork/orchestrators/gitbash_mintty.rs (cfg(windows))
  - Git Bash and mintty fallbacks; uses inner Git Bash builder. Keep “exec bash” tail only when no post-merge is requested.
- src/fork/post_merge.rs
  - apply_post_merge(repo_root, sid, strategy, autoclean, dry_run, verbose) -> Result<(), String>
  - Centralizes: colorized logs, invoking aifo_coder::fork_merge_branches_by_session, and conditional autoclean via aifo_coder::fork_clean. Message strings must match current main.rs output.

Implementation plan (phased, PR-size)
Phase 0 — Preparation
- Assert we won’t change CLI, outputs, or exit codes. Capture a list of strings printed by main.rs that must remain text-identical.
- Inventory all aifo_coder library functions currently used in main.rs to ensure they are available to orchestrator modules.

Phase 1 — Safe extractions (pure moves)
1. Create src/cli.rs and move Cli, Flavor, ToolchainKind, Agent, ForkCmd, validate_layout. Update main.rs to use crate::cli::*.
2. Create src/banner.rs and move print_startup_banner() with unchanged flow/strings.
3. Create src/agent_images.rs and move default_image_for() and default_image_for_quiet() for agent images only.
4. Extract fork_build_child_args(cli) into src/fork/mod.rs (or src/fork/args.rs) and move its current unit test intact to a #[cfg(test)] module there.

Phase 2 — Orchestrators and metadata deduplication
5. Add src/fork/{types.rs, meta.rs, env.rs, inner.rs} as defined above. inner.rs must reuse aifo_coder’s Windows builders and wt helpers.
6. Implement src/fork/orchestrators/* with the select_orchestrator logic clarified above.
7. Implement fork_run(cli, panes) in src/fork/mod.rs using:
   - Preflights (git/tmux/Windows tools).
   - Base info/snapshot via aifo_coder::* with unchanged prompts and messages.
   - Build ForkSession and Pane list; write initial meta via meta helpers.
   - Launch selected orchestrator; on failure, standardize cleanup and meta “panes_created” update.
   - Execute post-merge once via src/fork/post_merge.rs (if requested).
   - Print guidance via existing src/guidance.rs.
   - Maintain all message strings and color decisions.

Phase 3 — Toolchain session RAII
8. Create src/toolchain_session.rs with start_if_requested(&Cli) and cleanup(self, verbose, in_fork_pane).
   - Mirror the current behavior in main.rs: normalization, overrides from versions, unix socket transport, bootstrap, start proxy and export AIFO_TOOLEEXEC_URL/TOKEN, join and cleanup with respect to fork panes.
9. Update the agent execution path in main.rs to use ToolchainSession RAII for sidecars/proxy.

Phase 4 — Optional thin commands module
10. Add src/commands/mod.rs to move “images”, “cache”, “toolchain”, and agent run handlers out of main.rs. Keep all message strings and exit codes unchanged.

Testing strategy (v2)
- Move the child-args unit test from main.rs into src/fork (args_tests.rs or mod tests) and adapt imports (use crate::cli::*).
- Add unit tests for meta::write_initial_meta and update_panes_created to assert the exact keys and values produced (string-compare or serde_json::Value key checks). Preserve order to minimize diffs.
- Add unit tests for orchestrator selection (mock which() responses behind small indirection or cfg(test) shims).
- Continue relying on existing integration tests under tests/ that call aifo_coder::* (unchanged).
- Manual smoke tests (unchanged):
  - aifo-coder aider --dry-run --verbose
  - aifo-coder --fork 2 aider -- --help
  - aifo-coder images
  - aifo-coder toolchain --help
  - Doctor and Fork maintenance subcommands behave unchanged.

Risk mitigation
- Metadata JSON drift: lock key names and order; add focused unit tests.
- Windows orchestrator fallthrough: codify selection rules and mirror current warnings/messages.
- stdout/stderr ordering: keep banner/warnings/notes on the same streams; use eprintln where the original does.
- CI/TTY detection: preserve atty checks and color behavior.
- Avoid duplication with library helpers to reduce divergence (especially Windows inner builders and wt helpers).

Acceptance criteria
- cargo test passes locally (including integration tests under tests/).
- All manual smoke tests behave exactly as before (outputs, exit codes).
- Fork metadata files contain the same keys and expected values as before for fork runs from the bin entrypoint.
- Orchestrator selection and messages match existing behavior (Windows and Unix-like).
- main.rs is reduced to a thin dispatcher: parse CLI, set color/env, early fork maintenance handlers, fork_run dispatch, doctor/images/cache/toolchain subcommands dispatch, and agent run dispatch.

Rollback plan
- Each phase is self-contained and reversible. If regressions are detected, revert the last phase without impacting others.

Notes
- serde_json is already a dependency (keep using it).
- Keep all color handling via aifo_coder::paint and color_enabled_* to avoid drift.
- Retain the existing guidance text from src/guidance.rs; do not modify output strings in this refactor.
