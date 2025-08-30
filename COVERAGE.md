# Test Coverage Report

This document summarizes the current state of the test suite, highlights strengths, and proposes additional tests to close remaining gaps. It is based on all tests and source files provided in the chat.

Current coverage overview

A) CLI flows and subcommands
- doctor
  - tests/cli_doctor.rs: doctor executes and exits 0 (happy path).
  - tests/doctor_no_docker.rs: doctor still exits 0 with docker absent (PATH cleared); asserts stderr shows “docker command:  (not found)”.
- images
  - tests/cli_images.rs: prints codex/crush/aider image lines and exits 0.
  - tests/cli_images_flavor.rs and tests/cli_images_flavor_flag.rs: slim flavor honored via env and --flavor flag.
  - tests/cli_images_registry_env.rs: “images” prints the effective registry line; respects env override (value and empty) and displays “Docker Hub” when empty.
- agent run dry-run
  - tests/cli_dry_run.rs: --dry-run + --verbose prints docker preview for aider and exits 0.
  - tests/command_lock.rs:test_build_docker_cmd_preview_contains: preview contains expected markers (shell, image, agent invocation).
- cache maintenance
  - tests/cli_cache_clear.rs: cache-clear exits 0.
  - tests/cli_cache_clear_effect.rs: removes the on-disk registry cache file (XDG_RUNTIME_DIR).
  - tests/cli_toolchain_cache_clear.rs: toolchain-cache-clear exits 0 (skips when docker missing).
- toolchain command (Phase 1)
  - tests/cli_toolchain_dry_run.rs: toolchain rust dry-run prints docker run/exec previews.
  - tests/cli_toolchain_override_precedence.rs: --toolchain-image takes precedence over --toolchain-spec version mapping.
  - tests/toolchain_phase1.rs: direct toolchain_run dry-run success for rust and node.
  - tests/toolchain_live.rs (ignored): live runs for rust/node simple version commands (skips in CI unless images present).

B) Docker command preview for agent containers (build_docker_cmd)
- basic mounts and workdir
  - tests/preview_workspace.rs: mounts PWD:/workspace and sets -w /workspace.
- config/state mounts and GnuPG
  - tests/preview_mounts.rs: mounts ~/.gnupg-host:ro and Aider top-level config files to expected container locations.
- environment mapping
  - tests/preview_api_env.rs: AIFO_API_* map to OPENAI_* and AZURE_* correctly (including OPENAI_API_TYPE=azure).
  - tests/preview_proxy_env.rs: AIFO_TOOLEEXEC_URL/TOKEN are passed through.
  - tests/preview_pass_env.rs: EDITOR pass-through present; no git-sign env unless explicitly disabled.
  - tests/preview_git_sign.rs: AIFO_CODER_GIT_SIGN=0 injects GIT_CONFIG_* triplet to disable signing for Aider.
- container identity
  - tests/preview_container_name.rs: respects AIFO_CODER_CONTAINER_NAME for both --name and --hostname.
- network and host connectivity
  - tests/preview_network.rs: honors AIFO_SESSION_NETWORK; on Linux, injects --add-host host.docker.internal:host-gateway when requested.
- shims and unix mounts
  - tests/preview_shim_dir.rs: mounts host shim dir to /opt/aifo/bin:ro when AIFO_SHIM_DIR is set.
  - tests/preview_unix_mount.rs: mounts AIFO_TOOLEEXEC_UNIX_DIR to /run/aifo when set.
- AppArmor flag
  - tests/preview_apparmor.rs: includes or omits --security-opt apparmor=… based on docker_supports_apparmor().
- shell escaping and helpers
  - tests/docker_cmd_edges.rs: verifies shell escaping for spaces and single quotes in preview.
  - tests/helpers.rs: unit tests for shell_escape, shell_join, path_pair, ensure_file_exists, candidate_lock_paths, desired_apparmor_profile option behavior.

C) Toolchain sidecars and sessions
- session lifecycle
  - tests/session_cleanup.rs: starts rust sidecar (only if local image present), then verifies container and network are removed by cleanup.
  - tests/cleanup_idempotent.rs: cleanup with random/nonexistent session ID is a no-op.
- mappings and defaults
  - tests/toolchain_mappings.rs: normalize_toolchain_kind aliases and default image by version mapping for all kinds.
- C/C++ specific
  - tests/toolchain_cpp.rs: dry-run OK (live cmake version test ignored by default).

D) Proxy and shim protocol
- smoke and routing
  - tests/proxy_smoke.rs (ignored): end-to-end rust+node via TCP proxy; cargo/npx succeed.
- protocol and auth handling
  - tests/proxy_negatives_light.rs: 401 (no Authorization) and 400 (missing tool) without sidecars.
  - tests/proxy_protocol.rs: 426 + exit 86 for missing or wrong X-Aifo-Proto.
  - tests/proxy_allowlist.rs: 403 for disallowed tool names before any docker exec.
  - tests/proxy_negative.rs (ignored): 401 and 403 with a rust sidecar present.
- unix socket transport (Linux)
  - tests/proxy_unix_socket.rs (ignored, Linux-only): runs rust/node via unix:/// socket; asserts cleanup removes the socket directory.
