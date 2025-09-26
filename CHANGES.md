2025-09-26 15:30 user@example.com

feat: add Node toolchain image/version env overrides

- Implemented AIFO_NODE_TOOLCHAIN_IMAGE and AIFO_NODE_TOOLCHAIN_VERSION in image selection.
- Keeps rust overrides intact; preserves unknown-kind fallback to node:20-bookworm-slim.
- No user-visible string changes; tests remain green.

2025-09-26 15:10 user@example.com

tests: add optional node preview run/exec checks

- Added tests/preview_node_run_mount.rs to assert aifo-node-cache mount and key envs.
- Added tests/preview_node_exec_env.rs to assert PNPM_HOME and PATH include $PNPM_HOME/bin.
- Updated scoring per AGENT.md and proposed next steps.

2025-09-25 14:45 user@example.com

SCORE: comprehensive scoring and next steps (v2 refactor complete)

- Wrote an updated SCORE.md with grades, detailed assessment, issues, and next steps.
- Preserved user-visible behavior and tests (green); no functional changes made.

2025-09-25 14:05 user@example.com

Tidy: reduce dead_code allowances and silence unused fields

- Replace type-level #[allow(dead_code)] on fork types with field-level where needed.
- Remove enum-level allowance on Selected; rename variant field to _reason and update uses.
- No behavior or user-visible strings changed; tests remain green.

2025-09-25 13:25 user@example.com

Fix: robust ahead/base-unknown detection in fork pane checks

- Switched pane 'ahead' detection to use merge-base(base, HEAD) + rev-parse HEAD instead of rev-list counts.
- Correctly marks 'ahead' when base is an ancestor and HEAD != base; marks 'base-unknown' when base is not an ancestor or HEAD cannot be resolved.
- Keeps stderr silenced for git helper commands to avoid noisy test output.
- Resolves fork_autoclean and keep-dirty semantics: protected (ahead) sessions are no longer deleted.

2025-09-25 12:50 user@example.com

Fix: fork pane status detection and noisy git stderr

- Updated pane cleanliness checker to avoid marking base-unknown when a base commit is present but not an ancestor; treats panes as not-ahead instead.
- Suppressed git stderr in pane checks (merge-base/rev-list) and submodule status to avoid "fatal: Not a valid object name" noise in tests.
- This restores expected behavior for fork clean/autoclean tests: old clean sessions are deletable; protected (ahead/dirty) panes remain.

2025-09-24 12:40 user@example.com

Phase 3: tests consolidation (incremental)

- Added shared tests/support::capture_stdout() helper (Unix) to eliminate duplicated inline stdout-capture code.
- Updated tests/fork_list_plain_nocolor.rs to import tests/support and use the shared helper.
- Kept all user-visible messages and assertions unchanged.

2025-09-24 12:15 user@example.com

Phase 2 completion confirmation

- Completed error-surface audit: remaining io::Error::new/other sites at user-visible boundaries now route through display_for_toolchain_error/display_for_fork_error where applicable.
- No further changes required for Phase 2 in provided modules; logs/messages and behavior remain identical.
- All tests pass; lint/format checks are clean.

2025-09-24 11:55 user@example.com

Phase 2: error-surface audit and tiny refactors

- Wrapped proxy bind/address io::Error constructions via display_for_toolchain_error (no text change).
- Kept internal runtime errors localized; no user-visible changes.
- No changes to HTTP responses or log strings; CI expected to remain green.

2025-09-24 11:20 user@example.com

Phase 1: hygiene and consistency completed

- Prefer crate:: for intra-crate references in library modules; kept aifo_coder:: in shared fork modules compiled into the binary to preserve the build.
- Added orchestrator trait docs clarifying supports_post_merge semantics; documented warn module stty best-effort behavior; added error mapping guide header in errors.rs.
- Removed unnecessary #[allow(dead_code)] on ToolchainError and display_for_toolchain_error; retained allowances where platform gating may hide usage.
- Verified no remaining aifo_coder:: references in library-only modules; small hygiene clean-ups as needed.

2025-09-24 10:35 user@example.com

Refactor v2: comprehensive, phase-optimized specification

- Wrote an expanded, risk-aware, phase-optimized refactor plan covering:
  Windows orchestrators SUPPRESS injection, proxy bind configurability on Linux,
  default image alignment (Node 22), prompt/input consistency, final error
  surface audit, test helper consolidation, hygiene, and optional metrics/CI.
- Added the full specification to spec/aifo-coder-refactor-whole-codebase-v2.spec.
- No runtime behavior changed in this step; documentation/spec only.

2025-09-24 08:45 user@example.com

Error-surface consistency: wrap remaining io::Error::other messages

- Wrapped NotFound and empty pane directories errors in src/fork_impl/merge.rs using display_for_fork_error(ForkError::Message) for uniformity.
- No user-visible strings changed; preserves exit codes and behavior.

