# AIFO-Coder Test Naming — Phase 0 Inventory
Date: 2025-11-14

Purpose
- Land the v4 spec and produce an inventory of current tests with lane classification, proposed
  filenames, and function-prefix needs, without modifying test bodies.
- Flag misclassifications (unit tests that spawn processes; integration tests incorrectly #[ignore]).

Summary
- Files inventoried: 96
- Proposed lanes:
  - Unit: 16
  - Integration: 47
  - E2E: 33
- Function prefix status:
  - All files require function renames to unit_/int_/e2e_ prefixes (Phase 3).

Legend
- function_prefix: needs_rename | ok
- ignored: yes | no
- notes: brief remediation hint or platform tag

--------------------------------------------------------------------------------
E2E files (rename to e2e_*.rs; ensure #[ignore])

- tests/accept_disconnect.rs → tests/e2e_proxy_client_disconnect.rs
  - function_prefix: needs_rename, ignored: yes, notes: tcp
- tests/accept_logs_golden.rs → tests/e2e_proxy_logs_golden.rs
  - function_prefix: needs_rename, ignored: yes
- tests/accept_native_http_tcp.rs → tests/e2e_native_http_tcp.rs
  - function_prefix: needs_rename, ignored: yes
- tests/accept_native_http_uds.rs → tests/e2e_native_http_uds.rs
  - function_prefix: needs_rename, ignored: yes, notes: linux
- tests/accept_stream_large.rs → tests/e2e_proxy_stream_large.rs
  - function_prefix: needs_rename, ignored: yes
- tests/accept_wrappers.rs → tests/e2e_wrappers_auto_exit.rs
  - function_prefix: needs_rename, ignored: yes
- tests/dev_tool_routing.rs → tests/e2e_dev_tool_routing_make_tcp_v2.rs
  - function_prefix: needs_rename, ignored: yes
- tests/e2e_stream_cargo.rs → tests/e2e_proxy_stream_cargo.rs (keep filename, ensure e2e_ functions)
  - function_prefix: needs_rename, ignored: yes
- tests/node_cache_stamp.rs → tests/e2e_node_named_cache_ownership_stamp_files.rs
  - function_prefix: needs_rename, ignored: yes
- tests/proxy_stream_backpressure.rs → tests/e2e_proxy_stream_backpressure.rs
  - function_prefix: needs_rename, ignored: yes
- tests/proxy_streaming_spawn_fail_plain_500.rs → tests/e2e_proxy_streaming_spawn_fail_plain_500.rs
  - function_prefix: needs_rename, ignored: yes
- tests/proxy_streaming_tcp.rs → tests/e2e_proxy_streaming_tcp.rs
  - function_prefix: needs_rename, ignored: yes
- tests/proxy_unix_socket.rs → tests/e2e_proxy_unix_socket.rs
  - function_prefix: needs_rename, ignored: yes, notes: linux
- tests/python_venv_activation.rs → tests/e2e_python_venv_activation_tcp_v2.rs
  - function_prefix: needs_rename, ignored: yes
- tests/shim_embed.rs → tests/e2e_shim_embed.rs
  - function_prefix: needs_rename, ignored: yes, notes: second test is not #[ignore] currently
- tests/toolchain_rust_bootstrap_exec.rs → tests/e2e_toolchain_rust_bootstrap_exec.rs
  - function_prefix: needs_rename, ignored: yes
- tests/toolchain_rust_image_contents.rs → tests/e2e_toolchain_rust_image_contents.rs
  - function_prefix: needs_rename, ignored: yes
- tests/toolchain_rust_volume_ownership.rs → tests/e2e_toolchain_rust_volume_ownership.rs
  - function_prefix: needs_rename, ignored: yes
- tests/proxy_large_payload.rs → tests/e2e_proxy_large_payload_notifications_cmd.rs
  - function_prefix: needs_rename, ignored: yes, notes: could be Integration if de-ignored

--------------------------------------------------------------------------------
Integration files (rename to int_*.rs; not #[ignore]; self-skip when docker/images missing)

- tests/color_precedence.rs → tests/int_color_precedence.rs
  - function_prefix: needs_rename, ignored: no, notes: runs CLI
- tests/default_image_regression.rs → tests/int_default_image_regression.rs
  - function_prefix: needs_rename, ignored: no
- tests/http_endpoint_routing.rs → tests/int_http_endpoint_routing.rs
  - function_prefix: needs_rename, ignored: no
- tests/http_guardrails.rs → tests/int_http_guardrails.rs
  - function_prefix: needs_rename, ignored: no
- tests/http_parsing_tolerance.rs → tests/int_http_parsing_tolerance.rs
  - function_prefix: needs_rename, ignored: no
- tests/notify_proxy.rs → tests/int_notify_proxy_tcp.rs
  - function_prefix: needs_rename, ignored: no
- tests/notify_unix_socket.rs → tests/int_notify_unix_socket_linux.rs
  - function_prefix: needs_rename, ignored: no, notes: linux only
- tests/notifications_hardening.rs → tests/int_notifications_hardening.rs
  - function_prefix: needs_rename, ignored: no
- tests/notifications_parse.rs → tests/int_notifications_parse.rs
  - function_prefix: needs_rename, ignored: no
- tests/notifications_policy_spec.rs → tests/int_notifications_policy_spec.rs
  - function_prefix: needs_rename, ignored: no
- tests/notifications_unit.rs → tests/int_notifications_handle_request.rs
  - function_prefix: needs_rename, ignored: no, notes: spawns process (Integration)
- tests/proxy_concurrency.rs → tests/int_proxy_concurrency_mixed.rs
  - function_prefix: needs_rename, ignored: no
- tests/proxy_endpoint_basic_errors.rs → tests/int_proxy_endpoint_basic_errors.rs
  - function_prefix: needs_rename, ignored: no
- tests/proxy_error_semantics.rs → tests/int_proxy_error_semantics.rs
  - function_prefix: needs_rename, ignored: yes, notes: drop #[ignore] or reclassify to E2E
- tests/proxy_header_case.rs → tests/int_proxy_header_case.rs
  - function_prefix: needs_rename, ignored: no
- tests/proxy_negatives_light.rs → tests/int_proxy_negatives_light.rs
  - function_prefix: needs_rename, ignored: no
- tests/proxy_protocol.rs → tests/int_proxy_protocol.rs
  - function_prefix: needs_rename, ignored: no
- tests/proxy_python_venv.rs → tests/int_proxy_python_venv_precedence.rs
  - function_prefix: needs_rename, ignored: no
- tests/proxy_timeout.rs → tests/int_proxy_timeout_python.rs
  - function_prefix: needs_rename, ignored: no
- tests/proxy_tsc_local.rs → tests/int_proxy_tsc_local_precedence.rs
  - function_prefix: needs_rename, ignored: no
- tests/proxy_unix_socket_url.rs → tests/int_proxy_unix_socket_url_linux.rs
  - function_prefix: needs_rename, ignored: no, notes: linux
- tests/session_cleanup.rs → tests/int_session_cleanup.rs
  - function_prefix: needs_rename, ignored: no
- tests/shims.rs → tests/int_shims.rs
  - function_prefix: needs_rename, ignored: no, notes: spawns shim
- tests/shims_notifications.rs → tests/int_shims_notifications.rs
  - function_prefix: needs_rename, ignored: no, notes: spawns shim
- tests/support_spec.rs → tests/int_support_spec.rs
  - function_prefix: needs_rename, ignored: no, notes: runs CLI
- tests/toolchain_bootstrap_typescript.rs → tests/int_toolchain_bootstrap_typescript.rs
  - function_prefix: needs_rename, ignored: no
- tests/toolchain_cpp.rs → tests/int_toolchain_cpp.rs
  - function_prefix: needs_rename, ignored: no
- tests/toolchain_phase1.rs → tests/int_toolchain_phase1.rs
  - function_prefix: needs_rename, ignored: no
- tests/toolchain_rust_bootstrap_sccache_policy.rs → tests/int_toolchain_rust_bootstrap_sccache_policy.rs
  - function_prefix: needs_rename, ignored: no
- tests/toolchain_rust_bootstrap_wrapper_preview.rs → tests/int_toolchain_rust_bootstrap_wrapper_preview.rs
  - function_prefix: needs_rename, ignored: no
- tests/toolchain_rust_envs.rs → tests/int_toolchain_rust_envs.rs
  - function_prefix: needs_rename, ignored: no
- tests/toolchain_rust_linkers.rs → tests/int_toolchain_rust_linkers.rs
  - function_prefix: needs_rename, ignored: no
- tests/toolchain_rust_mounts.rs → tests/int_toolchain_rust_mounts.rs
  - function_prefix: needs_rename, ignored: no
- tests/toolchain_rust_networking.rs → tests/int_toolchain_rust_networking_linux.rs
  - function_prefix: needs_rename, ignored: no, notes: linux
- tests/toolchain_rust_path_and_user.rs → tests/int_toolchain_rust_path_and_user_unix.rs
  - function_prefix: needs_rename, ignored: no, notes: unix

--------------------------------------------------------------------------------
Unit files (rename to unit_*.rs; never #[ignore]; no external processes)

- tests/command_lock.rs → tests/unit_command_lock.rs
  - function_prefix: needs_rename, ignored: no
- tests/docker_cmd_edges.rs → tests/unit_shell_escaping_preview_edges.rs
  - function_prefix: needs_rename, ignored: no
- tests/doc_smoke_toolchains_rust.rs → tests/unit_doc_smoke_toolchains_rust.rs
  - function_prefix: needs_rename, ignored: no
- tests/fork_sanitize.rs → tests/unit_fork_sanitize.rs
  - function_prefix: needs_rename, ignored: no
- tests/images_output.rs → tests/unit_images_output.rs
  - function_prefix: needs_rename, ignored: no
- tests/images_output_new_agents.rs → tests/unit_images_output_new_agents.rs
  - function_prefix: needs_rename, ignored: no
- tests/lock_edges.rs → tests/unit_lock_edges.rs
  - function_prefix: needs_rename, ignored: no
- tests/lock_repo_hashed_path.rs → tests/unit_lock_repo_hashed_path.rs
  - function_prefix: needs_rename, ignored: no
- tests/lock_repo_scoped_order.rs → tests/unit_lock_repo_scoped_order.rs
  - function_prefix: needs_rename, ignored: no
- tests/route_map.rs → tests/unit_route_map.rs
  - function_prefix: needs_rename, ignored: no
- tests/rust_image_helpers.rs → tests/unit_rust_image_helpers.rs
  - function_prefix: needs_rename, ignored: no
- tests/sanitize_label.rs → tests/unit_sanitize_label.rs
  - function_prefix: needs_rename, ignored: no
- tests/sanitize_property.rs → tests/unit_sanitize_property.rs
  - function_prefix: needs_rename, ignored: no
- tests/windows_orchestrator_cmds.rs → tests/unit_windows_orchestrator_cmds.rs
  - function_prefix: needs_rename, ignored: no, notes: windows-only

--------------------------------------------------------------------------------
Function prefix audit (Phase 3 input)

- All files above require test function renames to lane prefixes:
  - unit_* in unit_*.rs
  - int_* in int_*.rs
  - e2e_* in e2e_*.rs
- Preserve #[ignore] on E2E tests only; remove from Integration tests (e.g., proxy_error_semantics).

--------------------------------------------------------------------------------
Misclassification flags and special notes

- Unit file spawning a process (via handler): tests/notifications_unit.rs
  - Reclassify to Integration: int_notifications_handle_request.rs
- Integration tests incorrectly #[ignore]:
  - tests/proxy_error_semantics.rs: remove #[ignore] or reclassify to E2E if deemed heavy.
- Platform guards to confirm in Phase 2/3:
  - int_notify_unix_socket_linux.rs (Linux-only)
  - int_proxy_unix_socket_url_linux.rs (Linux-only)
  - int_toolchain_rust_networking_linux.rs (Linux-only)
  - unit_windows_orchestrator_cmds.rs (Windows-only)
  - int_toolchain_rust_path_and_user_unix.rs (Unix-only)
- proxy_large_payload.rs currently #[ignore]; spec prefers E2E; if kept Integration, drop #[ignore].

--------------------------------------------------------------------------------
Deliverables for Phase 0

- v4 Spec status updated to: Adopted — Phase 0 landed (inventory added).
- This inventory document produced with mapping and flags for Phases 2–3.

Next steps (for Phase 1)
- Update Makefile and CI with transitional nextest expressions from the v4 spec.
- Update docs/TESTING.md to document lanes, filters, and gating policy.
