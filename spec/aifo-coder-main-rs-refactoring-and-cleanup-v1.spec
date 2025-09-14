AIFO Coder main.rs refactoring and cleanup v1

Objective
- Reduce src/main.rs to a small, readable entrypoint that wires CLI parsing to cohesive modules.
- Eliminate duplication (especially Windows orchestrators vs tmux paths, and post-merge logic).
- Centralize fork metadata writing via serde_json (already a dependency).
- Improve testability by extracting pure builders and small units with clear responsibilities.
- Preserve behavior 1:1 (CLI, outputs, exit codes, environment semantics).

Non-goals (v1)
- No changes to public CLI flags, default values, or environment variable semantics.
- No changes to library (aifo_coder) APIs other than reading existing functions.
- No functional changes to fork/merge flows or Docker command shape.

Current pain points (validated)
- main.rs is monolithic (2k+ LOC) with intertwined concerns: CLI, banner, agent image resolution, fork orchestration (tmux and multiple Windows launchers), toolchain session lifecycle, metadata building, post-merge, and repeated error cleanup branches.
- Windows orchestration is duplicated (wt.exe, PowerShell, Git Bash, mintty) with repeated post-merge logic.
- Metadata files are hand-built JSON strings in multiple places.
- Inner command builders (PS/Bash/tmux script) are reimplemented inline in several branches.
- Tests for child args live in main.rs, making reuse awkward.

Design guidelines
- Separate bin-crate concerns (banner, agent image resolution, orchestrators) from library (aifo_coder) which already provides core features.
- Introduce narrowly scoped modules, each with a single responsibility.
- Return Result<T, E> from internal helpers; convert to ExitCode only at user-facing layer(s).
- Keep log strings and stdout/stderr ordering unchanged (golden behavior preservation).
- Gate OS-specific code with cfg attributes to avoid accidental cross-platform regressions.

Target module layout (bin crate)
- src/cli.rs
  - Cli, Flavor, ToolchainKind, Agent, ForkCmd, validate_layout (moved from main.rs).
  - Keep ValueEnum derives and aliases (e.g., ts for Typescript).
- src/banner.rs
  - print_startup_banner() and StartupInfo (os/arch, virtualization, docker path, seccomp, cgroupns, rootless, AppArmor support/profile, version).
  - Reuse current heuristic parsing for docker info; optional serde_json parse can be Phase 5.
- src/agent_images.rs
  - default_image_for() and default_image_for_quiet() for agent images (codex, crush, aider).
  - Intentionally separate from library’s src/toolchain/images.rs to avoid collisions.
- src/toolchain_session.rs
  - RAII wrapper for toolchain sidecars/proxy lifecycle.
  - start_if_requested(&Cli) -> Result<ToolchainSession, io::Error>
  - cleanup(self, verbose: bool, in_fork_pane: bool)
  - Implements: kinds/spec parsing and normalization, overrides, optional unix socket selection (Linux), TypeScript bootstrap, start proxy and export env, graceful shutdown/join.
- src/fork/mod.rs
  - pub fn fork_run(cli: &Cli, panes: usize) -> ExitCode (top-level orchestrator; replaces monolith).
  - pub fn fork_build_child_args(cli: &Cli) -> Vec<String> (moved from main.rs; unit-tested).
  - Helper: select agent string ("aider" | "crush" | "codex") for use in fork env.
- src/fork/types.rs
  - ForkSession { sid, session_name, base_label, base_ref_or_sha, base_commit_sha, created_at, layout, agent }
  - Pane { index, dir, branch, state_dir, container_name }
  - ForkOptions { verbose, keep_on_failure, merge_strategy, autoclean, dry_run, include_dirty, dissociate }
- src/fork/meta.rs
  - SessionMeta (serde_json):
    - created_at, base_label, base_ref_or_sha, base_commit_sha, panes, panes_created (Option<usize>), pane_dirs, branches, layout, snapshot_sha (Option<String>)
  - write_initial_meta(dir, &SessionMeta) -> io::Result<()>
  - update_panes_created(dir, created_count, existing: &[(PathBuf, String)], snapshot_sha: Option<&str>, layout: &str) -> io::Result<()>
  - Guarantees same keys as current manual JSON (minimize diffs).
- src/fork/post_merge.rs
  - apply_post_merge(repo_root, sid, strategy, autoclean, dry_run, verbose) -> Result<(), String>
  - Centralizes: colorized logs, invoking aifo_coder::fork_merge_branches_by_session, and conditional autoclean via aifo_coder::fork_clean.
