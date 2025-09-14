AIFO Coder main.rs refactoring and cleanup v3 — Production-ready, library-aware specification and phased plan

Summary
- Goal: Reduce src/main.rs to a thin, testable entrypoint that wires CLI → cohesive internal modules while strictly preserving observable behavior (CLI, outputs, exit codes, env semantics, colorization, stdout/stderr ordering).
- Approach: Reuse existing aifo_coder library helpers; introduce bin-side modules for CLI, banner, orchestrators, metadata writers, toolchain session RAII, and fork glue. Avoid duplicating logic that already exists in the library.
- Constraints: No public CLI changes; no changes to aifo_coder library APIs; no functional drift in fork/merge flows or Docker command shapes.

Behavioral invariants (must remain exactly)
- CLI flags, aliases, defaults, and environment semantics unchanged.
- Log text and ordering remain 1:1, including:
  - Startup banner and summaries on stdout.
  - Warnings/notes/merge progress and errors on stderr; use eprintln where main.rs currently does.
- Exit codes unchanged for all success/error paths (including 127 for NotFound cases).
- Dry-run behavior exactly unchanged (previews printed, execution skipped).
- Color/TTY semantics via aifo_coder::color_* helpers unchanged; do not colorize the banner differently than today.
- Environment variables read/set and their timings remain identical (e.g., AIFO_CODER_SKIP_LOCK, AIFO_CODER_FORK_SESSION, AIFO_TOOLEEXEC_*).
- Metadata JSON files contain the same keys/values; preserve key order to minimize diffs where tests compare textually.

Non-goals
- No changes to CLI surface or defaults.
- No changes to aifo_coder library APIs or behavior.
- No changes to Docker command shapes or fork/merge flows.

Key corrections vs v2 (gap fixes)
1) Windows orchestrator preference env precedence
   - Correct selection to honor AIFO_CODER_FORK_ORCH before probing Windows Terminal:
     - AIFO_CODER_FORK_ORCH=gitbash → use Git Bash or mintty if present; else error with current exact text.
     - AIFO_CODER_FORK_ORCH=powershell → prefer PowerShell if available.
     - Else: prefer wt.exe; if post-merge requested and wt selected, automatically fall back to PowerShell and emit same warning text as today.
   - Keep all message strings and exit codes verbatim.

2) Library inner builders + required extra env
   - Reuse aifo_coder::fork_ps_inner_string and fork_bash_inner_string.
   - Add AIFO_CODER_SUPPRESS_TOOLCHAIN_WARNING=1 without changing lib APIs:
     - PowerShell: insert "$env:AIFO_CODER_SUPPRESS_TOOLCHAIN_WARNING='1';" immediately after the Set-Location prefix (first “; ”).
     - Git Bash: prefix "export AIFO_CODER_SUPPRESS_TOOLCHAIN_WARNING=1; " after existing export lines.
   - Git Bash “exec bash” tail:
     - Keep tail only when no post-merge is requested; trim trailing "; exec bash" when a post-merge strategy is requested.

3) wt helpers argv[0]
   - wt_build_new_tab_args and wt_build_split_args include "wt" as argv[0]. When executing via Command::new(wt_path), drop the first element; keep the full vector only for preview strings to remain identical to current output.

4) Base commit SHA for metadata
   - Use the exact current algorithm when writing initial meta:
     - If snapshot created: base_commit_sha = snapshot sha.
     - Else: try "git rev-parse --verify <base_ref_or_sha>" → if success, use that sha.
     - Else: fall back to the HEAD SHA from fork_base_info.

5) Metadata updates
   - update_panes_created must read current .meta.json, update panes_created and two arrays (pane_dirs, branches) from on-disk existing panes, and preserve all other keys (including optional snapshot_sha) and key order. Do not re-derive base_* fields here.

