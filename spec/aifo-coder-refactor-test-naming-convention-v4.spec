# AIFO-Coder Test Naming Convention v4
Date: 2025-11-14
Status: Proposed

Purpose

- Consolidate v1, v2, and v3 into a complete, self-contained, consistent specification.
- Standardize test naming and grouping so nextest filters are deterministic and stable.
- Keep Unit tests dockerless and fast on all hosts.
- Provide a precise phased plan with transitional filters to avoid CI breakage.

Scope

- All Rust tests under tests/.
- Makefile/CI filters and docs related to running tests.
- No runtime logic changes; only naming, filters, and minimal gating helpers where needed.

Goals

- Lane membership is obvious from names alone (filenames and function names).
- Integration tests self-skip cleanly when prerequisites are missing (no image pulls).
- E2E tests are #[ignore] by default and only run in dedicated lanes.
- Align Makefile/CI with one consistent filter scheme (target and transitional).
- Optional lint to enforce the naming rules and prevent regressions.

Non-goals

- Change application behavior or image contents.
- Refactor test bodies beyond renames and minimal gating helpers.

--------------------------------------------------------------------------------
Lane definitions (invariants)

- unit
  - Pure logic and lightweight host IO (env/fs); no external processes (no docker/git/curl).
  - No network; never #[ignore].
- int (integration)
  - May spawn subprocesses (aifo-coder CLI, git, docker CLI discovery).
  - Must self-skip cleanly when prerequisites are missing; never pull images implicitly.
  - Not #[ignore] by default.
- e2e (acceptance)
  - Heavy/long-running or “live” container/proxy flows; may start proxy/sidecars and inspect containers.
  - MUST be #[ignore] by default.

--------------------------------------------------------------------------------
Naming rules

1) File names (under tests/)
- unit_*.rs → Unit lane files.
- int_*.rs → Integration lane files.
- e2e_*.rs → E2E lane files.

2) Test function names (inside those files)
- Prefix all #[test] functions with the lane token:
  - unit_* in unit_*.rs
  - int_* in int_*.rs
  - e2e_* in e2e_*.rs
This enables reliable filtering by function name in addition to filenames.

3) Optional suffixes for specificity
- Platform: _linux, _macos, _windows, _unix.
- Transport: _tcp, _uds.
- Area/module: proxy_, http_, toolchain_, registry_, notifications_, fork_, cli_, images_, make_, support_, color_.

Examples
- unit_registry_cache_invalidate
- int_proxy_endpoint_basic_errors_tcp
- e2e_proxy_unix_socket_streaming_linux
- unit_fork_sanitize_label_rules
- int_cli_images_flavor_flag_slim

--------------------------------------------------------------------------------
Helper/gating guidance

- Only int/e2e tests may call:
  - aifo_coder::container_runtime_path()
  - tests/support::docker_image_present(runtime, image)
  - tests/support::have_git() or any std::process::Command spawns
- Prefer small wrapper helpers (optional):
  - skip_if_no_docker() -> bool
  - skip_if_image_missing(runtime, image) -> bool
- NEVER call these helpers from unit tests.
- Integration/E2E tests MUST self-skip quietly when prerequisites are absent (do not pull).

--------------------------------------------------------------------------------
Verification of current inventory and corrections (tailored to provided files)

Observed categories:

E2E (#[ignore] by default; heavy/live flows)
- accept_* files: accept_disconnect.rs, accept_logs_golden.rs, accept_native_http_{tcp,uds}.rs,
  accept_stream_large.rs, accept_wrappers.rs → e2e_* counterparts.
- proxy_streaming_* and proxy_unix_socket.rs when heavy/long: e2e_proxy_streaming_tcp.rs,
  e2e_proxy_unix_socket.rs.
- proxy_streaming_spawn_fail_plain_500.rs, proxy_stream_backpressure.rs → e2e_proxy_*.
- toolchain_rust_volume_ownership.rs, toolchain_rust_image_contents.rs,
  toolchain_rust_bootstrap_exec.rs → e2e_toolchain_*.
- dev_tool_routing.rs, python_venv_activation.rs → e2e_dev_tool_routing_*,
  e2e_python_venv_activation_*.

Integration (spawns CLI or depends on docker/images; self-skip when unavailable)
- proxy_*: concurrency, header_case, endpoint_basic_errors, protocol, negatives_light,
  timeout, large_payload (notifications-only), python_venv precedence, tsc local precedence,
  unix_socket_url (Linux) → int_proxy_*.
- http_*: parsing_tolerance, guardrails, endpoint_routing → int_http_*.
- notify_* and notifications_* (spawn local scripts): notify_proxy.rs, notify_unix_socket.rs,
  notifications_{parse,policy_spec,hardening,unit}.rs → int_notify_*, int_notifications_*.
- shims.rs, shims_notifications.rs (exec shims) → int_shims*, int_shims_notifications.
- preview_* (not present in the list, but any build_*_preview tests): int_preview_*.
- toolchain_* (dry-run/envs/linkers/mounts/path_and_user/networking/bootstrap_wrapper_preview,
  bootstrap_sccache_policy, phase1, cpp, bootstrap_typescript) → int_toolchain_*.
- default_image_regression.rs → int_default_image_regression.rs.
- session_cleanup.rs → int_session_cleanup.rs.
- support_spec.rs (exec aifo-coder) → int_support_spec.rs.
- color_precedence.rs (exec aifo-coder) → int_color_precedence.rs.

Unit (dockerless; pure functions; never #[ignore])
- helpers-like: docker_cmd_edges.rs → unit_shell_escaping_preview_edges.rs.
- command_lock.rs, lock_edges.rs, lock_repo_{hashed_path,scoped_order}.rs → unit_lock_*.
- route_map.rs, rust_image_helpers.rs → unit_*.
- sanitize_label.rs, sanitize_property.rs, fork_sanitize.rs → unit_*.
- images_output.rs, images_output_new_agents.rs (pure formatting) → unit_images_output_*.
- doc_smoke_toolchains_rust.rs → unit_doc_smoke_toolchains_rust.rs.
- windows_orchestrator_cmds.rs → unit_windows_orchestrator_cmds.rs.

Corrections and clarifications:
- notifications_* and shims_* spawn local processes by design → Integration (never Unit).
- dev_tool_routing.rs and python_venv_activation.rs are E2E (ignored); v3 correction stands.
- proxy_large_payload.rs is heavy; classify as E2E (#[ignore]). If a notifications-only variant exists,
  classify as Integration and do not ignore.
- windows_orchestrator_cmds.rs is Unit (pure string/args composition).
- Function prefixes inside tests must match lane membership (unit_/int_/e2e_).

--------------------------------------------------------------------------------
Representative mapping (non-exhaustive; based on provided files)

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
- toolchain_rust_image_contents.rs → e2e_toolchain_rust_image_contents.rs
- toolchain_rust_bootstrap_exec.rs → e2e_toolchain_rust_bootstrap_exec.rs
- e2e_stream_cargo.rs (keep; ensure e2e_* functions)
- dev_tool_routing.rs → e2e_dev_tool_routing_make_tcp_v2.rs
- python_venv_activation.rs → e2e_python_venv_activation_tcp_v2.rs
- shim_embed.rs → e2e_shim_embed.rs
- wrapper_behavior.rs → e2e_wrapper_behavior.rs

Integration
- proxy_concurrency.rs → int_proxy_concurrency_mixed.rs
- proxy_header_case.rs → int_proxy_header_case.rs
- proxy_endpoint_basic_errors.rs → int_proxy_endpoint_basic_errors.rs
- proxy_protocol.rs → int_proxy_protocol.rs
- proxy_negatives_light.rs → int_proxy_negatives_light.rs
- proxy_timeout.rs → int_proxy_timeout_python.rs
- proxy_large_payload.rs (notifications-only variant) → int_proxy_large_payload_notifications.rs
- proxy_python_venv.rs → int_proxy_python_venv_precedence.rs
- proxy_tsc_local.rs → int_proxy_tsc_local_precedence.rs
- proxy_unix_socket_url.rs (Linux) → int_proxy_unix_socket_url_linux.rs
- http_parsing_tolerance.rs → int_http_parsing_tolerance.rs
- http_guardrails.rs → int_http_guardrails.rs
- http_endpoint_routing.rs → int_http_endpoint_routing.rs
- notify_proxy.rs → int_notify_proxy_tcp.rs
- notify_unix_socket.rs (Linux) → int_notify_unix_socket_linux.rs
- notifications_parse.rs → int_notifications_parse.rs
- notifications_policy_spec.rs → int_notifications_policy_spec.rs
- notifications_hardening.rs → int_notifications_hardening.rs
- notifications_unit.rs → int_notifications_handle_request.rs
- shims.rs → int_shims.rs
- shims_notifications.rs → int_shims_notifications.rs
- toolchain_cpp.rs → int_toolchain_cpp.rs
- toolchain_phase1.rs → int_toolchain_phase1.rs
- toolchain_rust_envs.rs → int_toolchain_rust_envs.rs
- toolchain_rust_linkers.rs → int_toolchain_rust_linkers.rs
- toolchain_rust_mounts.rs → int_toolchain_rust_mounts.rs
- toolchain_rust_path_and_user.rs → int_toolchain_rust_path_and_user_unix.rs
- toolchain_rust_networking.rs → int_toolchain_rust_networking_linux.rs
- toolchain_rust_bootstrap_wrapper_preview.rs → int_toolchain_rust_bootstrap_wrapper_preview.rs
- toolchain_rust_bootstrap_sccache_policy.rs → int_toolchain_rust_bootstrap_sccache_policy.rs
- toolchain_bootstrap_typescript.rs → int_toolchain_bootstrap_typescript.rs
- default_image_regression.rs → int_default_image_regression.rs
- session_cleanup.rs → int_session_cleanup.rs
- support_spec.rs → int_support_spec.rs
- color_precedence.rs → int_color_precedence.rs

Unit
- docker_cmd_edges.rs → unit_shell_escaping_preview_edges.rs
- command_lock.rs → unit_command_lock.rs
- lock_edges.rs → unit_lock_edges.rs
- lock_repo_hashed_path.rs → unit_lock_repo_hashed_path.rs
- lock_repo_scoped_order.rs → unit_lock_repo_scoped_order.rs
- route_map.rs → unit_route_map.rs
- rust_image_helpers.rs → unit_rust_image_helpers.rs
- sanitize_label.rs → unit_sanitize_label.rs
- sanitize_property.rs → unit_sanitize_property.rs
- fork_sanitize.rs → unit_fork_sanitize.rs
- images_output.rs → unit_images_output.rs
- images_output_new_agents.rs → unit_images_output_new_agents.rs
- doc_smoke_toolchains_rust.rs → unit_doc_smoke_toolchains_rust.rs
- windows_orchestrator_cmds.rs → unit_windows_orchestrator_cmds.rs

Notes:
- tests/support (helpers) and tests/common are exempt from lane filename/function prefix rules.

--------------------------------------------------------------------------------
Makefile and CI filters

Target-state expressions (after file and function renames complete):
- Integration lane:
  - -E 'test(/^int_/)'
- Acceptance/E2E lane:
  - -E 'test(/^e2e_/)'
- Unit lane (default):
  - Run all non-ignored tests; unit tests comprise the bulk and remain dockerless.

Transitional expressions (Phases 1–3):

Integration (broad to include legacy names)
- -E 'test(/^int_/)|test(/^test_(proxy_|http_|cli_|toolchain_|notify_|notifications_|preview_|fork_|support_|default_image_regression|session_cleanup|color_precedence|python_venv_activation|dev_tool_routing)/)'

Acceptance/E2E (capture legacy accept_* and streaming/unix patterns)
- -E 'test(/^e2e_/)|test(/^accept_/)|test(/^test_(proxy_streaming_|e2e_|wrapper_behavior|shim_embed|node_named_cache_ownership_stamp_files|toolchain_rust_volume_ownership|python_venv_activation|dev_tool_routing|proxy_unix_socket)/)'

Notes:
- Expressions operate on test function names; ensure legacy test functions match these patterns until Phase 3.
- After Phase 5 (Cleanup), drop all aliases and rely solely on /^int_/ and /^e2e_/.

--------------------------------------------------------------------------------
Function prefix policy (mandatory)

- All #[test] functions MUST begin with the lane token matching the filename:
  - unit_* in unit_*.rs
  - int_* in int_*.rs
  - e2e_* in e2e_*.rs
- Preserve existing #[ignore] on E2E tests; ensure only e2e_* tests are #[ignore].
- Preserve #[cfg(...)] platform guards.

--------------------------------------------------------------------------------
Phased implementation plan

Phase 0 — Spec landing and inventory (today)
- Land this v4 spec at spec/aifo-coder-refactor-test-naming-convention-v4.spec.
- Inventory tests/:
  - List files not starting with unit_/int_/e2e_.
  - List #[test] functions not starting with unit_/int_/e2e_.
  - Flag any Unit tests that spawn external processes; reclassify them to Integration.
  - Flag any Integration tests incorrectly #[ignore]; either un-ignore or reclassify to E2E if heavy.

Phase 1 — Filters and documentation (transitional)
- Update Makefile nextest expressions to the Transitional expressions above.
- Update docs/TESTING.md:
  - The three lanes and naming scheme (files and function prefixes).
  - Transitional and target-state nextest filter expressions.
  - Gating policy (self-skip, no pulls, dockerless Unit lane).
  - Clarify preview_* → Integration rule and notifications/shims → Integration rule.
- No file renames yet; CI remains green.

Phase 2 — File renames (git mv only; keep bodies intact)
- Rename files per Representative mapping (batch by area: proxy/http → preview → toolchain → cli/make → unit/registry → acceptance/e2e).
- Do not change test function names in this phase.
- Validate locally:
  - make check passes on dockerless hosts.
  - make test-integration-suite self-skips reliably where docker/images are missing.
  - make check-e2e runs ignored tests only (when explicitly invoked).

Phase 3 — Function renames (deterministic filtering)
- Rename #[test] functions in each renamed file:
  - test_xxx → unit_xxx, int_xxx, e2e_xxx respectively.
- Ensure only e2e_* tests have #[ignore].
- Preserve #[cfg] guards and test bodies.
- Re-run validations from Phase 2.

Phase 4 — CI workflows alignment
- Update .github/workflows/*:
  - Use transitional filters until Phase 3 lands on main.
  - Move to target-state filters:
    - Integration: -E 'test(/^int_/)'
    - E2E: -E 'test(/^e2e_/)' with --run-ignored ignored-only
  - Ensure unit lane (“make check”) stays dockerless on all builders.

Phase 5 — Cleanup (drop aliases)
- Remove legacy patterns (accept_*, proxy_*, http_*, dev_tool_routing_*, python_venv_activation_*) from Makefile/CI filters.
- Ensure docs reference only the final scheme and filters.

Phase 6 — Optional enforcement (lint)
- Add a CI lint step validating:
  - File names start with unit_/int_/e2e_ (except tests/support, tests/common).
  - All #[test] function names begin with unit_/int_/e2e_ prefixes.
  - Unit files contain no obvious process spawns (grep for Command::new/std::process::Command).
- Provide remediation hints with expected rename predictions in CI logs.

Phase 7 — Ongoing maintenance
- Require lane-aligned filenames and function prefixes in code review for all new tests.
- Keep Integration tests self-skip reliable and never pull images.
- Keep dockerless Unit invariant enforced.

--------------------------------------------------------------------------------
Acceptance criteria

- Unit lane (make check) passes on hosts without docker or git installed.
- Integration lane (make test-integration-suite) passes or self-skips cleanly (no image pulls).
- E2E lane (make check-e2e) runs ignored-heavy tests and passes in provisioned CI.
- After Phase 5:
  - nextest and CI filters rely solely on unit_/int_/e2e_ prefixes.
  - All test files and functions follow naming rules.

--------------------------------------------------------------------------------
Risk and mitigation

- CI red due to filter changes while PRs are in-flight:
  - Mitigate via transitional aliases in Phase 1 and Phase 4.
- Misclassification (e.g., Unit test invoking docker or git):
  - Inventory in Phase 0; fix mapping; add lint in Phase 6.
- Large rename diffs causing conflicts:
  - Split into small batches; land frequently; separate file renames from function renames.
- Platform-specific drifts (Unix-only tests):
  - Preserve #[cfg] guards; suffix names with _linux/_unix/_windows where appropriate.

--------------------------------------------------------------------------------
Operational notes

- tests/support and tests/common are helper modules and exempt from filename/function prefix rules.
- Starts proxy or sidecars → Integration or E2E (E2E if heavy/long/ignored).
- Spawns external commands (docker, git, shell) → Integration or E2E (never Unit).
- Pure mapping/parsing/cache logic → Unit.

--------------------------------------------------------------------------------
Change log

- 2025-11-14 (v4):
  - Consolidated v1/v2/v3 into final, self-contained specification; clarified transitional filters.
  - Reaffirmed notifications_* and shims_* as Integration (spawn local processes).
  - Corrected python_venv_activation.rs and dev_tool_routing.rs as E2E (#[ignore]).
  - Clarified proxy_large_payload.rs as E2E; windows_orchestrator_cmds.rs as Unit.
  - Tightened function-prefix enforcement and CI target-state filters; added enforcement lint.
