# AIFO-Coder Test Naming Convention v1
Date: 2025-11-14
Status: Proposed → Adopted after rollout

This specification standardizes test naming and grouping across the repository to ensure:
- Lane membership (unit, integration, E2E) is inferable from filenames and test function names.
- nextest filters are deterministic and unambiguous.
- CI lanes run the appropriate subsets without fragile ad-hoc patterns.
- Unit tests remain fast and dockerless on all hosts.

The plan below verifies the previous proposal, corrects inconsistencies, fills gaps, and defines a phased rollout.

--------------------------------------------------------------------------------
Goals and non-goals

- Goals
  - Make lane membership obvious from names alone.
  - Keep “unit” tests dockerless and without external subprocesses.
  - Ensure “integration” tests self-skip cleanly when docker or local images are absent.
  - Ensure “E2E/acceptance” tests are #[ignore] by default and only run in dedicated lanes.
  - Align Makefile and CI with a single consistent filter scheme.

- Non-goals
  - Change any runtime logic of the application.
  - Change existing test behaviors beyond renames and early-skip gating.
  - Consolidate or refactor test bodies (only naming/filters and minimal gating helpers if needed).

--------------------------------------------------------------------------------
Lane definitions (invariants)

- unit: Pure logic and lightweight host IO (env/fs) only; no external processes (no git/curl/docker); no network; never #[ignore].
- int (integration): May spawn subprocesses (git, aifo-coder CLI), detect docker CLI, or depend on local docker images; not #[ignore] by default; MUST self-skip cleanly if prerequisites are missing.
- e2e (acceptance): Heavy/long-running or “live” container/proxy flows; MUST be #[ignore] by default.

If a test starts the proxy or sidecars → int or e2e (usually e2e if heavy or long/streaming).
If a test shells out to git or docker CLI → integration or e2e (never unit).
If a test requires docker images to be present → integration or e2e (self-skip if absent).
Registry prefix/source/cache tests → unit (pure in-process state and disk).

--------------------------------------------------------------------------------
Naming rules

1) File names (under tests/)
- unit_*.rs for unit lane files.
- int_*.rs for integration lane files.
- e2e_*.rs for E2E lane files.

2) Test function names inside files
- Prefix all test function names with the same lane token:
  - unit_* in unit_*.rs
  - int_* in int_*.rs
  - e2e_* in e2e_*.rs
This enables filtering by function name (e.g., test(/^int_/)) in addition to filename filtering.

3) Optional suffixes for specificity
- Platform: _linux, _macos, _windows, _unix.
- Transport: _tcp, _uds.
- Area/module: e.g., proxy_, toolchain_, registry_, notifications_, fork_, cli_, images_, http_.

Example names
- unit_registry_cache_invalidate
- int_proxy_endpoint_basic_errors_tcp
- e2e_proxy_unix_socket_streaming_linux
- unit_fork_sanitize_label_rules
- int_cli_images_flavor_flag_slim

--------------------------------------------------------------------------------
Verification of current inventory and corrections

Validated classification principles (with representative examples from the current tree):

- Unit (dockerless, no external commands)
  - helpers.rs → unit_helpers.rs (pure helpers)
  - docker_cmd_edges.rs → unit_shell_escaping_preview_edges.rs (string quoting only)
  - command_lock.rs, lock_edges.rs, lock_repo_hashed_path.rs, lock_repo_scoped_order.rs → unit_*
  - sanitize_label.rs, sanitize_property.rs, fork_sanitize.rs → unit_fork_*
  - route_map.rs → unit_route_map.rs
  - rust_image_helpers.rs → unit_rust_image_helpers.rs
  - repo_uses_lfs_quick.rs, repo_lfs_quick.rs → unit_repo_uses_lfs_quick.rs / unit_repo_lfs_quick.rs
  - registry_* (all env/cache/source tests) → unit_registry_*
  - shims.rs, shims_notifications.rs, shim_writer.rs → unit_*
  - apparmor_env_disable.rs → unit_apparmor_env_disable.rs
  - windows_orchestrator_cmds.rs → unit_windows_orchestrator_cmds.rs (CORRECTION: pure string building → unit)
  - images_output.rs, images_output_new_agents.rs → unit_*
  - doc_smoke_toolchains_rust.rs → unit_doc_smoke_toolchains_rust.rs (CORRECTION: docs-only, no docker)

