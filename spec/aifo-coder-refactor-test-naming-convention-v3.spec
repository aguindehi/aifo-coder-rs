# AIFO-Coder Test Naming Convention v3
Date: 2025-11-14
Status: Proposed

Purpose

- Validate the v2 scheme against the current repository contents provided above.
- Close gaps found during v2 rollout (notably notifications_* classification, python_venv_activation.rs, dev_tool_routing.rs).
- Tighten the lane invariants and function-prefix requirements.
- Define precise transitional filters for nextest that reflect the current tree and minimize CI breakage.
- Provide a concrete phased implementation plan and enforcement guidance.

Recap of v2 (summary)

- Lanes:
  - unit_*.rs: pure logic, no external processes, dockerless.
  - int_*.rs: may spawn subprocesses (cli, git, docker CLI), self-skip when not available.
  - e2e_*.rs: heavy, live container/proxy flows; #[ignore] by default.
- Test function names in each file also start with unit_/int_/e2e_.
- Makefile/CI use nextest expressions to select lanes, with transitional aliases.

Validation of current tree (observed 2025-11-14)

- Many files remain with legacy prefixes:
  - Acceptance/E2E-like: accept_*.rs, proxy_streaming_*.rs, proxy_unix_socket.rs, accept_native_http_{tcp,uds}.rs, accept_wrappers.rs, accept_disconnect.rs, node_cache_stamp.rs, toolchain_rust_volume_ownership.rs, wrapper_behavior.rs, shim_embed.rs.
  - Integration-like: proxy_*.rs, http_*.rs, preview_*.rs, notify_* and notifications_*.rs, toolchain_* (dry-run/envs/linkers/mounts), cli_*.rs, make_*.rs, support_spec.rs, default_image_regression.rs, python_venv_activation.rs, proxy_unix_socket_url.rs, session_cleanup.rs, color_precedence.rs.
  - Unit-like: helpers.rs, docker_cmd_edges.rs, command_lock.rs, lock_*; route_map.rs, rust_image_helpers.rs, images_output*.rs; many registry_* files; doc_smoke_toolchains_rust.rs; windows_orchestrator_cmds.rs.

Gaps, inconsistencies, and corrections (v3)

1) Notifications classification (correction)
- Issue: v2 suggested notifications_unit.rs → unit_*. However notifications_handle_request() spawns a subprocess (the configured command), violating the “no external processes” rule for unit lane.
- Correction:
  - notifications_unit.rs is Integration: rename to int_notifications_handle_request.rs.
  - notifications_parse.rs, notifications_policy_spec.rs, notifications_hardening.rs also belong to Integration (they spawn or prepare to spawn local scripts).
  - shims_notifications.rs (execs a shim) and shims.rs (execs shims) are Integration.

2) dev_tool_routing.rs and python_venv_activation.rs (correction)
- Observation: Both tests are marked #[ignore] and perform live proxy/sidecar flows.
- Correction:
  - dev_tool_routing.rs → e2e_dev_tool_routing_* (functions e2e_*).
  - python_venv_activation.rs → e2e_python_venv_activation_tcp_v2.rs (functions e2e_*).
- Rationale: #[ignore] + live container/proxy behavior categorizes them as E2E.

3) Wrapper and embed checks (confirmation)
- wrapper_behavior.rs and shim_embed.rs are #[ignore], require docker/images and inspect containers → E2E.

4) Preview tests (confirmation)
- All preview_* tests (build_docker_cmd/build_sidecar_* previews) depend on docker CLI path/env, not necessarily images; classify as Integration (int_preview_*). They must self-skip cleanly when docker path is unavailable.

5) Proxy/HTTP tests (confirmation)
- proxy_* (smoke/timeout/protocol/allowlist/header-case/large-payload/etc.) and http_* (routing/guardrails/parsing) start proxy and bind sockets (without necessarily starting sidecars), thus Integration. Keep not #[ignore] and self-skip when docker/runtime prerequisites are missing.

6) Windows orchestrator tests (confirmation)
- windows_orchestrator_cmds.rs constructs command lines/strings; pure logic: Unit (unit_windows_orchestrator_cmds.rs).

7) Unit tests invoking external processes (clarification)
- Unit lane prohibits any external subprocess. If a test currently spawns sh/git/docker/curl (even in a tempdir), reclassify to Integration.

