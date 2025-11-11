AIFO Coder: Rust Toolchain Sidecar (v7) — Final, Production‑Ready Specification

Status
- Stage: v7 (final; implementation-ready)
- Scope: Rust toolchain image supply, runtime mounts/env, fallback bootstrap, developer tooling, cross‑platform behavior, volume ownership initialization, observability, CI/testing plan
- Compatibility: Backward compatible with existing sidecar model and other toolchains (Node/TS, Python, Go, C/C++); clarifies and supersedes v6 with normative paths and full per‑phase tests; migration notes provided
- Platforms: Linux/macOS/Windows (Docker-based; Linux adds optional unix socket transport)

Motivation
Rust development inside agent panes (Aider/Crush/Codex and generic toolchain runs) must “just work” with reliability, performance, and reproducibility:
- cargo nextest run --no-fail-fast succeeds (cargo-nextest present).
- cargo test --no-fail-fast succeeds with writable, persistent cargo caches (registry + git).
- cargo clippy --all-targets --all-features -- -D warnings succeeds (clippy installed).
Provide clear knobs for SSH-based git dependencies, sccache, fast linkers (lld/mold), coverage tooling, and corporate proxies with zero‑surprise defaults and robust fallbacks.

Guiding Principles
- Zero‑surprise defaults: non‑root uid:gid works; caches are writable and persistent.
- Prefer host caches when safe; per‑path fallback to named Docker volumes; Windows defaults to named volumes.
- Clear, explicit knobs for optional mounts and behaviors; cross‑arch correctness (amd64/arm64).
- Concise, actionable errors; verbose mode reveals exact commands and stderr for troubleshooting.
- Security unchanged: no privileged containers; no Docker socket mounts; AppArmor when available; minimum necessary mounts.

Key v7 highlights and diffs vs v6
- Normative CARGO_HOME and PATH:
  - CARGO_HOME MUST be /home/coder/.cargo in rust sidecars (run and exec).
  - PATH MUST prepend $CARGO_HOME/bin and SHOULD retain /usr/local/cargo/bin as fallback.
- Default images and selection:
  - Default rust toolchain image MUST be aifo-coder-toolchain-rust:<version|latest>. Official rust:<version>-slim/bookworm is a fallback only when AIFO_RUST_TOOLCHAIN_USE_OFFICIAL=1 or when our image is unavailable; fallback MUST engage a bootstrap wrapper.
- Cache strategy:
  - Host-preferred per-path for Linux/macOS: if $HOME/.cargo/registry exists mount to /home/coder/.cargo/registry; else aifo-cargo-registry named volume. Same for $HOME/.cargo/git -> /home/coder/.cargo/git or aifo-cargo-git.
  - Windows: default to named volumes; host cache mounts are opt‑in only when path semantics are verified safe (future extension).
- Optional knobs (normative behavior and previews):
  - SSH agent forwarding (socket bind mount).
  - sccache (dir/volume) + env.
  - Proxies and cargo networking env pass-through.
  - Fast linkers via RUSTFLAGS.
  - Host cargo config mount (read-only).
- Volume ownership initialization:
  - When named volumes are used, sidecars MUST ensure /home/coder/.cargo/{registry,git} are owned by mapped uid:gid. MUST implement a one‑shot helper (best‑effort) with stamp file to avoid repetition.
- Test plan:
  - v7 finalizes a comprehensive per‑phase test suite: unit/integration (preview/dry-run) run by default, E2E marked ignored for CI lanes that opt in.
  - Existing repository preview and CLI tests are integrated; new toolchain_rust_* tests are added for v7 behaviors.

High‑Level Design
- Toolchain image: aifo-coder-toolchain-rust:<tag>, derived from official rust:<tag> images.
  - Preinstalled: rustup components clippy, rustfmt, rust-src, llvm-tools-preview; cargo-nextest via cargo install --locked.
  - PATH includes /home/coder/.cargo/bin (prepend) and keeps /usr/local/cargo/bin as fallback.
  - CARGO_HOME=/home/coder/.cargo.
