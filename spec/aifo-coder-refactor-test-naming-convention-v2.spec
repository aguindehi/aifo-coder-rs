# AIFO-Coder Test Naming Convention v2
Date: 2025-11-14
Status: Proposed → Adopted after rollout

Purpose

- Validate the v1 scheme against the current repository contents.
- Close gaps found during v1 rollout (file/function prefix drift, preview vs unit classification).
- Define precise filters, enforcement, and a safe migration plan with transitional aliases.
- Keep test lanes stable across hosts (dockerless unit lane).

Summary of v1

- Filenames under tests/ use lane prefixes:
  - unit_*.rs (no external processes; dockerless)
  - int_*.rs (spawns CLI or depends on docker/images; self-skip when not available)
  - e2e_*.rs (heavy, live container/proxy flows; #[ignore] by default)
- Test function names in each file also start with the same lane token.
- Makefile/CI use nextest expressions to select lanes.

Validation of current tree (2025-11-14)

- Present file groups observed:
  - Acceptance/E2E style files still named accept_* and some proxy_* streaming tests:
    - accept_*.rs, proxy_streaming_*.rs, proxy_unix_socket.rs, accept_native_http_{tcp,uds}.rs,
      accept_wrappers.rs, accept_disconnect.rs, accept_logs_golden.rs, accept_stream_large.rs,
      e2e_stream_cargo.rs (already v2-compliant filename).
  - Integration style files without int_ prefix:
    - proxy_*.rs (smoke/timeout/protocol/header-case/allowlist/large-payload),
      http_* (routing/guardrails/parsing), dev_tool_routing.rs, session_cleanup.rs,
      notify_* and notifications.rs (proxy endpoints), preview_* (docker previews),
      toolchain_* (dry-run, envs, linkers, mounts), cli_* (doctor/images/cache),
      make_* (Makefile-related).
  - Unit style files without unit_ prefix:
    - helpers.rs, docker_cmd_edges.rs, command_lock.rs, lock_* (paths/edges),
      route_map.rs, rust_image_helpers.rs, repo_*_quick.rs, sanitize_*.rs,
      notifications_unit.rs, shims.rs, shims_notifications.rs, shim_writer.rs,
      windows_orchestrator_cmds.rs, doc_smoke_toolchains_rust.rs, images_output*.rs,
      registry_* (pure env/cache/source/probe abstraction and determinism).

Corrections and clarifications (v2)

- Preview tests (build_docker_cmd, build_sidecar_*_preview) are Integration, not Unit:
  - Rationale: they rely on docker path discovery and environment-driven behavior
    (container_runtime_path), even when run in dry-run mode.
  - Action: ensure preview_* and related tests move to int_preview_* filenames and
    test function names are prefixed int_.

- Proxy endpoint/routing/HTTP tolerance/guardrails tests are Integration:
  - They start the proxy, bind sockets and exercise request parsing (docker path
    and/or sidecar presence is probed). Filenames should be int_http_* or int_proxy_*.

- Acceptance/E2E tests are consistently e2e_*:
  - All accept_* and long-running proxy streaming tests must be renamed to e2e_* and
    functions to e2e_*, preserving #[ignore].

- windows_orchestrator_cmds.rs is Unit:
  - Pure string/args composition without external processes. Keep as unit_windows_*.

- Unit tests must not call container_runtime_path() or spawn external processes:
  - Any unit_* file doing so must be reclassified to Integration.

- Test function prefixes must match their lane:
  - Files renamed to unit_/int_/e2e_ MUST also rename test functions to unit_*/int_*/e2e_*.

Naming rules (unchanged, reiterated)

- File names under tests/:
  - unit_*.rs, int_*.rs, e2e_*.rs.
- Test function names in those files:
  - unit_*, int_*, e2e_* respectively.
- Optional suffixes:
  - Platforms: _linux, _macos, _windows, _unix.
  - Transport: _tcp, _uds.
  - Area/module: proxy_, toolchain_, registry_, notifications_, fork_, cli_, images_, http_.

Lane invariants

- unit:
  - Pure logic and lightweight host IO only (env/fs), no external processes (no git/curl/docker),
    no network, never #[ignore].
- int:
  - May spawn subprocesses (aifo-coder CLI, git), detect docker CLI, or depend on local docker images;
    not #[ignore] by default; MUST self-skip cleanly if prerequisites are missing.
- e2e:
  - Heavy/long-running or live container/proxy flows; MUST be #[ignore] by default.

Representative mapping (non-exhaustive; exact rename list to be generated during Phase 2)

- E2E (acceptance):
  - accept_logs_golden.rs → e2e_proxy_logs_golden.rs
  - accept_stream_large.rs → e2e_proxy_stream_large.rs
  - e2e_stream_cargo.rs (keep; ensure functions start with e2e_)
  - proxy_unix_socket.rs → e2e_proxy_unix_socket.rs
  - proxy_streaming_tcp.rs → e2e_proxy_streaming_tcp.rs
  - proxy_streaming_spawn_fail_plain_500.rs → e2e_proxy_streaming_spawn_fail_plain_500.rs
  - proxy_streaming_slow_consumer_disconnect.rs → e2e_proxy_streaming_slow_consumer_disconnect.rs
  - accept_native_http_{tcp,uds}.rs → e2e_native_http_{tcp,uds}.rs
  - accept_override_shim_dir.rs → e2e_shim_override_dir.rs
  - accept_disconnect.rs → e2e_proxy_client_disconnect.rs
  - accept_wrappers.rs → e2e_wrappers_auto_exit.rs
  - toolchain_rust_image_contents.rs → e2e_toolchain_rust_image_contents.rs
  - node_cache_stamp.rs → e2e_node_named_cache_ownership_stamp_files.rs
  - e2e_fork_tmux_smoke.rs (keep), e2e_fork_wt_smoke.rs → e2e_fork_windows_terminal_smoke_opt_in.rs

- Integration:
  - proxy_smoke*.rs, proxy_timeout.rs, proxy_negatives_light.rs,
    proxy_endpoint_basic_errors.rs, proxy_protocol.rs, http_parsing_tolerance.rs,
    http_guardrails.rs, proxy_concurrency.rs, proxy_allowlist.rs, proxy_header_case.rs,
    proxy_large_payload.rs, http_endpoint_routing.rs → int_* (area-appropriate).
  - dev_tool_routing.rs → int_dev_tool_routing_*.rs
  - preview_* and docker_preview_mounts.rs → int_preview_* (all previews)
  - notify_proxy.rs, notify_unix_socket.rs, notifications.rs, notifications_policy_spec.rs,
    notifications_hardening.rs → int_notifications_* (policy, cmd routes).
  - toolchain_cpp.rs (dry-run), toolchain_phase1.rs, toolchain_rust_path_and_user.rs,
    toolchain_rust_envs.rs, toolchain_rust_linkers.rs,
    toolchain_rust_bootstrap_wrapper_preview.rs,
    toolchain_rust_bootstrap_sccache_policy.rs,
    toolchain_rust_networking.rs, toolchain_rust_optional_mounts.rs,
    toolchain_rust_sccache.rs → int_toolchain_rust_* / int_toolchain_cpp_*.
  - session_cleanup.rs → int_session_cleanup.rs
  - CLI and Makefile helpers: cli_images*.rs, cli_dry_run.rs, cli_toolchain_*.rs,
    cli_cache_clear*.rs, cli_doctor*.rs, support_spec.rs, make_targets_rust_toolchain.rs,
    make_dry_run_rust_toolchain.rs, publish_dry_run_rust_toolchain.rs → int_cli_* / int_make_*.
  - default_image_regression.rs → int_default_image_regression.rs
  - proxy_unix_socket_url.rs → int_proxy_unix_socket_url_linux.rs
  - python_venv_activation.rs → int_python_venv_activation_* (TCP v2 routing)

- Unit:
  - helpers.rs → unit_helpers.rs
  - docker_cmd_edges.rs → unit_shell_escaping_preview_edges.rs
  - command_lock.rs → unit_command_lock.rs
  - lock_edges.rs, lock_repo_hashed_path.rs, lock_repo_scoped_order.rs → unit_lock_*.rs
  - sanitize_label.rs, sanitize_property.rs, fork_sanitize.rs → unit_fork_sanitize_*.rs
  - route_map.rs → unit_route_map.rs
  - rust_image_helpers.rs → unit_rust_image_helpers.rs
  - repo_uses_lfs_quick.rs, repo_lfs_quick.rs → unit_repo_uses_lfs_quick.rs / unit_repo_lfs_quick.rs
  - notifications_unit.rs → unit_notifications_handle_request.rs
  - shims.rs, shims_notifications.rs, shim_writer.rs → unit_shims*.rs
  - windows_orchestrator_cmds.rs → unit_windows_orchestrator_cmds.rs
  - doc_smoke_toolchains_rust.rs → unit_doc_smoke_toolchains_rust.rs
  - images_output.rs, images_output_new_agents.rs → unit_images_output*.rs
  - registry_* (env/cache/source, quiet variants, precedence, determinism, abstraction) → unit_registry_*.rs
  - auth_authorization_value_matches.rs → unit_auth_authorization_value_matches.rs

Makefile and CI filters

Target state (after renames complete):
- Integration suite:
  - -E 'test(/^int_/)'
- Acceptance/E2E suite:
  - -E 'test(/^e2e_/)'
- Unit lane (default make check):
  - Run all non-ignored tests; unit_ comprise the bulk.

Transitional state (during rename window):
- Integration:
  - -E 'test(/^int_/)|test(/^test_proxy_/)|test(/^test_tsc_/)|test(/^accept_/)|test(/^proxy_/)|test(/^http_/)|test(/^dev_tool_routing_/)'
- Acceptance/E2E:
  - -E 'test(/^e2e_/)|test(/^accept_/)|test(/^proxy_streaming_/)|test(/^accept_native_http_/)|test(/^accept_wrappers_/)|test(/^accept_disconnect_/)'

Function prefixes

- Within each file, all #[test] functions MUST be renamed:
  - unit_* in unit_* files, int_* in int_* files, e2e_* in e2e_* files.
- Preserve existing #[ignore] on e2e_ tests.
- Preserve #[cfg(...)] platform guards.

Self-skip and gating guidance

- Integration/E2E tests MUST self-skip when prerequisites are missing:
  - Use container_runtime_path() detection; never pull images implicitly.
  - Use tests/support::docker_image_present(...) for image presence checks.
  - Use have_git() helper for git detection; never call it from unit tests.

Enforcement (lint)

- Add an optional CI lint that checks:
  - File names under tests/ start with unit_/int_/e2e_ (except tests/support, tests/common).
  - Each test function name starts with the matching lane token.
- Allow a temporary allowlist for legacy names during the transitional window to avoid blocking merges.

Phased implementation plan

Phase 0 — Spec freeze and inventory (today)
- Land this document (v2) at spec/aifo-coder-refactor-test-naming-convention-v2.spec.
- Script inventory (ripgrep/awk) to list all tests that:
  - do not start with unit_/int_/e2e_ (files and functions),
  - call container_runtime_path(), have_git(), or spawn Command::new(...).

Phase 1 — Filters and documentation (transitional)
- Update Makefile targets that run nextest to include the transitional expressions above.
- Update docs/TESTING.md to:
  - Reiterate lane definitions and naming conventions.
  - Show transitional and target-state nextest expressions.
  - Clarify that preview_* tests belong to Integration.
  - Emphasize dockerless unit lane.
- No file renames yet; CI stays green.

Phase 2 — File renames (git mv only; no code edits yet)
- Rename files per the representative mapping to unit_/int_/e2e_ prefixes.
- Do not change test function names in this phase.
- Validate locally:
  - make check (unit) passes on dockerless host.
  - make test-integration-suite self-skips when docker/images are not available.
  - make check-e2e runs ignored tests only under explicit suite.

Phase 3 — Test function renames (deterministic filtering)
- In each renamed file, rename #[test] functions to lane-prefixed names (unit_/int_/e2e_).
- Keep #[ignore] on e2e_ tests only.
- Preserve #[cfg] platform guards and existing test bodies.
- Validate as in Phase 2.

Phase 4 — CI workflows alignment
- Update .github/workflows/* (linux-smoke.yml, linux-smoke-extended.yml) to:
  - Use the transitional expressions first, then move to target-state filters as soon as the rename PR is merged.
  - Ensure E2E workflows explicitly use -E 'test(/^e2e_/)' and set --run-ignored ignored-only.

Phase 5 — Cleanup (drop aliases)
- Remove legacy filter aliases (accept_*, proxy_*, http_* prefixes) from Makefile/CI.
- Ensure docs reflect only the new scheme.

Phase 6 — Optional lint enforcement
- Introduce a CI step to validate filename and function prefixes (allowlist tests/support, tests/common).
- Fail fast on drift; provide remediation hints in CI logs.

Phase 7 — Ongoing maintenance
- For any newly added tests, require lane-aligned filename and function prefixes in code review.
- Keep self-skip paths reliable and dockerless-unit invariant enforced.

Acceptance criteria

- Unit lane (make check) passes on hosts without docker or git installed.
- Integration lane (make test-integration-suite) passes or cleanly self-skips (no pulls).
- E2E lane (make check-e2e) runs ignored-heavy tests and passes when prerequisites are met.
- After Phase 5:
  - nextest and CI filters rely solely on unit_/int_/e2e_ prefixes.
  - All tests under tests/ follow filename and function naming rules.

Risk and mitigation

- CI breakage during transition:
  - Use transitional filters (Phase 1) before renames; flip to target-state after merges.
- Misclassification (e.g., unit test invoking docker path):
  - Inventory in Phase 0; fix in mapping; add lint in Phase 6.
- Large rename diffs causing conflicts:
  - Split into batches; land frequently; function renames in separate commits (Phase 3).

Operational notes

- Keep tests/support and tests/common as helpers exempt from filename prefix rules.
- When in doubt:
  - Starts proxy or sidecars → Integration or E2E (long/heavy/ignored).
  - Spawns external commands (docker, git) → Integration/E2E (never Unit).
  - Pure mapping/parsing/cache logic → Unit.

Change log

- 2025-11-14 (v2):
  - Validated v1 against current tree; clarified preview→Integration rule.
  - Locked function prefix requirement; added transitional filters.
  - Added explicit mapping guidance and phased plan with lint enforcement.
