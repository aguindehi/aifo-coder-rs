# Test Coverage Report

This document summarizes the current state of the test suite, highlights strengths, and scores coverage with clear next steps. It reflects all tests currently provided in the chat.

Current coverage overview

A) CLI flows and subcommands
- doctor
  - tests/cli_doctor.rs: doctor executes and exits 0 (happy path).
  - tests/doctor_no_docker.rs: doctor still exits 0 with docker absent (PATH cleared); asserts stderr shows “docker command:  (not found)”.
  - tests/cli_doctor_details.rs: when Docker is present, asserts “docker registry:” and “docker security options:” lines are printed.
- images
  - tests/cli_images.rs: prints codex/crush/aider image lines and exits 0.
  - tests/cli_images_flavor.rs and tests/cli_images_flavor_flag.rs: slim flavor honored via env and --flavor flag.
  - tests/cli_images_registry_env.rs: “images” prints the effective registry line according to env override (value or empty/Docker Hub).
- agent run dry-run
  - tests/cli_dry_run.rs: --dry-run + --verbose prints docker preview for aider and exits 0.
  - tests/cli_image_override.rs: --image override reflected in verbose output and preview string.
- cache maintenance
  - tests/cli_cache_clear.rs: cache-clear exits 0.
  - tests/cli_cache_clear_effect.rs: removes the on-disk registry cache file (XDG_RUNTIME_DIR).
  - tests/cli_toolchain_cache_clear.rs: toolchain-cache-clear exits 0 (skips when docker missing).
- toolchain command (Phase 1)
  - tests/cli_toolchain_dry_run.rs: toolchain rust dry-run prints docker run/exec previews.
  - tests/cli_toolchain_override_precedence.rs: --toolchain-image takes precedence over --toolchain-spec version mapping.
  - tests/cli_toolchain_flags_reporting.rs: verbose dry-run reports attached toolchains and computed image overrides.
  - tests/toolchain_phase1.rs: direct toolchain_run dry-run success for rust and node.
  - tests/toolchain_live.rs (ignored): live runs for rust/node simple version commands (skips unless images present).

B) Docker command preview for agent containers (build_docker_cmd)
- basic mounts and workdir
  - tests/preview_workspace.rs: mounts PWD:/workspace and sets -w /workspace.
- config/state mounts and GnuPG
  - tests/preview_mounts.rs: mounts ~/.gnupg-host:ro and Aider top-level config files to expected container locations.
- environment mapping
  - tests/preview_api_env.rs: AIFO_API_* map to OPENAI_* and AZURE_* correctly (including OPENAI_API_TYPE=azure).
  - tests/preview_proxy_env.rs: AIFO_TOOLEEXEC_URL/TOKEN are passed through.
  - tests/preview_pass_env.rs: EDITOR pass-through present; no git-sign env unless explicitly disabled.
  - tests/preview_pass_env_more.rs: confirms VISUAL, TERM and TZ are passed via -e NAME.
  - tests/preview_git_sign.rs: AIFO_CODER_GIT_SIGN=0 injects GIT_CONFIG_* triplet to disable signing for Aider.
- container identity
  - tests/preview_container_name.rs: respects AIFO_CODER_CONTAINER_NAME for both --name and --hostname.
  - tests/preview_hostname_env.rs: AIFO_CODER_HOSTNAME can differ from container name and is honored.
- network and host connectivity
  - tests/preview_network.rs: honors AIFO_SESSION_NETWORK; on Linux, injects --add-host host.docker.internal:host-gateway when requested.
- shims and unix mounts
  - tests/preview_shim_dir.rs: mounts host shim dir to /opt/aifo/bin:ro when AIFO_SHIM_DIR is set.
  - tests/preview_unix_mount.rs: mounts AIFO_TOOLEEXEC_UNIX_DIR to /run/aifo when set.
  - tests/preview_path_contains_shim_dir.rs: PATH export inside the container includes /opt/aifo/bin and /opt/venv/bin.
- AppArmor flags
  - tests/preview_apparmor.rs: includes or omits --security-opt apparmor=… based on docker_supports_apparmor().
  - tests/apparmor_env_disable.rs: env override “none” disables AppArmor.
  - tests/apparmor_env_fallback.rs (Linux): env forces non-existent profile; falls back to docker-default if available.
- user mapping
  - tests/preview_user_flag_unix.rs (Unix): preview contains --user <uid>:<gid>.

C) Toolchain sidecars and sessions
- session lifecycle
  - tests/session_cleanup.rs: starts rust sidecar (only if local image present), then verifies container and network are removed by cleanup.
  - tests/cleanup_idempotent.rs: cleanup with random/nonexistent session ID is a no-op.