- Caches:
  - Default mounts host $HOME/.cargo/{registry,git} if present; per-path fallback to named volumes aifo-cargo-registry and aifo-cargo-git.
  - Forced volumes: AIFO_TOOLCHAIN_RUST_USE_DOCKER_VOLUMES=1 uses named volumes even if host caches exist.
  - Windows hosts: default to named volumes; host cache mounts only when explicitly opted in and safe.
- Environment defaults in sidecar:
  - HOME=/home/coder; GNUPGHOME=/home/coder/.gnupg; CARGO_HOME=/home/coder/.cargo.
  - PATH="$CARGO_HOME/bin:/usr/local/cargo/bin:$PATH".
  - RUST_BACKTRACE=1 by default (best‑effort).
- Optional mounts/env (opt‑in):
  - Host cargo config (read‑only), SSH agent socket, sccache (dir/volume), proxies, cargo networking envs, fast linkers (lld/mold) via RUSTFLAGS.
- Fallback bootstrap (official rust images):
  - First exec installs cargo-nextest (cargo install --locked) and rustup components clippy/rustfmt when missing; idempotent on subsequent execs; logs terse by default; verbose when AIFO_TOOLCHAIN_VERBOSE=1.
- Networking and proxying:
  - Sidecars join a session network aifo-net-<sid>; agent joins the same when toolchains are enabled.
  - On Linux with TCP proxy, add host.docker.internal:host-gateway when AIFO_TOOLEEXEC_ADD_HOST=1 to enable sidecar↔host connectivity.
  - Optional unix socket proxy on Linux (AIFO_TOOLEEXEC_USE_UNIX=1) mounts a host socket directory at /run/aifo.
- Security and isolation:
  - No privileged mode; no Docker socket mounts.
  - AppArmor/seccomp/cgroupns behavior unchanged (apply AppArmor profile when available).

Image Specification (aifo-coder-toolchain-rust)
Base
- FROM ${REGISTRY_PREFIX}rust:<RUST_TAG> (bookworm/slim variants). Multi‑arch: amd64 and arm64.

Rust components (preinstalled via rustup)
- clippy, rustfmt, rust-src, llvm-tools-preview

Cargo tools (preinstalled; pinned where applicable)
- cargo-nextest (cargo install --locked)

System packages (typical crate requirements)
- build-essential (gcc, g++, make), pkg-config
- cmake, ninja
- clang, libclang-dev (bindgen)
- python3
- libssl-dev, zlib1g-dev, libsqlite3-dev, libcurl4-openssl-dev
- git, ca-certificates, curl, tzdata, locales (UTF‑8)

Environment in image
- ENV CARGO_HOME=/home/coder/.cargo
- Ensure PATH contains /home/coder/.cargo/bin (prepend) and /usr/local/cargo/bin (fallback)
- LANG=C.UTF-8

Optional variant
- :ci flavor SHOULD include: sccache, lld and/or mold (fast linkers), and optional QA cargo-* tools

Runtime Behavior (Sidecar)
Mounts and cache strategy
- Workdir:
  - -v <host_repo>:/workspace
  - -w /workspace
- Cargo caches (unless disabled by --no-toolchain-cache or AIFO_TOOLCHAIN_NO_CACHE=1):
  - Default (host caches preferred, per path):
    - If $HOME/.cargo/registry exists: -v $HOME/.cargo/registry:/home/coder/.cargo/registry
      Else: -v aifo-cargo-registry:/home/coder/.cargo/registry
    - If $HOME/.cargo/git exists: -v $HOME/.cargo/git:/home/coder/.cargo/git
      Else: -v aifo-cargo-git:/home/coder/.cargo/git
  - Forced volumes (opt‑in): AIFO_TOOLCHAIN_RUST_USE_DOCKER_VOLUMES=1 forces named volumes even if host caches exist.
  - Windows hosts: default to named volumes; host cache mounts enabled only when validated safe.