6) Guidance call patterns (unchanged)
   - Keep print_inspect_merge_guidance flags exact per orchestrator:
     - Git Bash enforced: include_remote_examples=true, extra_spacing_before_wrapper=false.
     - Windows Terminal: include_remote_examples=false, extra_spacing_before_wrapper=true.
     - PowerShell: include_remote_examples=false, extra_spacing_before_wrapper=true.
     - Tmux: include_remote_examples=false, extra_spacing_before_wrapper=true, with colored header based on stdout TTY.

7) Banner and stdout/stderr
   - Keep banner prints exactly as today (stdout, same text, emojis, no additional color handling).
   - Continue using aifo_coder::paint and color_enabled_* only where current code already does (stderr/info flows).

8) Exact messages and exit codes
   - Preserve all preflight and error strings verbatim (e.g., tmux not found message, Windows orchestrator not found messages, manual post-merge guidance lines).
   - Return ExitCode::from(127) for NotFound paths; otherwise unchanged codes.

Target bin crate module layout
- src/cli.rs
  - Types: Cli, Flavor, ToolchainKind, Agent, ForkCmd.
  - validate_layout(s) -> Result<String, String>.
  - 1:1 Clap attributes, ValueEnum derives, aliases, default_value_t fields as in current main.rs.
- src/banner.rs
  - print_startup_banner() with same strings, Docker info parsing heuristics, and stdout usage.
- src/agent_images.rs
  - default_image_for(agent: &str) and default_image_for_quiet(agent: &str) for agent images only.
  - Use aifo_coder::preferred_registry_prefix[_quiet] and environment overrides exactly as current code.
- src/toolchain_session.rs
  - ToolchainSession RAII:
    - start_if_requested(&Cli) -> Result<Option<ToolchainSession>, io::Error>
      - If no toolchain flags, return Ok(None).
      - Reuse aifo_coder functions: normalize kinds, derive overrides from versions via default_toolchain_image_for_version, apply unix socket env on Linux, bootstrap TS when requested and node present, start session, start proxy, export AIFO_TOOLEEXEC_* (+AIFO_TOOLCHAIN_VERBOSE when verbose).
    - cleanup(self, verbose: bool, in_fork_pane: bool)
      - Stop proxy and sidecars unless in a fork pane (AIFO_CODER_FORK_SESSION set).
- src/fork/mod.rs
  - fork_run(cli: &Cli, panes: usize) -> ExitCode (top-level fork orchestration).
  - fork_build_child_args(cli: &Cli) -> Vec<String> extracted as-is from main.rs.
  - select_agent_str(cli) -> &'static str ("aider"|"crush"|"codex").
- src/fork/types.rs
  - ForkSession { sid, session_name, base_label, base_ref_or_sha, base_commit_sha, created_at, layout, agent }
  - Pane { index, dir, branch, state_dir, container_name }
  - ForkOptions { verbose, keep_on_failure, merge_strategy, autoclean, dry_run, include_dirty, dissociate }
- src/fork/meta.rs
  - SessionMeta serde struct with keys (order preserved):
    created_at, base_label, base_ref_or_sha, base_commit_sha, panes, panes_created (Option<usize>), pane_dirs, branches, layout, snapshot_sha (Option<String>)
  - write_initial_meta(dir, &SessionMeta) -> io::Result<()>; compute base_commit_sha per rules above; preserve key order on write.
  - update_panes_created(dir, created_count, existing: &[(PathBuf, String)], snapshot_sha: Option<&str>, layout: &str) -> io::Result<()>; read, minimally update fields, preserve key order.
- src/fork/env.rs
  - fork_env_for_pane(session: &ForkSession, pane: &Pane) -> Vec<(String, String)> with:
    AIFO_CODER_SUPPRESS_TOOLCHAIN_WARNING=1, AIFO_CODER_SKIP_LOCK=1, and pane/session keys (AIFO_CODER_CONTAINER_NAME, AIFO_CODER_HOSTNAME, AIFO_CODER_FORK_SESSION, AIFO_CODER_FORK_INDEX, AIFO_CODER_FORK_STATE_DIR).
