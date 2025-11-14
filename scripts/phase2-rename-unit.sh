#!/bin/sh
# Phase 2 — Unit file renames (git mv only; keep bodies intact)
# How to run:
#   sh scripts/phase2-rename-unit.sh
set -eu

git mv -v tests/command_lock.rs tests/unit_command_lock.rs
git mv -v tests/docker_cmd_edges.rs tests/unit_shell_escaping_preview_edges.rs
git mv -v tests/doc_smoke_toolchains_rust.rs tests/unit_doc_smoke_toolchains_rust.rs
git mv -v tests/fork_sanitize.rs tests/unit_fork_sanitize.rs
git mv -v tests/images_output.rs tests/unit_images_output.rs
git mv -v tests/images_output_new_agents.rs tests/unit_images_output_new_agents.rs
git mv -v tests/lock_edges.rs tests/unit_lock_edges.rs
git mv -v tests/lock_repo_hashed_path.rs tests/unit_lock_repo_hashed_path.rs
git mv -v tests/lock_repo_scoped_order.rs tests/unit_lock_repo_scoped_order.rs
git mv -v tests/route_map.rs tests/unit_route_map.rs
git mv -v tests/rust_image_helpers.rs tests/unit_rust_image_helpers.rs
git mv -v tests/sanitize_label.rs tests/unit_sanitize_label.rs
git mv -v tests/sanitize_property.rs tests/unit_sanitize_property.rs
git mv -v tests/windows_orchestrator_cmds.rs tests/unit_windows_orchestrator_cmds.rs