- Volume ownership initialization (named volumes):
  - When mounting aifo-cargo-registry/git named volumes, ensure /home/coder/.cargo/{registry,git} are owned by mapped uid:gid.
  - Implement one‑shot “init”: if writes fail (permission denied) or target dirs are missing, run a minimal helper container as root (not privileged) that mounts only the named volume(s) and executes:
    - mkdir -p /home/coder/.cargo/{registry,git} && chown -R <uid>:<gid> /home/coder/.cargo/{registry,git}
  - Stamp each volume with /home/coder/.cargo/<subdir>/.aifo-init-done to avoid repetition; re‑attempt if stamp missing or ownership still invalid.

Optional mounts/env:
- Host cargo config (read‑only): If AIFO_TOOLCHAIN_RUST_USE_HOST_CONFIG=1 and $HOME/.cargo/config(.toml) exists, mount as /home/coder/.cargo/config.toml.
- SSH agent forward: If AIFO_TOOLCHAIN_SSH_FORWARD=1 and SSH_AUTH_SOCK is set, bind‑mount the socket path and export SSH_AUTH_SOCK; user controls known_hosts.
- sccache: If AIFO_RUST_SCCACHE=1,
  - If $AIFO_RUST_SCCACHE_DIR set: -v $AIFO_RUST_SCCACHE_DIR:/home/coder/.cache/sccache
  - Else: -v aifo-sccache:/home/coder/.cache/sccache
  - Export RUSTC_WRAPPER=sccache and SCCACHE_DIR=/home/coder/.cache/sccache
- Proxies: If HTTP_PROXY/HTTPS_PROXY/NO_PROXY defined on host, pass through into sidecar.
- Fast linkers: If AIFO_RUST_LINKER=lld|mold, export RUSTFLAGS:
  - lld: -Clinker=clang -Clink-arg=-fuse-ld=lld
  - mold: -Clinker=clang -Clink-arg=-fuse-ld=mold
- Cargo networking envs (pass‑through if set):
  - CARGO_NET_GIT_FETCH_WITH_CLI
  - CARGO_REGISTRIES_CRATES_IO_PROTOCOL

Other env inside sidecar (always):
- HOME=/home/coder
- GNUPGHOME=/home/coder/.gnupg
- CARGO_HOME=/home/coder/.cargo
- PATH="$CARGO_HOME/bin:/usr/local/cargo/bin:$PATH"
- Default RUST_BACKTRACE=1 when unset

Environment variables (selection and precedence)
- Image selection:
  - AIFO_RUST_TOOLCHAIN_IMAGE: full image ref override wins.
  - AIFO_RUST_TOOLCHAIN_VERSION: tag for aifo-coder-toolchain-rust, default latest.
  - AIFO_RUST_TOOLCHAIN_USE_OFFICIAL=1: force official rust:<version>-slim/bookworm and engage bootstrap wrapper.
- Caches:
  - AIFO_TOOLCHAIN_NO_CACHE=1
  - AIFO_TOOLCHAIN_RUST_USE_DOCKER_VOLUMES=1
  - AIFO_TOOLCHAIN_RUST_USE_HOST_CONFIG=1
- SSH:
  - AIFO_TOOLCHAIN_SSH_FORWARD=1, SSH_AUTH_SOCK
- sccache:
  - AIFO_RUST_SCCACHE=1, AIFO_RUST_SCCACHE_DIR
- Linkers:
  - AIFO_RUST_LINKER=lld|mold
- Proxies/cargo networking:
  - HTTP_PROXY, HTTPS_PROXY, NO_PROXY
  - CARGO_NET_GIT_FETCH_WITH_CLI, CARGO_REGISTRIES_CRATES_IO_PROTOCOL
