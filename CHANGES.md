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