- Integration (external CLI or docker presence checks, but light; self-skip when unavailable)
  - proxy_smoke*.rs, proxy_timeout.rs, proxy_endpoint_basic_errors.rs, proxy_protocol.rs, http_parsing_tolerance.rs, http_guardrails.rs, proxy_concurrency.rs, proxy_allowlist.rs, proxy_header_case.rs, proxy_large_payload.rs, http_endpoint_routing.rs → int_*
  - notify_proxy.rs, notify_unix_socket.rs, notifications.rs, notifications_policy_spec.rs, notifications_hardening.rs → int_*
  - preview_* files (workspace/mounts/network/proxy_env/unix_mount/shim_dir/path_policy/git_sign/container_name/hostname_env/no_git_mutation) → int_preview_* (CORRECTION: previews depend on docker path discovery → integration)
  - toolchain_cpp.rs (dry-run), toolchain_phase1.rs, toolchain_rust_path_and_user.rs, toolchain_rust_envs.rs, toolchain_rust_linkers.rs, toolchain_rust_bootstrap_wrapper_preview.rs, toolchain_rust_bootstrap_sccache_policy.rs → int_*
  - toolchain_mappings.rs → unit_toolchain_mappings.rs (CORRECTION: pure mapping logic → unit)
  - session_cleanup.rs → int_session_cleanup.rs
  - CLI and Make helpers: cli_images*.rs, cli_dry_run.rs, cli_toolchain_*.rs, cli_cache_clear*.rs, cli_doctor*.rs, support_spec.rs, make_targets_rust_toolchain.rs, make_dry_run_rust_toolchain.rs → int_*
  - default_image_regression.rs → int_default_image_regression.rs (checks docker CLI/image presence; self-skip)
  - fork_* that spawn git (snapshot/clean/plan/list/merge/clone/base_info/autoclean) → int_fork_*