2025-09-24 08:20 user@example.com

Phase 5 follow-up: error-surface consistency and proxy helpers

Short summary: Wrap remaining io::Error::other strings, add proxy log helper.

- Wrapped remaining io::Error::other strings with display_for_* in lock.rs, merge.rs, and toolchain sidecar start paths.
- Added proxy helper log_disconnect() and constant to reduce repeated disconnect strings; reused in two places.
- No user-visible strings changed; behavior unchanged. Suggested running rustfmt.

2025-09-24 08:00 user@example.com

Scoring: comprehensive source code assessment and next steps

- Wrote comprehensive scoring to SCORE.md; kept previous score in SCORE-before.md if present.
- No source code behavior changed; documentation/analysis only.

2025-09-24 07:40 user@example.com

Phase 5: documentation and style

Short summary: Add module docs and crate-level doc; keep strings unchanged.

- Added crate-level documentation in src/lib.rs summarizing architecture and env invariants.
- Added module-level docs to src/fork/{types,env}.rs, src/agent_images.rs, src/fork_args.rs,
  and src/toolchain_session.rs to aid contributors.
- Preferred shorter lines in comments/doc blocks; no user-visible strings changed.
- Left golden outputs and tested strings intact.

2025-09-24 07:20 user@example.com

Phase 4: error enums adoption and contributor docs

