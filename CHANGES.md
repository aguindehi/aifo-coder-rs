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