- Diagnostics:
  - AIFO_TOOLCHAIN_VERBOSE=1
  - RUST_BACKTRACE (default 1)

Phased Plan (Implementation and Tests)

Phase 0 — Image creation (aifo-coder-toolchain-rust)
Objective
- Provide a first‑party rust toolchain image with mandatory components and tools, multi‑arch.

Implementation
- Add toolchains/rust/Dockerfile implementing the “Image Specification” above with:
  - ARG RUST_TAG (e.g., 1.80-bookworm).
  - Preinstall rustup components and cargo-nextest.
  - Install system dependencies.
  - ENV CARGO_HOME=/home/coder/.cargo and PATH as specified.
  - Add :ci variant (optional) with sccache and linkers.

Tests
- New (ignored by default):
  - tests/toolchain_rust_image_contents.rs
    - docker run aifo-coder-toolchain-rust:<tag> sh -lc 'rustup component list' and assert clippy, rustfmt, rust-src, llvm-tools-preview.
    - cargo nextest -V succeeds.
    - PATH/CARGO_HOME env correct.
    - Basic system deps present (gcc, g++, make, pkg-config, cmake, ninja, clang, python3, ssl/zlib/sqlite/curl libs, git).
    - LANG=C.UTF-8.
  - tests/toolchain_rust_image_ci_variant.rs (if CI variant exists)
    - sccache present; lld and/or mold present.
  - CI step:
    - docker buildx imagetools inspect aifo-coder-toolchain-rust:<tag> includes linux/amd64 and linux/arm64 manifests.

Success Criteria
- All required components/tools present; multi-arch published; envs configured.

Phase 1 — Makefile integration (build/publish)
Objective
- Add Makefile targets to build/rebuild/publish rust toolchain image.

Implementation
- Add targets mirroring cpp:
  - build-toolchain-rust, rebuild-toolchain-rust
  - publish-toolchain-rust (buildx multi-arch; respects REGISTRY/PLATFORMS/PUSH)

Tests
- New:
  - tests/make_targets_rust_toolchain.rs
    - make -n help contains build/rebuild/publish rust targets.
  - tests/make_dry_run_rust_toolchain.rs
    - make -n build-toolchain-rust includes docker build(x) with toolchains/rust/Dockerfile, tag aifo-coder-toolchain-rust:latest (or version), and proper cache flags.
  - tests/publish_dry_run_rust_toolchain.rs
    - make -n publish-toolchain-rust with PLATFORMS, PUSH=0 shows --platform and --output or --push decisions; REGISTRY normalization ensures trailing slash as needed.

Success Criteria
- Targets exist and preview correct commands without network side effects.

Phase 2 — Runtime image selection in code
Objective
- Select aifo-coder-toolchain-rust by default; support overrides; official fallback engages bootstrap.

Implementation
- src/toolchain.rs:
  - default_toolchain_image("rust"):
    - If AIFO_RUST_TOOLCHAIN_IMAGE set: use it.
    - Else if AIFO_RUST_TOOLCHAIN_VERSION set: aifo-coder-toolchain-rust:<version>.
    - Else: aifo-coder-toolchain-rust:latest.
  - default_toolchain_image_for_version("rust", v): aifo-coder-toolchain-rust:<v>.
  - Respect AIFO_RUST_TOOLCHAIN_USE_OFFICIAL=1: use official rust image and mark for bootstrap in exec.

Tests
- Existing (integration):
  - tests/cli_toolchain_flags_reporting.rs (verifies verbose dry-run reports toolchains and overrides)
  - tests/cli_toolchain_dry_run.rs (verifies dry-run previews)
  - tests/cli_toolchain_override_precedence.rs (override wins over version)
- New (unit):
  - tests/toolchain_rust_image_selection.rs
    - Assert mapping for default, versioned, explicit image, and official fallback marking.

Success Criteria
- Overrides honored; default image is aifo-coder-toolchain-rust; official fallback marked for bootstrap.

