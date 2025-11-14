#!/bin/sh
# Phase 2 — E2E file renames (git mv only; keep bodies intact)
# How to run:
#   sh scripts/phase2-rename-e2e.sh
set -eu

git mv -v tests/accept_disconnect.rs tests/e2e_proxy_client_disconnect.rs
git mv -v tests/accept_logs_golden.rs tests/e2e_proxy_logs_golden.rs
git mv -v tests/accept_native_http_tcp.rs tests/e2e_native_http_tcp.rs
git mv -v tests/accept_native_http_uds.rs tests/e2e_native_http_uds.rs
git mv -v tests/accept_stream_large.rs tests/e2e_proxy_stream_large.rs
git mv -v tests/accept_wrappers.rs tests/e2e_wrappers_auto_exit.rs

git mv -v tests/proxy_unix_socket.rs tests/e2e_proxy_unix_socket.rs
git mv -v tests/proxy_streaming_tcp.rs tests/e2e_proxy_streaming_tcp.rs
git mv -v tests/proxy_streaming_spawn_fail_plain_500.rs tests/e2e_proxy_streaming_spawn_fail_plain_500.rs
git mv -v tests/proxy_stream_backpressure.rs tests/e2e_proxy_stream_backpressure.rs

git mv -v tests/python_venv_activation.rs tests/e2e_python_venv_activation_tcp_v2.rs
git mv -v tests/dev_tool_routing.rs tests/e2e_dev_tool_routing_make_tcp_v2.rs
git mv -v tests/node_cache_stamp.rs tests/e2e_node_named_cache_ownership_stamp_files.rs
git mv -v tests/shim_embed.rs tests/e2e_shim_embed.rs
git mv -v tests/wrapper_behavior.rs tests/e2e_wrapper_behavior.rs

git mv -v tests/toolchain_rust_bootstrap_exec.rs tests/e2e_toolchain_rust_bootstrap_exec.rs
git mv -v tests/toolchain_rust_image_contents.rs tests/e2e_toolchain_rust_image_contents.rs
git mv -v tests/toolchain_rust_volume_ownership.rs tests/e2e_toolchain_rust_volume_ownership.rs

git mv -v tests/proxy_large_payload.rs tests/e2e_proxy_large_payload_notifications_cmd.rs

# Note: tests/e2e_stream_cargo.rs already follows the e2e_ prefix → no rename needed.