- mappings and defaults
  - tests/toolchain_mappings.rs: normalize_toolchain_kind aliases and default image by version mapping for all kinds.
  - tests/toolchain_unknown_kind_version_default.rs: unknown kind falls back to node default mapping.
- C/C++ specific
  - tests/toolchain_cpp.rs: dry-run OK (live cmake version test ignored by default).
- bootstrap
  - tests/toolchain_bootstrap_typescript.rs: best-effort TypeScript global bootstrap does not fail (even if sidecar absent).

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
  - tests/cli_images_registry_env.rs: ensures images output reflects the effective registry consistent with env override.

F) Locking
- tests/lock_edges.rs: threaded contention produces “already running” error; lock then can be reacquired.

G) Shims and notifications
- tests/shims.rs: writes all expected shims; invoking any shim without proxy env exits 86.
- tests/shims_notifications.rs: notifications-cmd shim exits 86 without proxy env.
- tests/notifications.rs: end-to-end notifications-cmd over proxy:
  - OK path with matching args runs host “say”.
  - Mismatch path returns 403 and exit 86 (with specific error text).

H) Image content smoke
- tests/shim_embed.rs (ignored): verifies embedded aifo-shim and PATH tools in agent image (if present).

What’s notably strong
- Thorough coverage of build_docker_cmd: mounts, env mappings (OpenAI/Azure and proxy), container identity, networking, PATH export, user mapping, AppArmor flags.
- Robust proxy protocol surface: auth and version enforcement, allowlist, timeout, unix transport (Linux), and language-specific routing (Python venv; TypeScript local vs npx).
- Toolchain lifecycle: dry-run, overrides precedence, bootstrap best-effort, cleanup idempotency, and per-language caches/envs (including unit-level previews).
- CLI behaviors across doctor/images/cache clear, toolchain subcommand (phase 1), and verbose reporting of effective configuration.
- Shims presence and failure modes without proxy env; notifications feature validates both allow and reject paths.
- Registry handling via env overrides and normalization.

Coverage scoring

- Feature breadth coverage: 90% (A-)
  - Most CLI commands, preview construction, proxy protocol, and toolchain flows are exercised.
- Critical path assertions: 88% (A-)
  - Previews include security, identity, env, mounts, and user mapping; doctor prints key diagnostics; proxy rejects invalid requests.
- OS-specific behavior: 85% (B+)
  - Linux-only paths (add-host, unix sockets, AppArmor) have tests; macOS/Windows specifics are indirectly covered via doctor; some host variations remain untested.
- Negative/error handling: 90% (A-)
  - Auth, protocol version, allowlist, timeouts, and config mismatches covered.
- E2E/integration depth: 80% (B)
  - End-to-end proxy tests are present for rust/node (TCP/unix), with heavier tests gated behind #[ignore]; other toolchains less exercised in live mode.
- Gaps (not yet covered): wrapper script behavior; live smoke for python/c-cpp/go; deterministic tests for registry curl/TCP probe branches.

Overall grade: A- (strong, comprehensive suite with a few targeted areas to extend)

Next steps (prioritized)

1) Wrapper script behavior (ignored by default; low risk to core code)
- Add tests to ensure the aifo-coder shell wrapper prefers a system-installed binary when present and falls back to cargo build otherwise.
- Guard with environment isolation and mark #[ignore] to run on dedicated runners.

2) Live smoke for additional toolchains (conditional, skip if images missing)
- Add end-to-end proxy smoke tests for python, c-cpp, and go (e.g., pip --version, cmake --version, go version).
- Keep skipped when images aren’t present locally to avoid pulls in CI.

3) Registry probe determinism
- Consider refactoring preferred_registry_prefix into a probe abstraction to allow injecting test doubles for curl/TCP.
- Add unit tests for “curl success,” “curl failure,” “TCP success,” and “TCP fail → Docker Hub” branches.

4) AppArmor portability checks
- Add tests for macOS/Windows default behavior of desired_apparmor_profile_quiet() (docker-default), guarded with appropriate #[cfg] and skipping where Docker info is unavailable.

5) Doctor output refinements (optional)
- Add assertions for the “workspace writable” line when Docker is present and mount succeeds, and for editor discovery lines when images are locally available.

6) C/C++ and Go cache effectiveness (optional, flaky → #[ignore])
- Run a tiny build twice inside c-cpp to observe ccache hits (or at least stable stats output).
- For Go, assert go env GOPATH/GOMODCACHE/GOCACHE values match expectations inside sidecar.

Stability and CI notes
- Continue to gate Docker-dependent tests with presence checks; prefer skipping to avoid flaky CI.
- Keep heavy or platform-sensitive tests under #[ignore] and run them in a separate job or locally.
- Maintain Linux-specific guards (cfg[target_os = "linux"]) for unix sockets and host-gateway behavior.