Phase 3 — Mount strategy and env propagation
Objective
- Enforce CARGO_HOME=/home/coder/.cargo; PATH prefix; cache mounts; optional mounts/envs; RUST_BACKTRACE default; Windows default volumes.

Implementation
- src/toolchain.rs:
  - build_sidecar_run_preview(kind="rust"):
    - Inject -e CARGO_HOME=/home/coder/.cargo; -e PATH="$CARGO_HOME/bin:/usr/local/cargo/bin:$PATH".
    - If no_cache: no cargo mounts.
    - Else if AIFO_TOOLCHAIN_RUST_USE_DOCKER_VOLUMES=1: named volumes to /home/coder/.cargo/{registry,git}.
    - Else: host mounts $HOME/.cargo/{registry,git} if exists, otherwise named volumes.
    - Windows default to named volumes.
    - Optional mounts/envs:
      - Host cargo config (read‑only) when AIFO_TOOLCHAIN_RUST_USE_HOST_CONFIG=1.
      - SSH agent mount when AIFO_TOOLCHAIN_SSH_FORWARD=1 and SSH_AUTH_SOCK present.
      - sccache mounts and envs when AIFO_RUST_SCCACHE=1.
      - Pass-through HTTP(S)_PROXY/NO_PROXY, cargo networking envs, set default RUST_BACKTRACE=1 if unset.
  - build_sidecar_exec_preview(kind="rust"):
    - Export CARGO_HOME and PATH as above (always).
    - When official rust image in use: wrap exec with bootstrap sequence (Phase 4).

Tests
- Existing (preview/agent integration; ensure no regressions):
  - tests/preview_mounts.rs (gnupg host mount and aider configs)
  - tests/preview_proxy_env.rs (AIFO_TOOLEEXEC_URL/TOKEN mapping)
  - tests/preview_user_flag_unix.rs (--user uid:gid on unix)
  - tests/preview_workspace.rs (-v workspace and -w /workspace)
  - tests/preview_path_contains_shim_dir.rs (PATH includes /opt/aifo/bin:/opt/venv/bin in agent)
  - tests/preview_git_sign.rs (GIT signing disable for aider)
  - tests/preview_container_name.rs and tests/preview_hostname_env.rs (--name/--hostname handling)
  - tests/preview_unix_mount.rs (unix socket mount when set)
- New (integration/unit for rust sidecar):
  - tests/toolchain_rust_mounts.rs
    - Host-present registry/git -> /home/coder/.cargo/{registry,git}
    - Fallback to aifo-cargo-registry/git volumes
    - AIFO_TOOLCHAIN_NO_CACHE=1 removes mounts
    - AIFO_TOOLCHAIN_RUST_USE_DOCKER_VOLUMES=1 forces named volumes
    - Windows: defaults to named volumes (guarded for Windows CI)
  - tests/toolchain_rust_envs.rs
    - Asserts CARGO_HOME and PATH in run/exec previews; RUST_BACKTRACE defaults to 1; proxy and cargo networking env pass-through present when set
  - tests/toolchain_rust_optional_mounts.rs
    - Host cargo config mount when enabled; SSH agent forwarding mount/env when enabled
  - tests/toolchain_rust_sccache.rs
    - sccache dir/volume mount and envs for both with/without AIFO_RUST_SCCACHE_DIR
  - tests/toolchain_rust_linkers.rs
    - RUSTFLAGS set appropriately for lld and mold

Success Criteria
- Previewed sidecar commands include correct mounts/envs across all toggles and OS defaults.

Phase 4 — Fallback bootstrap on official rust images
Objective
- When using official rust image, ensure idempotent bootstrap installs cargo-nextest and rustup components.