- Adopted lightweight error enums (ForkError, ToolchainError) internally in fork_impl/* and
  toolchain/* for sentinel cases, mapping to identical strings at boundaries.
- Added module-level docs for orchestrators overview and runner decomposition to aid contributors.
- Continued measured logging-helper adoption where message text is identical (no string changes).

2025-09-24 06:30 user@example.com

Phase 4: finalize logging helpers adoption in toolchain session

- Adopted log_error_stderr for toolchain startup failures (sidecars/proxy) in toolchain_session.rs,
  preserving exact message text and adding consistent color-aware stderr output.
- Phase 4 is now complete: runner decomposition, logging helpers, and targeted adoption across
  fork and toolchain paths without changing user-visible strings.

2025-09-24 06:00 user@example.com

Phase 4: re-export error enums and adopt logging helpers in warnings

- Re-exported ForkError/ToolchainError and helper mapping/display functions via lib.rs for
  internal use across subsystems without changing external APIs or messages.
- Adopted log_warn_stderr for key warning headers in warnings.rs (sidecars and LLM credentials)
  to standardize color-aware stderr logging while preserving exact strings.

2025-09-24 05:30 user@example.com

Phase 4: complete logging helper adoption in errors

- Switched refusal header in fork clean prompt to log_error_stderr (red) with identical text.
- Adopted log_error_stderr in toolchain command error paths (cache clear and run), preserving messages.

2025-09-24 05:00 user@example.com

Phase 4: finalize by adopting logging helper in clean prompt

- Replaced a paint-based warning in fork clean confirmation with log_warn_stderr,
  preserving the exact message text and color semantics where identical.
- Error enums (ForkError, ToolchainError) remain available for progressive adoption;
  no user-visible string changes.

2025-09-24 04:30 user@example.com

Phase 4: adopt logging helpers in runner and merge failure

- Replaced ad-hoc ANSI prints with log_warn_stderr in fork runner guidance lines and
  log_error_stderr for fork merge failure in main.rs, preserving exact messages and colors.
- Completes the Phase 4 logging refinement with consistent wrappers where messages are identical.

2025-09-24 04:05 user@example.com

Fix: silence dead_code warnings for new error enums

- Added #[allow(dead_code)] on ForkError/ToolchainError and their helper functions to
  keep clippy green while broader adoption is pending.

2025-09-24 03:40 user@example.com

Phase 4: finalize error/logging refinement

- Introduced lightweight error enums (ForkError, ToolchainError) in errors.rs along with mapping
  helpers to preserve existing exit codes and user-visible messages.
- Adopted color-aware logging helpers in post-merge paths (info/warn/error) without changing
  any message text.
- Runner decomposition remains complete from earlier steps.

2025-09-24 03:10 user@example.com

Phase 4: error/logging refinement and runner decomposition

- Added minimal color-aware logging helpers (log_info_stderr, log_warn_stderr, log_error_stderr)
  in color.rs to support consistent stderr prints without changing message strings.
- Removed legacy unreachable runner code below the early return in fork/runner.rs; the runner
  remains decomposed into clear preflight/base/snapshot/clones/meta/orchestrator/post-merge steps.

2025-09-24 02:50 user@example.com

Scoring: comprehensive source code assessment and next steps

- Wrote a comprehensive source code scoring to SCORE.md and moved previous
  scoring to SCORE-before.md.
- Proposed actionable next steps in SCORE.md.

2025-09-24 02:35 user@example.com

tests: update notifications policy tests to validated path

- Updated notifications_policy_spec to assert policy errors via the validated wrapper
  (notifications_handle_request), which uses parse_notif_cfg() internally.
- Preserved expected error texts; removed reliance on tokenizer enforcing policy.

2025-09-24 02:20 user@example.com

Phase 3: strict notifications policy consolidation

- Removed all policy checks from parse_notifications_command_config(); it now performs
  tokenization only (YAML -> argv tokens).
- Policy (absolute exec path, strictly-trailing "{args}") remains enforced exclusively
  in parse_notif_cfg() and used by public/proxy wrappers.
- Note for tests: assertions expecting policy errors should call the validated wrapper
  (e.g., notifications_exec_basename()/parse_notif_cfg path) rather than the tokenizer.

2025-09-24 02:00 user@example.com

Fix: restore notifications policy checks for tests

- Reintroduced absolute-path and trailing "{args}" validation into
  parse_notifications_command_config() so existing tests expecting policy
  errors continue to pass.
- Kept parse_notif_cfg() as the single internal policy authority; no behavior
  changes for callers, error texts preserved.

2025-09-24 01:35 user@example.com

Fix: private-interfaces: make parse_notif_cfg private

- Downgraded parse_notif_cfg() visibility to module-private to avoid exposing
  private NotifCfg in signature under -D private-interfaces.
- No behavior changes; notifications policy consolidation remains intact.

2025-09-24 01:20 user@example.com

Phase 3: notifications policy consolidation

- Moved all notifications policy checks into parse_notif_cfg(); tokenization now in
  parse_notifications_command_config() only.
- Added notifications_exec_basename() helper and updated public wrapper to use it.
- Preserved all error texts; proxy and wrapper map structured errors to identical strings.

2025-09-24 00:40 user@example.com

Phase 2: orchestrators implementation and selection integration

- Implemented tmux (Unix), Windows Terminal, PowerShell, and Git Bash/mintty orchestrators.
- Delegated fork pane launch to orchestrators; integrated selection cross-platform.
- Preserved user-visible messages and post-merge guidance; gated automatic post-merge by orchestrator waitability.

Details:
- Added implementations in src/fork/orchestrators/{tmux.rs,windows_terminal.rs,powershell.rs,gitbash_mintty.rs}.
- Updated src/fork/orchestrators/mod.rs to expose orchestrator modules on relevant platforms.
- Refactored src/fork/runner.rs to select and launch via orchestrators; kept guidance strings intact.

2025-09-23 00:30 user@example.com

Phase 1 verification: all tests green

- Verified Phase 1 fully implemented; 246 tests passed, 32 skipped.
- No source code changes; updated SCORE.md with outcomes and next steps.

Details:
- Confirmed docker helpers consolidation, warn prompt helpers, security parser, exit-code mapping.
- Ensured sidecar test runs use /var/tmp target dir to avoid noexec issues.

2025-09-23 00:20 user@example.com

tests: set target dir to /var/tmp for sidecar test runs

- Prefix CARGO_TARGET_DIR=/var/tmp/aifo-target for cargo nextest/cargo test when
  running inside the sidecar (AIFO_EXEC_ID set) to avoid noexec on /workspace.

Details:
- Updated Makefile test target (sidecar branch) to set CARGO_TARGET_DIR only for
  test executions inside the container.

2025-09-23 00:10 user@example.com

Phase 1: central exit-code mapping

- Centralized io::Error -> exit code mapping in a new src/errors.rs helper.
- Updated main.rs and commands/mod.rs to use the shared mapping without changing messages.

Details:
- Added src/errors.rs with exit_code_for_io_error(); re-exported via lib.rs.
- Replaced ad-hoc NotFound mappings with helper (127 for NotFound, 1 otherwise).
- No behavior or strings changed; only mapping centralized per spec.

2025-09-23 00:00 user@example.com

Phase 1: utility consolidation and low-risk refactors

- Consolidated docker helpers to reuse util::fs::{path_pair, ensure_file_exists}.
- Extracted platform-specific warn prompt input helpers in src/ui/warn.rs.
- Introduced docker_security_options_parse helper and reused in banner.rs and doctor.rs.

Details:
- src/docker.rs now imports crate::path_pair and crate::ensure_file_exists; removed local copies.
- src/ui/warn.rs gains warn_input_windows(), warn_input_unix(), warn_input_fallback(); warn_prompt_continue_or_quit delegates to them.
- Added src/util/docker_security.rs with DockerSecurityOptions struct and parser; updated src/util/mod.rs and re-exports in src/lib.rs.
- Replaced manual Docker SecurityOptions parsing in banner.rs and doctor.rs with the shared helper, preserving exact output strings.