- E2E (#[ignore], heavy/live container or streaming)
  - accept_logs_golden.rs, accept_stream_large.rs → e2e_proxy_logs_golden.rs / e2e_proxy_stream_large.rs
  - e2e_stream_cargo.rs → e2e_proxy_stream_cargo.rs
  - proxy_unix_socket.rs, proxy_streaming_tcp.rs, proxy_streaming_spawn_fail_plain_500.rs, proxy_streaming_slow_consumer_disconnect.rs → e2e_proxy_*
  - accept_native_http_tcp.rs, accept_native_http_uds.rs → e2e_native_http_{tcp,uds}.rs
  - toolchain_live.rs, toolchain_go_cache_env.rs, toolchain_rust_volume_ownership.rs → e2e_toolchain_*
  - node_cache_stamp.rs → e2e_node_named_cache_ownership_stamp_files.rs
  - accept_override_shim_dir.rs → e2e_shim_override_dir.rs
  - accept_disconnect.rs → e2e_proxy_client_disconnect.rs
  - accept_wrappers.rs → e2e_wrappers_auto_exit.rs
  - e2e_fork_tmux_smoke.rs (already e2e_), e2e_fork_wt_smoke.rs → e2e_fork_windows_terminal_smoke_opt_in.rs
  - toolchain_rust_image_contents.rs → e2e_toolchain_rust_image_contents.rs

Consistency fixes and gap closure
- PREV ERROR: Some preview_* tests were previously treated as unit; corrected to integration because they depend on docker CLI discovery (even when self-skipping).
- PREV ERROR: windows_orchestrator_cmds.rs was previously assumed integration in the draft; corrected to unit (pure string/args composition).
- GAP: doc_smoke_toolchains_rust.rs not previously classified; added to unit.
- GAP: Function prefixes inside tests must also reflect lane membership (unit_/int_/e2e_). This spec makes it mandatory.
- GAP: nextest expressions in Makefile must be updated to match new prefixes with a transitional period (see Phases).
- GAP: CI workflows must be updated accordingly; temporary aliasing retained for one release window.

--------------------------------------------------------------------------------
Makefile and CI filters (target state)

- Integration suite (not ignored by default):
  - nextest expression: test(/^int_/)
- Acceptance/E2E suite (ignored by default):
  - nextest expression: test(/^e2e_/)

Transitional compatibility (one-cycle grace)
- Integration: test(/^int_/)|test(/^test_proxy_/)|test(/^test_tsc_/)|test(/^test_e2e_stream_cargo_/)
- Acceptance/E2E: test(/^e2e_/)|test(/^accept_/)

Unit lane
- Default make check runs all non-ignored tests; unit tests should comprise the bulk of fast tests and have no docker/git dependencies.

--------------------------------------------------------------------------------
Helper/gating guidance

- Prefer tests/support helpers for early skip and IO:
  - container_runtime_path() used only in int/e2e; NEVER in unit tests.
  - docker_image_present(runtime, image) to self-skip quietly; NEVER pull.
  - have_git() helper for git detection; NEVER called in unit tests.
- Consider adding small wrapper helpers (optional):
  - skip_if_no_docker() returning bool for int/e2e tests.
  - skip_if_image_missing(runtime, image) for image-gated int/e2e tests.

--------------------------------------------------------------------------------
Function rename policy

- All test function names MUST begin with the lane token matching the filename:
  - unit_* in unit_*.rs
  - int_* in int_*.rs
  - e2e_* in e2e_*.rs
- Keep descriptive tails and platform/transport suffixes intact.
- Preserve existing #[ignore] attributes on e2e_ tests.
- Preserve all #[cfg(...)] platform guards.

--------------------------------------------------------------------------------
Phased implementation plan

Phase 0 — Spec and inventory (this document)
- Land this spec at spec/aifo-coder-refactor-test-naming-convention-v1.spec.
- Inventory all tests/ files and confirm classification above (scriptable via ripgrep/awk).
- Identify any unit tests that call external processes or implicitly depend on docker; reclassify them to integration.

Phase 1 — Filters and documentation (transitional)
- Update Makefile nextest expressions to include both new prefixes and transitional aliases:
  - Integration: -E "test(/^int_/)|test(/^test_proxy_/)|test(/^test_tsc_/)|test(/^test_e2e_stream_cargo_/)"
  - E2E: -E "test(/^e2e_/)|test(/^accept_/)"
- Update docs/TESTING.md with:
  - The three lanes and naming scheme.
  - The new filtering expressions (and the temporary aliases).
  - The gating policy (self-skip, no pulls, dockerless unit lane).

Phase 2 — File renames
- git mv tests/*.rs to the new lane-prefixed filenames per inventory.
- Keep module internals unchanged for now (only the filename changes).
- Validate:
  - make check (unit/fast) passes on a dockerless host.
  - make test-integration-suite self-skips as designed where docker or images are missing.
  - make check-e2e runs ignored E2E tests under CI lanes only.

Phase 3 — Function renames
- Within each renamed file, rename test functions to lane-prefixed names (unit_/int_/e2e_).
- Keep #[ignore] on e2e_ tests only.
- Validate again as in Phase 2.

Phase 4 — CI workflows alignment
- Update .github/workflows/linux-smoke.yml and linux-smoke-extended.yml:
  - Replace legacy patterns with the new expressions.
  - Retain transitional aliases for one CI cycle to avoid red builds across PR merges.

Phase 5 — Cleanup
- Remove transitional aliases from Makefile and CI filters.
- Ensure docs/TESTING.md only references the final scheme and filters.

Phase 6 — Optional enforcement
- Add a simple lint step in CI that validates:
  - All tests under tests/ start with unit_/int_/e2e_ filenames.
  - All test functions start with unit_/int_/e2e_ prefixes.
  - tests/support and tests/common are exempted.

--------------------------------------------------------------------------------
Acceptance criteria

- Unit lane (make check) passes on hosts without docker or git installed.
- Integration lane (make test-integration-suite) passes or self-skips cleanly (no pulls).
- E2E lane (make check-e2e) runs ignored-heavy tests and passes when prerequisites are met.
- nextest and CI filters rely solely on unit_/int_/e2e_ prefixes (after transitional period).
- All test files and functions follow the naming convention with appropriate platform/transport suffixes where relevant.

--------------------------------------------------------------------------------
Risk and mitigation

- Risk: CI red due to filter change while PRs in-flight.
  - Mitigation: transitional aliases in Phase 1 and Phase 4.
- Risk: Misclassified tests (e.g., unit test invoking docker path).
  - Mitigation: Phase 0 inventory and post-rename validation; add CI lint in Phase 6.
- Risk: Long files with many function renames cause merge conflicts.
  - Mitigation: split renames into small batches; land frequently.

--------------------------------------------------------------------------------
Change log

- 2025-11-14: Initial spec and rollout plan adopted. Corrections applied:
  - preview_* reclassified to integration.
  - windows_orchestrator_cmds.rs clarified as unit.
  - doc_smoke_toolchains_rust.rs added to unit.
  - Mandatory lane prefixes for test function names.