- src/fork/env.rs
  - fork_env_for_pane(session: &ForkSession, pane: &Pane) -> Vec<(String, String)>
  - Always adds AIFO_CODER_SUPPRESS_TOOLCHAIN_WARNING=1 for fork panes.
- src/fork/inner.rs
  - build_inner_powershell(session, pane, child_args) -> String
  - build_inner_gitbash(session, pane, child_args, exec_shell_tail: bool) -> String
  - build_tmux_launch_script(session, pane, child_args, launcher_path: &str) -> String
    - Includes the current “press ‘s’ to open a shell” post-exit prompt logic unchanged.
- src/fork/orchestrators/mod.rs
  - trait Orchestrator {
      fn launch(&self, session: &ForkSession, panes: &[Pane], child_args: &[String]) -> Result<(), String>;
      fn supports_post_merge(&self) -> bool;
    }
  - fn select_orchestrator(cli: &Cli, layout_requested: &str) -> Selected { variant + reason }
- src/fork/orchestrators/tmux.rs                (cfg(not(windows)))
  - Implements tmux session creation/split/layout/send-keys/attach using inner builders and meta helpers.
- src/fork/orchestrators/windows_terminal.rs    (cfg(windows))
  - wt.exe launcher. If a merging strategy is requested and waiting is required, either:
    - fall back automatically to PowerShell orchestrator (waitable), or
    - clearly warn and print manual merge instructions (behavior preserved).
- src/fork/orchestrators/powershell.rs          (cfg(windows))
  - Start-Process -PassThru PID capture, optional -NoExit, Wait-Process for post-merge; uses inner PS builder.
- src/fork/orchestrators/gitbash_mintty.rs      (cfg(windows))
  - Git Bash and mintty fallbacks, using inner Git Bash builder; keeps “exec bash” tail only when no post-merge is requested.
- src/commands/mod.rs (optional Phase 4+)
  - Thin runners to further declutter main.rs (images, cache, toolchain, agent run).

Compatibility constraints (must-haves)
- Log text and ordering remain the same (banner, notes, warnings, previews).
- Exit codes remain unchanged for all success/error paths.
- Dry-run behavior unchanged; still prints previews and skips execution.
- Environment variable semantics unchanged (read or set) for all flows.
- Metadata file contains the same keys/fields as today (values equal where deterministically comparable).

Testing strategy (v1)
- Move the child-args unit test from main.rs into src/fork/args.rs (or args_tests.rs) and adapt imports.
- Add unit tests for meta::write_initial_meta and update_panes_created (assert keys and core values).
- Rely on existing integration tests in tests/ that call aifo_coder:: functions (unchanged by refactor).
- Smoke tests (manual):
  - aifo-coder aider --dry-run --verbose
  - aifo-coder --fork 2 aider -- --help
  - aifo-coder images
  - aifo-coder toolchain --help (sidecar path)
  - Doctor and Fork maintenance subcommands behave unchanged.

Risks and mitigations
- Metadata JSON shape drift: lock key names to current output, add focused unit tests.
- Windows orchestrator fallthrough: ensure selection rules match current preferences and warnings.
- stdout/stderr ordering changes: keep banner/warnings/notes on the same streams (use eprintln where original does).
- CI/TTY detection: preserve atty checks and existing color behavior.

Optimized phased plan (PR-sized, low risk)

Phase 0 — Preparation and constraints
- Confirm we will not change CLI, outputs, or exit codes.
- Inventory uses of aifo_coder::* in main.rs; ensure all are available to modules post-extraction (public visibility ok).
- Document current strings that must remain stable (banner + warnings) for quick comparison.

Phase 1 — Safe extractions (pure moves, no logic changes)
1. Create src/cli.rs
   - Move Cli, Flavor, ToolchainKind, Agent, ForkCmd, validate_layout.
   - Update main.rs to use crate::cli::*.
2. Create src/banner.rs
   - Move print_startup_banner and its helper code; expose print_startup_banner() and, internally, a StartupInfo struct.
   - Keep strings and flow identical.
3. Create src/agent_images.rs
   - Move default_image_for() and default_image_for_quiet() (agent images only).
   - Update main.rs calls (Images subcommand and agent run path).
4. Create src/fork/mod.rs (initial)
   - Move fork_build_child_args(cli: &Cli) -> Vec<String>.
   - Move its unit test here (adapt imports) without behavior changes.

