# Test lanes and toggles

This repository organizes tests into three pragmatic lanes. The suite is designed to
run fast by default (no Docker required), and to gate heavier integration/E2E tests
cleanly when Docker or local images are unavailable.

Lanes

- Unit/fast (default nextest run; no Docker required)
  - Parser/utility/unit tests.
  - Run with: make check or make test.
  - Should pass on hosts without Docker.

- Integration (Docker present; local images only)
  - Proxy and sidecar integration that depend on Docker and specific images.
  - Tests gate on:
    - aifo_coder::container_runtime_path() (Docker CLI available)
    - tests/support::docker_image_present(runtime, image) (image present locally)
  - Skips cleanly if requirements are not met (no pulls).
  - Run with: make test-integration-suite

- Acceptance/E2E (ignored by default)
  - Heavy tests and end-to-end paths; include unix:// transport (Linux-only).
  - Marked #[ignore] by default.
  - Run with: make test-acceptance-suite or make check-e2e

Gating helpers

- aifo_coder::container_runtime_path()
  - Returns Ok(path) when Docker CLI is available in PATH; otherwise Err(NotFound).

- tests/support::docker_image_present(runtime, image)
  - Returns true only if the image is present locally; avoids pulling.

Default test images

- Node: support::default_node_test_image()
  - Defaults to node:22-bookworm-slim (override via AIFO_CODER_TEST_NODE_IMAGE)

- Python: support::default_python_test_image()
  - Defaults to python:3.12-slim (override via AIFO_CODER_TEST_PY_IMAGE or AIFO_CODER_TEST_PYTHON_IMAGE)

Important environment toggles

- AIFO_CODER_TEST_ENABLE_NOTIFY_SPAWN_500=1
  - Enables notifications_exec_spawn_error_500 test (forces spawn error path).

- AIFO_CODER_TEST_ASSERT_RUST_ENV_NORMATIVE=1
  - Asserts normative rust env replacements in env_blocklist_rust_normative test.

- AIFO_TOOLEEXEC_TIMEOUT_SECS, AIFO_TOOLEEXEC_MAX_SECS
  - Proxy runtime limits for exec:
    - v1 buffered: maps initial INT timeout to 504/124 as per spec.
    - v2 streaming: trailers reflect child exit code; disconnect behavior is distinct.

- AIFO_TOOLEEXEC_USE_UNIX=1 (Linux-only)
  - Enables unix:// proxy transport for tests that support UDS.

- AIFO_TEST_LOG_PATH
  - When set, proxy logs tee to the given file (used by acceptance/integration tests
    to assert disconnect/escalation sequences without dup2 tricks).

- AIFO_SHIM_EXIT_ZERO_ON_DISCONNECT, AIFO_SHIM_DISCONNECT_WAIT_SECS
  - Shim behavior on streaming disconnect:
    - Default exit zero on disconnect unless opted out via AIFO_SHIM_EXIT_ZERO_ON_DISCONNECT=0.
    - Wait seconds printed and honored via AIFO_SHIM_DISCONNECT_WAIT_SECS.

- NO_COLOR vs AIFO_CODER_COLOR / CLI --color
  - NO_COLOR disables color unconditionally.
  - CLI flags override env; env override is honored when CLI did not set a mode.

- AIFO_CODER_TEST_DISABLE_DOCKER=1
  - Forces Docker detection to return NotFound and causes docker/E2E tests to self-skip.
  - CI sets this to ensure unit/host-only tests run on builder images without a Docker daemon.

How to run

- Default (unit/fast):
  - make check
  - or: make test

- Integration/E2E (Docker and images present):
  - make check-e2e
  - make test-integration-suite
  - make test-acceptance-suite


Coverage

- HTML:
  - make coverage-html

- lcov.info:
  - make coverage-lcov

Naming conventions

- Files under tests/:
  - unit_*.rs for Unit lane (dockerless, no external processes).
  - int_*.rs for Integration lane (may spawn CLI; self-skip when prerequisites missing).
  - e2e_*.rs for E2E lane (#[ignore] by default).
- Test function names:
  - unit_* in unit_*.rs
  - int_* in int_*.rs
  - e2e_* in e2e_*.rs
- Helpers under tests/support and tests/common are exempt from lane filename/function prefix rules.
- preview_* tests are Integration (int_preview_*): they depend on docker CLI path/env discovery.
- notifications_* and shims_* tests are Integration: they spawn local processes/shims.

Filters (transitional → target)

- Transitional filters: removed (Phase 5 complete).
- Target-state filters (after file/function renames):
  - Integration: -E 'test(/^int_/)'
  - Acceptance/E2E: -E 'test(/^e2e_/)' with --run-ignored ignored-only
  - Unit lane: default “make check” runs all non-ignored tests (dockerless).

Notes

- Tests skip cleanly when Docker is unavailable or images are not present. This avoids
  unexpected pulls and keeps CI deterministic across lanes.
- Image defaults and helper usage have been consolidated across the suite to reduce drift
  and flakiness. Prefer tests/support helpers for URL→port parsing and raw HTTP/TCP sends.