Implementation
- src/toolchain.rs:
  - In build_sidecar_exec_preview(kind="rust"), when official image selected:
    - Wrap user command with sh -lc '<bootstrap>; exec "$@"' where bootstrap:
      - cargo nextest -V >/dev/null 2>&1 || cargo install cargo-nextest --locked
      - rustup component list | grep -q '^clippy ' || rustup component add clippy rustfmt
      - Optional: if AIFO_RUST_SCCACHE=1 and sccache missing -> install or emit a concise error (policy-based; at minimum emit an actionable warning)
    - Honor HTTP(S)_PROXY/NO_PROXY and AIFO_TOOLCHAIN_VERBOSE for log verbosity.

Tests
- New:
  - tests/toolchain_rust_bootstrap_wrapper_preview.rs (unit/integration)
    - Exec preview shows bootstrap wrapper on official images; no wrapper on aifo-coder-toolchain-rust
  - tests/toolchain_rust_bootstrap_exec.rs (ignored by default; E2E)
    - First exec installs nextest/clippy/rustfmt; second exec is idempotent; verbose flag prints expanded logs
  - tests/toolchain_rust_bootstrap_sccache_policy.rs (integration)
    - With AIFO_RUST_SCCACHE=1 on official images, either installs sccache or produces a clear message (policy enforced)

Success Criteria
- Bootstrap wrapper present and effective only when appropriate; idempotency verified; verbosity toggles logs.

Phase 5 — PATH, ownership, and networking
Objective
- Ensure PATH and user mapping; perform volume ownership initialization; network/add-host correctness.

Implementation
- src/toolchain.rs:
  - build_sidecar_run_preview/build_sidecar_exec_preview:
    - Always export PATH="$CARGO_HOME/bin:/usr/local/cargo/bin:$PATH" and CARGO_HOME.
  - One‑shot ownership init:
    - When named volumes selected for registry/git, attempt write to /home/coder/.cargo/{registry,git}; on permission error or missing dirs:
      - Run helper container as root mounting the named volume(s) only, execute mkdir -p and chown to uid:gid, drop .aifo-init-done stamp.
      - Retry write; log concise messages in verbose mode; best‑effort (do not fail agent run if helper fails; surface a warning).
- Networking:
  - Sidecars join aifo-net-<sid>.
  - On Linux with TCP proxy, add host.docker.internal:host-gateway when AIFO_TOOLEEXEC_ADD_HOST=1 (already implemented for agent; replicate for sidecars).

Tests
- Existing (agent):
  - tests/preview_network.rs (session network and Linux add-host for agent)
- New:
  - tests/toolchain_rust_path_and_user.rs (unit/integration)
    - PATH and --user uid:gid present in run/exec previews
  - tests/toolchain_rust_volume_ownership.rs (ignored by default; E2E)
    - Simulate permission denial; verify helper chown runs, stamp file created, and subsequent runs skip init
  - tests/toolchain_rust_networking.rs (integration)
    - Sidecar run preview includes --network aifo-net-<sid>; on Linux, includes --add-host when AIFO_TOOLEEXEC_ADD_HOST=1

Success Criteria
- PATH and --user env correct; ownership init self‑heals named volumes; networking flags correct for sidecars.

Phase 6 — Testing and validation (tie-back)
Objective
- Ensure a cohesive, automated verification strategy with unit/integration and E2E layers.

Implementation
- Organize tests into:
  - Unit/preview (run by default): validate previews, env and mount shaping, selection logic, CLI reporting.
  - E2E (ignored by default): run real docker flows when CI lane opts in; Linux/macOS/Windows matrix as applicable.
- Preserve and integrate existing preview_* and CLI tests; add new toolchain_rust_* suite as specified.

Tests
- Existing suite integrated (examples):
  - tests/cli_toolchain_dry_run.rs
  - tests/cli_toolchain_flags_reporting.rs
  - tests/cli_toolchain_override_precedence.rs
  - tests/preview_container_name.rs
  - tests/preview_git_sign.rs
  - tests/preview_hostname_env.rs
  - tests/preview_mounts.rs
  - tests/preview_path_contains_shim_dir.rs
  - tests/preview_proxy_env.rs
  - tests/preview_unix_mount.rs
  - tests/preview_user_flag_unix.rs
  - tests/preview_workspace.rs
  - tests/preview_network.rs