- language-specific proxy behaviors
  - tests/proxy_python_venv.rs: respects workspace .venv/bin/python (venv preferred).
  - tests/proxy_tsc_local.rs: prefers local ./node_modules/.bin/tsc over npx fallback.
  - tests/proxy_timeout.rs (ignored): 504 timeout + X-Exit-Code: 124 on long-running python.

E) Registry prefix logic
- tests/registry_env_empty.rs and tests/registry_env_value.rs:
  - Environment override empty forces Docker Hub (no prefix); non-empty normalized to exactly one trailing slash; source tags env/env-empty.
- CLI reflection
  - tests/cli_images_registry_env.rs: ensures “images” outputs the effective registry consistent with env override.

F) Locking
- tests/lock_edges.rs: threaded contention produces “already running” error; lock then can be reacquired.
- tests/command_lock.rs:test_acquire_lock_at_exclusive_and_release: exclusive lock behavior on a specific path.

G) Shims and notifications
- tests/shims.rs: writes all expected shims; invoking any shim without proxy env exits 86.
- tests/shims_notifications.rs: notifications-cmd shim exits 86 without proxy env.
- tests/notifications.rs: end-to-end notifications-cmd over proxy:
  - OK path with matching args runs host “say”.
  - Mismatch path returns 403 and exit 86 (with specific error text).

H) Image content smoke
- tests/shim_embed.rs (ignored): verifies embedded aifo-shim and PATH tools in agent image (if present).

What’s notably strong
- Thorough coverage of build_docker_cmd, including env mappings, mounts, container identity, networking, AppArmor, and shell-escaping behavior.
- Robust proxy protocol surface: auth, version negotiation, allowlist, timeout, unix transport (Linux), and language-specific routing rules (Python venv; TypeScript local vs npx).
- Toolchain lifecycle: dry-run, overrides precedence, cleanup, and per-language cache/envs verified (plus unit tests in src/lib.rs).
- CLI behaviors across doctor/images/cache clear and the Phase 1 toolchain subcommand.
- Shims presence and failure modes without proxy env; notifications feature tests validate both allow and reject paths.
- Registry handling: env-based overrides and CLI reflection; normalization of trailing slash is tested.

Proposed additions (prioritized and low-flakiness first)
- CLI: explicit image override for agent runs
  - tests/cli_image_override.rs
    - Run: aifo-coder --verbose --dry-run --image alpine:3.20 aider -- --version
    - Assert: stderr shows chosen image alpine:3.20 and preview uses it.
- Hostname override distinct from container name
  - tests/preview_hostname_env.rs
    - Set AIFO_CODER_HOSTNAME=my-host; assert preview has --hostname my-host (independent of --name).
- AppArmor environment overrides (deterministic)
  - tests/apparmor_env_disable.rs
    - Set AIFO_CODER_APPARMOR_PROFILE=none; desired_apparmor_profile_quiet() returns None and build_docker_cmd omits apparmor flag.
  - tests/apparmor_env_fallback.rs (Linux-only, conditional on daemon support)
    - Set AIFO_CODER_APPARMOR_PROFILE to a non-existent profile; expect fallback to docker-default if available, else omit.
- Agent PATH export includes shim dir
  - tests/preview_path_contains_shim_dir.rs
    - Assert preview shell command includes PATH="/opt/aifo/bin:/opt/venv/bin:$PATH".
- Additional env pass-throughs
  - tests/preview_pass_env_more.rs
    - Set VISUAL, TERM, TZ; assert -e VISUAL, -e TERM, -e TZ flags appear by name (complementing EDITOR test).
- User/UID mapping flag (Unix)
  - tests/preview_user_flag_unix.rs (Unix-only)
    - Assert preview includes --user <uid>:<gid>.
- Toolchain bootstrap (TypeScript)
  - tests/toolchain_bootstrap_typescript.rs (skip unless node image present locally)
    - Start node sidecar session; call toolchain_bootstrap_typescript_global(); then run tsc --version via proxy and assert non-failure.
- Toolchain CLI flags round-trip (dry-run)
  - tests/cli_toolchain_flags_reporting.rs
    - Run: aifo-coder --verbose --dry-run --toolchain rust --toolchain-spec node@20 --toolchain-image python=python:3.12-slim aider -- --version
    - Assert verbose output lists attached toolchains and computed image overrides.
- Unknown toolchain kind default mapping
  - tests/toolchain_unknown_kind_version_default.rs
    - Validate default_toolchain_image_for_version() fallback for unknown kinds (or via verbose dry-run image override behavior).
- Doctor extended assertions (when docker present)
  - tests/cli_doctor_details.rs
    - Assert presence of “docker registry:” and parsed “docker security options:” lines; optionally assert “workspace writable” line.

Notes on stability and CI
- Continue to skip live/sidecar tests if required images are not present locally, to avoid pulling in CI.
- Keep heavier or environment-sensitive tests as #[ignore], running them only in extended pipelines.
- Guard Linux-only behavior (e.g., host-gateway add-host and unix socket transport) using #[cfg(target_os = "linux")] and/or environment toggles.

If you want me to draft any of the proposed tests as ready-to-drop files (e.g., cli_image_override.rs, preview_hostname_env.rs, apparmor_env_disable.rs), let me know which ones to start with.