- src/fork/inner.rs
  - build_inner_powershell(session, pane, child_args) -> String:
    reuse aifo_coder::fork_ps_inner_string and inject SUPPRESS var (string augmentation); leave -NoExit control to orchestrators.
  - build_inner_gitbash(session, pane, child_args, exec_shell_tail: bool) -> String:
    reuse aifo_coder::fork_bash_inner_string, inject SUPPRESS var, strip "; exec bash" tail when exec_shell_tail=false (i.e., post-merge requested).
  - build_tmux_launch_script(session, pane, child_args, launcher_path: &str) -> String:
    same “press 's' to open a shell” logic, honoring AIFO_CODER_FORK_SHELL_PROMPT_SECS.
- src/fork/orchestrators/mod.rs
  - trait Orchestrator {
      fn launch(&self, session: &ForkSession, panes: &[Pane], child_args: &[String]) -> Result<(), String>;
      fn supports_post_merge(&self) -> bool;
    }
  - enum Selected { Tmux { reason: String }, WindowsTerminal { reason: String }, PowerShell { reason: String }, GitBashMintty { reason: String } }
  - fn select_orchestrator(cli: &Cli, layout_requested: &str) -> Selected with corrected v2 rules:
    - If env AIFO_CODER_FORK_ORCH=gitbash: use Git Bash or mintty when present; else error with current exact message.
    - If env AIFO_CODER_FORK_ORCH=powershell and PowerShell is available: select PowerShell.
    - Else prefer wt.exe.
      - If post-merge requested while wt selected: fall back to PowerShell with current warning string.
      - If no waitable orchestrator available but wt exists: allow non-waiting launch and print manual-merge guidance identical to current code.
    - If wt.exe absent: try PowerShell; else Git Bash/mintty; else error with current message.
  - Messages and stderr/stdout routing identical to current main.rs.
- src/fork/orchestrators/tmux.rs (cfg(not(windows)))
  - Implement tmux session creation, split/layout, send-keys, attach/switch; reuse inner builders; write initial meta and update on failures; print exactly the current strings and previews (stdout/stderr).
- src/fork/orchestrators/windows_terminal.rs (cfg(windows))
  - wt.exe launcher with -NoExit for panes; honor post-merge fallback to PowerShell; print manual-merge guidance identical to current code when non-waiting.
  - Use wt_* helpers for preview strings or drop argv[0] when executing.
- src/fork/orchestrators/powershell.rs (cfg(windows))
  - Separate PowerShell windows via Start-Process -PassThru, capture PIDs, optionally -NoExit; use ps_wait_process_cmd for waiting; print per-pane PID lines exactly as now.
- src/fork/orchestrators/gitbash_mintty.rs (cfg(windows))
  - Launch via git-bash.exe or mintty.exe; keep “exec bash” tail only when no post-merge is requested; previews and errors identical to current code.
- src/fork/post_merge.rs
  - apply_post_merge(repo_root, sid, strategy, autoclean, dry_run, verbose) -> Result<(), String>
  - Centralize stderr-colored logs and aifo_coder::fork_merge_branches_by_session invocation; perform autoclean exactly as current main.rs.

Streams and color
- stdout: banner and informational summaries (as today).
- stderr: warnings/notes/progress and errors (as today).
- Use aifo_coder::paint and color_enabled_* where current code already does; do not change banner color behavior.

Platform gating and portability
- cfg(not(windows)) for tmux orchestrator.
- cfg(windows) for Windows orchestrators and helpers.
- Avoid compiling Windows-only code on Unix and vice versa.

Error handling boundaries
- Internal helpers return Result<T, String/io::Error> as appropriate.
- Convert to ExitCode only in the thin command handlers and main entrypoint.
- Preserve 127 for NotFound paths and all current exit codes.