- New suite listed under Phases 0–5.

Success Criteria
- All unit/integration tests pass by default; E2E tests pass in opt-in CI lanes.

Phase 7 — Documentation
Objective
- Document image selection, cache strategy, SSH forwarding, sccache, proxies, cargo networking, fast linkers, volume ownership init, verbosity controls.

Implementation
- Update docs/TOOLCHAINS.md with rust-specific sections; include Windows guidance and ownership init troubleshooting.

Tests
- New:
  - tests/doc_smoke_toolchains_rust.rs (doc lint)
    - Asserts presence of key headings/phrases in docs/TOOLCHAINS.md (e.g., “AIFO Rust Toolchain”, “CARGO_HOME”, “sccache”, “AIFO_RUST_TOOLCHAIN_IMAGE”, “ownership initialization”)

Success Criteria
- Documentation smoke test passes; docs are current and actionable.

Phase 8 — Rollout (acceptance)
Objective
- Validate end‑to‑end experience in sidecars; confirm default preferences and envs.

Implementation
- Ensure Makefile and CI jobs publish multi‑arch images; update release notes.

Tests
- New (ignored by default; E2E):
  - tests/acceptance_rust_sidecar_smoke.rs
    - cargo nextest run --no-fail-fast
    - cargo test --no-fail-fast (no permission errors on registry/git)
    - cargo clippy --all-targets --all-features -- -D warnings
    - cargo fmt -- --check
  - tests/default_image_regression.rs (unit)
    - Ensures default code path prefers aifo-coder-toolchain-rust and CARGO_HOME=/home/coder/.cargo in previews

Success Criteria
- Smoke suite passes; defaults remain stable.

Repository Integration Map (files and responsibilities)
- toolchains/rust/Dockerfile: v7 image per spec (Phase 0).
- Makefile: add build-toolchain-rust, rebuild-toolchain-rust, publish-toolchain-rust (Phase 1).
- src/toolchain.rs:
  - Image selection (Phase 2).
  - Rust mounts and env (Phase 3).
  - Optional knobs (SSH, sccache, proxies, cargo networking, host cargo config, linkers) (Phase 3).
  - Bootstrap wrapper (Phase 4).
  - Ownership init helper logic (Phase 5).
  - Sidecar networking flags (Phase 5).
- src/docker.rs, src/apparmor.rs, src/registry.rs, src/util.rs:
  - No changes required for v7 beyond integration points; ensure existing agent previews remain consistent with tests (PATH shims, mounts, env pass-through, network flags).

Makefile Test Targets (optional, recommended)
- test-toolchain-rust: run unit/integration rust sidecar tests (exclude ignored/E2E)
- test-toolchain-rust-e2e: run ignored tests explicitly (CI lanes)
- Example nextest expressions (informational):
  - cargo nextest run -E 'test(/^toolchain_rust_/)'
  - cargo nextest run --include-ignored -E 'test(/^toolchain_rust_/)'

Acceptance Criteria (final)
- In agent panes with --toolchain rust:
  - cargo nextest run --no-fail-fast works (cargo-nextest present).
  - cargo test --no-fail-fast works without registry permission errors.
  - cargo clippy --all-targets --all-features -- -D warnings works (clippy installed).
- Default behavior prefers host cache mounts with per‑path fallback to named volumes (Linux/macOS) and named volumes by default on Windows.
- PATH includes $CARGO_HOME/bin and /usr/local/cargo/bin across run/exec.
- Bootstrap is engaged only on official rust images and is idempotent.
- Ownership initialization self‑heals named volumes and avoids repeated work via stamps.
- Unit/integration preview and CLI tests pass by default; E2E tests pass in opt‑in CI.

