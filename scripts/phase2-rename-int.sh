#!/bin/sh
# Phase 2 — Integration file renames (git mv only; keep bodies intact)
# How to run:
#   sh scripts/phase2-rename-int.sh
set -eu

git mv -v tests/color_precedence.rs tests/int_color_precedence.rs
git mv -v tests/default_image_regression.rs tests/int_default_image_regression.rs
git mv -v tests/http_endpoint_routing.rs tests/int_http_endpoint_routing.rs
git mv -v tests/http_guardrails.rs tests/int_http_guardrails.rs
git mv -v tests/http_parsing_tolerance.rs tests/int_http_parsing_tolerance.rs

git mv -v tests/notify_proxy.rs tests/int_notify_proxy_tcp.rs
git mv -v tests/notify_unix_socket.rs tests/int_notify_unix_socket_linux.rs

git mv -v tests/notifications_hardening.rs tests/int_notifications_hardening.rs
git mv -v tests/notifications_parse.rs tests/int_notifications_parse.rs
git mv -v tests/notifications_policy_spec.rs tests/int_notifications_policy_spec.rs
git mv -v tests/notifications_unit.rs tests/int_notifications_handle_request.rs

git mv -v tests/proxy_concurrency.rs tests/int_proxy_concurrency_mixed.rs
git mv -v tests/proxy_endpoint_basic_errors.rs tests/int_proxy_endpoint_basic_errors.rs
git mv -v tests/proxy_error_semantics.rs tests/int_proxy_error_semantics.rs
git mv -v tests/proxy_header_case.rs tests/int_proxy_header_case.rs
git mv -v tests/proxy_negatives_light.rs tests/int_proxy_negatives_light.rs
git mv -v tests/proxy_protocol.rs tests/int_proxy_protocol.rs
git mv -v tests/proxy_python_venv.rs tests/int_proxy_python_venv_precedence.rs
git mv -v tests/proxy_timeout.rs tests/int_proxy_timeout_python.rs
git mv -v tests/proxy_tsc_local.rs tests/int_proxy_tsc_local_precedence.rs
git mv -v tests/proxy_unix_socket_url.rs tests/int_proxy_unix_socket_url_linux.rs

git mv -v tests/session_cleanup.rs tests/int_session_cleanup.rs

git mv -v tests/shims.rs tests/int_shims.rs
git mv -v tests/shims_notifications.rs tests/int_shims_notifications.rs

git mv -v tests/support_spec.rs tests/int_support_spec.rs

git mv -v tests/toolchain_bootstrap_typescript.rs tests/int_toolchain_bootstrap_typescript.rs
git mv -v tests/toolchain_cpp.rs tests/int_toolchain_cpp.rs
git mv -v tests/toolchain_phase1.rs tests/int_toolchain_phase1.rs
git mv -v tests/toolchain_rust_bootstrap_sccache_policy.rs tests/int_toolchain_rust_bootstrap_sccache_policy.rs
git mv -v tests/toolchain_rust_bootstrap_wrapper_preview.rs tests/int_toolchain_rust_bootstrap_wrapper_preview.rs
git mv -v tests/toolchain_rust_envs.rs tests/int_toolchain_rust_envs.rs
git mv -v tests/toolchain_rust_linkers.rs tests/int_toolchain_rust_linkers.rs
git mv -v tests/toolchain_rust_mounts.rs tests/int_toolchain_rust_mounts.rs
git mv -v tests/toolchain_rust_networking.rs tests/int_toolchain_rust_networking_linux.rs
git mv -v tests/toolchain_rust_path_and_user.rs tests/int_toolchain_rust_path_and_user_unix.rs