Phase 2 — Fork orchestration structure and deduplication
5. Add src/fork/types.rs and src/fork/meta.rs
   - Introduce ForkSession, Pane, ForkOptions, and SessionMeta (serde_json).
   - Replace all manual JSON string assembly in fork paths with meta helpers.
6. Add src/fork/env.rs and src/fork/inner.rs
   - Implement fork_env_for_pane and inner command/script builders.
   - Remove inline closures build_ps_inner/build_bash_inner/build tmux script; call shared helpers.
7. Introduce Orchestrator trait and implementations
   - src/fork/orchestrators/{tmux.rs, windows_terminal.rs, powershell.rs, gitbash_mintty.rs}
   - src/fork/orchestrators/mod.rs with select_orchestrator(cli, layout).
   - Ensure logs/warnings and preferences match current behavior.
8. Implement fork_run(cli, panes) in src/fork/mod.rs
   - High-level flow:
     - Preflight (git/tmux/Windows tools).
     - Determine base info and snapshot (using aifo_coder::*).
     - Build ForkSession and Pane list; write initial meta.
     - Launch orchestrator; on failure, standardize cleanup and meta update.
     - Run post-merge once via src/fork/post_merge.rs (if requested).
     - Print guidance via existing src/guidance.rs.
   - Remove duplicated post-merge code paths; consolidate to a single call site.

Phase 3 — Toolchain lifecycle RAII
9. Create src/toolchain_session.rs
   - Implement start_if_requested(&Cli) and cleanup(self, verbose, in_fork_pane).
   - Move “toolchain attach” responsibilities from main.rs:
     - Parse kinds/specs/overrides, normalize, add derived overrides for versions.
     - Linux unix socket transport env switch (AIFO_TOOLEEXEC_USE_UNIX).
     - Bootstrap typescript=global when requested and node present.
     - Start proxy and export AIFO_TOOLEEXEC_URL/TOKEN (+ AIFO_TOOLCHAIN_VERBOSE when verbose).
     - Provide proxy flag/handle for graceful shutdown and .join().
   - Ensure cleanup respects fork panes (do not auto-clean shared sidecars for panes).
10. Update main.rs agent path to use ToolchainSession RAII wrapper.

Phase 4 — Commands module and final entrypoint cleanup (optional but recommended)
11. Create src/commands/mod.rs
   - run_doctor(verbose), run_images(), run_cache_clear(), run_toolchain_cache_clear(verbose), run_toolchain(kind, image_override, no_cache, args, verbose, dry_run), run_agent(agent, args, image, &Cli).
   - Migrate subcommand handlers from main.rs into these functions; keep main.rs concise.
12. Verify that main.rs contains only:
   - parse CLI + set color/env
   - early fork maintenance (list/clean/merge) calls
   - fork_run dispatch
   - doctor/images/cache/toolchain subcommands dispatch
   - agent run dispatch via run_agent()

Phase 5 — Optional quality improvements (off by default; careful review)
- Parse Docker security options via serde_json instead of manual scanning; fallback if parsing fails.
- Collect env var constants into a small envkeys.rs (e.g., "AIFO_CODER_*" names) to avoid typos.
- Expand unit tests for orchestrator selection and meta updates.
- Add developer docs in docs/refactor-main.md summarizing the architecture.

Deliverables per phase
- Phase 1: src/cli.rs, src/banner.rs, src/agent_images.rs, src/fork/mod.rs (fork_build_child_args), main.rs updated to import and use them. All tests pass.
- Phase 2: src/fork/{types.rs, meta.rs, env.rs, inner.rs, orchestrators/*}, fork_run implemented; main.rs calls fork_run; tests pass.
- Phase 3: src/toolchain_session.rs; main.rs agent path updated; tests pass.
- Phase 4: src/commands/mod.rs; main.rs slimmed to dispatcher; tests pass.

Acceptance criteria
- cargo test passes locally (including integration tests under tests/).
- Manual smoke tests behave exactly as before (outputs, exit codes).
- Fork metadata files contain the same keys and expected values as before.
- Orchestrator selection and messages match existing behavior (Windows and Unix-like).

Rollback plan
- Each phase is self-contained and reversible.
- Limit PR size; if regressions are detected, revert the last phase without impacting others.

Notes
- Use of serde_json is already supported (dependency present).
- Keep all color handling via aifo_coder::paint and color_enabled_* helpers to avoid drift.
- Retain the existing guidance text from src/guidance.rs; do not modify output strings in v1 refactor.