Migration Notes (from current codebase)
- Current implementation mounts cargo caches at /usr/local/cargo and sets CARGO_HOME=/usr/local/cargo for rust; v7 migrates to /home/coder/.cargo:
  - Update run/exec previews, tests, and any assertions to /home/coder/.cargo.
  - Keep named volume names (aifo-cargo-registry/git) unchanged for continuity.
  - Prepend PATH with $CARGO_HOME/bin in all rust sidecar contexts to expose cargo‑installed tools.
- Introduce image selection envs and bootstrap wrapper as specified; add proxy/cargo networking pass‑through and optional knobs incrementally if needed.
- Add non‑privileged volume ownership initialization for named volumes to avoid first‑use permission issues.
- Ensure sidecars adopt session networking and host‑gateway add-host behavior on Linux consistently with agent behavior.

Appendix A: Summary of Key Environment Variables
- Image:
  - AIFO_RUST_TOOLCHAIN_IMAGE, AIFO_RUST_TOOLCHAIN_VERSION, AIFO_RUST_TOOLCHAIN_USE_OFFICIAL
- Caches:
  - AIFO_TOOLCHAIN_NO_CACHE, AIFO_TOOLCHAIN_RUST_USE_DOCKER_VOLUMES, AIFO_TOOLCHAIN_RUST_USE_HOST_CONFIG
- SSH:
  - AIFO_TOOLCHAIN_SSH_FORWARD, SSH_AUTH_SOCK
- sccache:
  - AIFO_RUST_SCCACHE, AIFO_RUST_SCCACHE_DIR
- Linkers:
  - AIFO_RUST_LINKER
- Proxies/cargo networking:
  - HTTP_PROXY, HTTPS_PROXY, NO_PROXY
  - CARGO_NET_GIT_FETCH_WITH_CLI, CARGO_REGISTRIES_CRATES_IO_PROTOCOL
- Diagnostics:
  - AIFO_TOOLCHAIN_VERBOSE, RUST_BACKTRACE (default 1)

Appendix B: Test Suite Inventory (new files to add)
- tests/toolchain_rust_image_contents.rs
- tests/toolchain_rust_image_ci_variant.rs (optional)
- tests/make_targets_rust_toolchain.rs
- tests/make_dry_run_rust_toolchain.rs
- tests/publish_dry_run_rust_toolchain.rs
- tests/toolchain_rust_image_selection.rs
- tests/toolchain_rust_mounts.rs
- tests/toolchain_rust_envs.rs
- tests/toolchain_rust_optional_mounts.rs
- tests/toolchain_rust_sccache.rs
- tests/toolchain_rust_linkers.rs
- tests/toolchain_rust_bootstrap_wrapper_preview.rs
- tests/toolchain_rust_bootstrap_exec.rs
- tests/toolchain_rust_bootstrap_sccache_policy.rs
- tests/toolchain_rust_path_and_user.rs
- tests/toolchain_rust_volume_ownership.rs
- tests/toolchain_rust_networking.rs
- tests/doc_smoke_toolchains_rust.rs
- tests/acceptance_rust_sidecar_smoke.rs
- tests/default_image_regression.rs

Notes on Test Hygiene
- All docker-dependent tests MUST:
  - Skip when docker is not available (reuse pattern via container_runtime_path()).
  - Avoid pulling images by default; allow overriding test images via env (e.g., AIFO_CODER_TEST_RUST_IMAGE).
- Windows-only assertions should live in a Windows CI job (or be guarded and skipped elsewhere).
- E2E tests MUST be marked #[ignore] by default to keep default test runs fast and reliable.

This v7 specification consolidates and finalizes all requirements and testing for the Rust toolchain sidecar in aifo-coder. It aligns code, images, Makefile, and tests with a cohesive, production‑ready standard and a clear migration path.