Testing strategy
- Move existing child-args unit test into src/fork/mod.rs (or args.rs) under #[cfg(test)] args_tests; keep expected strings identical.
- Add unit tests:
  - meta::write_initial_meta and update_panes_created checking exact keys and presence; preserve order (either via serde_json::Value with deterministic serializer or string compare).
  - orchestrator selection honoring AIFO_CODER_FORK_ORCH and fallbacks; place which() behind small test shims to mock presence/absence.
  - inner::build_inner_gitbash trims "; exec bash" when post-merge requested; PS/Git Bash injects AIFO_CODER_SUPPRESS_TOOLCHAIN_WARNING=1.
  - base_commit_sha resolution branch (snapshot vs rev-parse vs fallback).
- Keep existing library tests unchanged; ensure cargo test passes on supported platforms.

Phased implementation plan (PR-sized)
Phase 0 — Preparation and string inventory
- Enumerate all user-visible strings in current main.rs that must remain identical (errors, warnings, previews, guidance, summaries).
- Verify all aifo_coder helpers needed by orchestrators exist and match expectations (fork_* builders, wt_* helpers, ps_wait_process_cmd, merge and clean helpers).

Phase 1 — Safe extractions (pure moves, no behavior change)
- Add src/cli.rs: move Cli, Flavor, ToolchainKind, Agent, ForkCmd, validate_layout with all Clap attributes and default values unchanged.
- Add src/banner.rs: move print_startup_banner() as-is (stdout-only, same heuristics/strings).
- Add src/agent_images.rs: move default_image_for() and default_image_for_quiet() for agent images only.
- Extract fork_build_child_args(cli) into src/fork/mod.rs (or src/fork/args.rs), move its current unit test into #[cfg(test)] mod args_tests within the same module.

Phase 2 — Orchestrators and metadata helpers
- Add src/fork/{types.rs, env.rs, inner.rs, meta.rs} with definitions above (env injection rules, inner augmentation, metadata writers).
- Implement src/fork/orchestrators/* (tmux, windows_terminal, powershell, gitbash_mintty) with corrected selection rules and exact messages.
- Implement fork_run(cli, panes) in src/fork/mod.rs:
  - Preflights (git/tmux/Windows tool presence).
  - Base info and optional snapshot via aifo_coder::* (unchanged user prompts/messages).
  - Build ForkSession/Panes; write initial meta via meta helpers.
  - Launch orchestrator; update panes_created on failure with existing panes; honor fork_keep_on_failure.
  - Execute post-merge via src/fork/post_merge.rs; print guidance via existing guidance.rs.
- Maintain stdout/stderr ordering and color decisions.

Phase 3 — Toolchain session RAII
- Add src/toolchain_session.rs with start_if_requested(&Cli) and cleanup(self, verbose, in_fork_pane), reusing aifo_coder library functions.
- Update main.rs agent execution path to use ToolchainSession RAII for sidecars and proxy; preserve env set order and messages.

Phase 4 — Optional thin commands module (if desired and small)
- Add src/commands/mod.rs to move “images”, “cache”, “toolchain”, and agent run handlers; keep strings and exit codes identical. This can be deferred if it introduces risk.

Phase 5 — Tests and polish
- Ensure moved unit tests compile and pass.
- Add new orchestrator/meta/inner tests noted above.
- Run all existing integration tests under tests/ unchanged.

Risk mitigation
- Metadata drift: lock key names and order in meta serializer; test them explicitly.
- Windows orchestrator fallthrough: keep messages verbatim and mirror current fallbacks; test select_orchestrator.
- stdout/stderr ordering: keep banner/warnings/notes on the same streams; use eprintln where the original does.
- TTY/color behavior: do not alter banner color; use aifo_coder::color_* in places already using them.
- Library reuse: avoid duplicating builders; only apply minimal string augmentation for SUPPRESS var and Git Bash tail.

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

Rollback plan
- Each phase is self-contained. If regressions occur, revert the last phase to regain stability without undoing prior safe extractions.

Notes
- serde_json already a dependency; use it in meta helpers for correctness and determinism.
- Keep guidance text from src/guidance.rs unchanged.
- Retain OutputNewlineGuard and the initial leading println!() in main to preserve trailing newline behavior on stdout.