8) Function prefixes (mandatory)
- All test functions in unit_*.rs must start with unit_.
- All test functions in int_*.rs must start with int_.
- All test functions in e2e_*.rs must start with e2e_ (and keep #[ignore]).
- This is required to achieve deterministic nextest filtering and decouple CI from filenames alone.

Naming rules (unchanged, reiterated)

- Files under tests/:
  - unit_*.rs for Unit
  - int_*.rs for Integration
  - e2e_*.rs for E2E (#[ignore] by default)
- Test function names:
  - unit_*, int_*, e2e_* respectively.
- Optional suffixes to disambiguate:
  - Platform: _linux, _macos, _windows, _unix.
  - Transport: _tcp, _uds.
  - Area: proxy_, http_, toolchain_, registry_, notifications_, fork_, cli_, images_, make_, support_.

Representative mapping for this repository (non-exhaustive but tailored to the provided files)

E2E (#[ignore] by default)
- accept_logs_golden.rs → e2e_proxy_logs_golden.rs
- accept_stream_large.rs → e2e_proxy_stream_large.rs
- accept_native_http_tcp.rs → e2e_native_http_tcp.rs
- accept_native_http_uds.rs → e2e_native_http_uds.rs
- accept_wrappers.rs → e2e_wrappers_auto_exit.rs
- accept_disconnect.rs → e2e_proxy_client_disconnect.rs
- proxy_unix_socket.rs → e2e_proxy_unix_socket.rs
- proxy_streaming_tcp.rs → e2e_proxy_streaming_tcp.rs
- proxy_streaming_spawn_fail_plain_500.rs → e2e_proxy_streaming_spawn_fail_plain_500.rs
- proxy_stream_backpressure.rs → e2e_proxy_stream_backpressure.rs
- node_cache_stamp.rs → e2e_node_named_cache_ownership_stamp_files.rs
- toolchain_rust_volume_ownership.rs → e2e_toolchain_rust_volume_ownership.rs
- dev_tool_routing.rs → e2e_dev_tool_routing_make_tcp_v2.rs
- python_venv_activation.rs → e2e_python_venv_activation_tcp_v2.rs
- wrapper_behavior.rs → e2e_wrapper_behavior.rs
- shim_embed.rs → e2e_shim_embed.rs
- e2e_stream_cargo.rs (keep; ensure functions start with e2e_)

Integration
- proxy_smoke.rs, proxy_smoke_more.rs → int_proxy_smoke*.rs
- proxy_timeout.rs → int_proxy_timeout_python.rs
- proxy_negatives_light.rs → int_proxy_negatives_light.rs
- proxy_protocol.rs → int_proxy_protocol.rs
- proxy_endpoint_basic_errors.rs → int_proxy_endpoint_basic_errors.rs
- proxy_header_case.rs → int_proxy_header_case.rs
- proxy_large_payload.rs (ignored) → e2e_proxy_large_payload.rs (heavier; mark #[ignore])
- proxy_unix_socket_url.rs (Linux) → int_proxy_unix_socket_url_linux.rs
- http_parsing_tolerance.rs → int_http_parsing_tolerance.rs
- http_guardrails.rs → int_http_guardrails.rs
- http_endpoint_routing.rs → int_http_endpoint_routing.rs
- notify_unix_socket.rs (Linux) → int_notify_unix_socket_linux.rs
- notify_proxy.rs → int_notify_proxy_tcp.rs
- notifications.rs → int_notifications_cmd_routes.rs
- notifications_parse.rs → int_notifications_parse.rs
- notifications_policy_spec.rs → int_notifications_policy_spec.rs
- notifications_hardening.rs → int_notifications_hardening.rs
- notifications_unit.rs → int_notifications_handle_request.rs
- shims.rs → int_shims.rs
- shims_notifications.rs → int_shims_notifications.rs
- preview_* (api/env/git_sign/hostname/mounts/network/no_git_mutation/path_contains_shim_dir/per_pane_state/proxy_env/shim_dir/unix_mount/user_flag_unix/workspace) → int_preview_*.rs
- cli_* (images, images_flavor, images_flavor_flag, images_registry_env, dry_run, toolchain_dry_run, toolchain_flags_reporting, toolchain_override_precedence, cache_clear, cache_clear_effect, doctor, doctor_details, doctor_workspace) → int_cli_*.rs
- make_* (make_dry_run_rust_toolchain.rs, publish_dry_run_rust_toolchain.rs) → int_make_*.rs
- default_image_regression.rs → int_default_image_regression.rs
- session_cleanup.rs → int_session_cleanup.rs
- color_precedence.rs → int_color_precedence.rs
- support_spec.rs → int_support_spec.rs
- toolchain_cpp.rs (dry-run) → int_toolchain_cpp.rs
- toolchain_phase1.rs → int_toolchain_phase1.rs
- toolchain_rust_envs.rs → int_toolchain_rust_envs.rs
- toolchain_rust_linkers.rs → int_toolchain_rust_linkers.rs
- toolchain_rust_mounts.rs → int_toolchain_rust_mounts.rs
- toolchain_rust_path_and_user.rs → int_toolchain_rust_path_and_user_unix.rs
- toolchain_rust_networking.rs → int_toolchain_rust_networking_linux.rs
- toolchain_rust_bootstrap_wrapper_preview.rs → int_toolchain_rust_bootstrap_wrapper_preview.rs
- toolchain_rust_bootstrap_sccache_policy.rs → int_toolchain_rust_bootstrap_sccache_policy.rs
- toolchain_bootstrap_typescript.rs → int_toolchain_bootstrap_typescript.rs
- toolchain_rust_image_contents.rs (#[ignore]) → e2e_toolchain_rust_image_contents.rs
- toolchain_rust_bootstrap_exec.rs (#[ignore]) → e2e_toolchain_rust_bootstrap_exec.rs
- proxy_python_venv.rs (local .venv precedence) → int_proxy_python_venv_precedence.rs
- proxy_tsc_local.rs (local tsc) → int_proxy_tsc_local_precedence.rs
- proxy_concurrency.rs → int_proxy_concurrency_mixed.rs
- proxy_error_semantics.rs → int_proxy_error_semantics.rs
- proxy_large_payload_notifications (if present) → int_proxy_large_payload_notifications.rs

Unit
- helpers.rs → unit_helpers.rs
- docker_cmd_edges.rs → unit_shell_escaping_preview_edges.rs
- command_lock.rs → unit_command_lock.rs
- lock_edges.rs → unit_lock_edges.rs
- lock_repo_hashed_path.rs → unit_lock_repo_hashed_path.rs
- lock_repo_scoped_order.rs → unit_lock_repo_scoped_order.rs
- route_map.rs → unit_route_map.rs
- rust_image_helpers.rs → unit_rust_image_helpers.rs
- images_output.rs, images_output_new_agents.rs → unit_images_output*.rs
- registry_* (env, quiet, override, probe, cache, source, determinism variants) → unit_registry_*.rs
- doc_smoke_toolchains_rust.rs → unit_doc_smoke_toolchains_rust.rs
- windows_orchestrator_cmds.rs → unit_windows_orchestrator_cmds.rs
- json/format-only helpers under tests/common remain as helpers; not lane-classified.

Makefile and CI filters

Target-state expressions (after renames and function prefixes complete):
- Integration suite:
  - -E 'test(/^int_/)'
- Acceptance/E2E suite:
  - -E 'test(/^e2e_/)'
- Unit lane (default make check):
  - Run all non-ignored tests.

Transitional expressions (aligned to current function naming patterns and legacy filenames):
- Integration (keep broad to reduce misses until Phase 3):
  - -E 'test(/^int_/)|test(/^test_(proxy_|http_|cli_|toolchain_|notify_|notifications_|preview_|fork_|support_|default_image_regression)/)'
- Acceptance/E2E:
  - -E 'test(/^e2e_/)|test(/^accept_/)|test(/^test_(proxy_streaming_|e2e_|wrapper_behavior|shim_embed|node_named_cache_ownership_stamp_files|toolchain_rust_volume_ownership|python_venv_activation|dev_tool_routing)/)'
Notes:
- These filter regexes operate on test function names; they intentionally match common “test_proxy_*”, “accept_*” and similar patterns present today.
- Drop transitional aliases after Phase 5 (cleanup).

Lane invariants (strict)

- unit:
  - No external processes (no git/curl/docker or even local shell scripts).
  - No network. No #[ignore].
- int:
  - May spawn subprocesses (aifo-coder CLI, git, shell, docker CLI discovery).
  - Must self-skip cleanly if prerequisites are missing; never pull images implicitly.
  - Not #[ignore] by default.
- e2e:
  - Heavy/long-running, multiple sidecars, image/content validation, container filesystem assertions, etc.
  - MUST be #[ignore] by default.

Function prefixes (required)

- Rename test functions for determinism:
  - unit_* inside unit_*.rs
  - int_* inside int_*.rs
  - e2e_* inside e2e_*.rs
- Preserve #[ignore] on e2e_* only.
- Keep #[cfg(...)] platform guards.

Phased implementation plan

Phase 0 — Spec freeze and inventory (today)
- Land this document at spec/aifo-coder-refactor-test-naming-convention-v3.spec.
- Inventory the tests/ tree:
  - List files not starting with unit_/int_/e2e_.
  - List functions not starting with unit_/int_/e2e_.
  - Flag unit tests that spawn external processes (reclassify to Integration).
  - Flag Integration tests incorrectly marked #[ignore] (either un-ignore or reclassify to E2E).
- Produce a CSV or md checklist mapping “current → new name” and “function rename needed (y/n)”.

Phase 1 — Filters and docs (transitional)
- Update Makefile nextest expressions to the Transitional expressions above (do not remove current aliases until all renames are merged).
- Update docs/TESTING.md:
  - Reiterate lane definitions and naming conventions.
  - Show transitional and target-state filters.
  - Clarify preview_* → Integration rule.
  - Emphasize dockerless unit lane.
- No file renames yet; ensure CI stays green.

Phase 2 — File renames (git mv only)
- Batch git mv files per the Representative mapping:
  - Batch by area (proxy/http first; preview next; toolchain; cli/make; registry/unit; acceptance/e2e).
  - Do not change test function names yet.
- Validate locally:
  - make check (unit) must pass on dockerless host.
  - make test-integration-suite self-skips cleanly if docker/images are not available.
  - make check-e2e runs ignored tests only.
- If needed, widen transitional filters temporarily to include any newly exposed legacy prefixes.

Phase 3 — Function renames (deterministic filtering)
- In each renamed file, rename #[test] functions to lane-prefixed names:
  - test_xxx → unit_xxx in unit_*.rs
  - test_xxx → int_xxx in int_*.rs
  - test_xxx → e2e_xxx in e2e_*.rs
- Keep #[ignore] only on e2e_*.
- Preserve #[cfg] guards and test bodies.
- Re-run Phase 2 validations.

Phase 4 — CI workflows alignment
- Update .github/workflows/* to:
  - Use transitional filters first (until Phase 3 is merged on main).
  - Move to target-state (-E 'test(/^int_/)' and -E 'test(/^e2e_/)' with --run-ignored ignored-only) once function renames land.
  - Ensure unit lane (“make check”) continues to be dockerless on all builders.

Phase 5 — Cleanup (drop aliases)
- Remove legacy patterns (accept_*, proxy_*, http_*, dev_tool_routing_*, etc.) from Makefile/CI filters.
- Docs: reflect only the new scheme and final filters.

Phase 6 — Optional lint enforcement
- Add a CI lint step that validates:
  - File names under tests/ start with unit_/int_/e2e_ (except tests/support and tests/common).
  - All test functions start with unit_/int_/e2e_ as appropriate.
  - Unit files contain no obvious process spawns (grep for Command::new, std::process::Command, or shell shebangs in fixtures).
- Provide remediation hints in CI logs with the expected rename pattern.

Phase 7 — Ongoing maintenance
- Require lane-aligned filename and function prefixes in code review for all new tests.
- Keep Integration tests self-skip reliable and never pull images.
- Keep dockerless Unit invariant enforced.

Acceptance criteria

- Unit lane (make check) passes on hosts without docker or git installed.
- Integration lane (make test-integration-suite) passes or self-skips cleanly (no pulls).
- E2E lane (make check-e2e) runs only ignored tests and passes in appropriately provisioned CI.
- After Phase 5:
  - nextest and CI filters rely solely on unit_/int_/e2e_ prefixes.
  - All test files and functions follow the naming rules.

Known edge-cases and decisions (v3)

- notifications_* and shims_* tests spawn local processes by design. They are Integration even if they do not require docker.
- proxy_large_payload.rs is long-running; classify as E2E and keep #[ignore].
- python_venv_activation.rs and dev_tool_routing.rs are E2E (#[ignore]); v3 corrects v2 which suggested Integration for python_venv_activation.
- windows_orchestrator_cmds.rs remains Unit (pure string/arg composition).
- tests/support and tests/common are helper modules and exempt from filename/function prefix rules.

Change log

- 2025-11-14 (v3):
  - Corrected notifications_* classification to Integration (spawns processes).
  - Corrected python_venv_activation.rs and dev_tool_routing.rs to E2E.
  - Reiterated function prefix requirement and added stronger transitional filters matching current function naming.
  - Clarified expectations for proxy_large_payload.rs (E2E).
  - Kept windows_orchestrator_cmds.rs as Unit; preview_* as Integration.
