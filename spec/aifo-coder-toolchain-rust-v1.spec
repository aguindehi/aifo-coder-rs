AIFO Coder: Rust Toolchain Sidecar (v1) – Comprehensive, Production‑Ready Specification

Status
- Stage: v1 specification (implementation-ready)
- Scope: Docker image supply for Rust toolchain, sidecar runtime mounts/env, optional developer tools
- Compatibility: Backward compatible with existing sidecar model; additive improvements and safe fallbacks
- Platforms: Linux/macOS/Windows (Docker-based)

Motivation
In agent-driven workflows (Aider/Crush/Codex) with the Rust toolchain sidecar, common developer commands should “just work”:
- cargo nextest run --no-fail-fast must work (cargo-nextest available).
- cargo test --no-fail-fast must not fail with permission errors (writable registry and git caches).
- cargo clippy --all-targets --all-features -- -D warnings must work (clippy installed).
This spec defines a robust Rust toolchain sidecar that satisfies these requirements and adds pragmatic developer tooling and mounts to make day‑to‑day Rust development fast and reliable.

Goals
- Provide a first-party Rust toolchain image with:
  - rustfmt, clippy, rust-src, llvm-tools-preview installed via rustup.
  - cargo-nextest preinstalled.
  - Common system dependencies for building typical Rust crates.
- Ensure writable and persistent cargo caches to avoid permission errors and accelerate builds.
- Maintain cross-platform behavior and current security posture (no privileged containers; no Docker socket).
- Provide sensible defaults and opt-ins for advanced workflows (SSH agent forwarding, sccache, fast linkers, coverage tools).

Non-Goals
- Changing the agent-shim-to-sidecar invocation model.
- Managing user project-specific rustup “toolchain files” (left to users).
- Shipping an exhaustive set of cargo subcommands in the base toolchain; only a curated minimal-but-useful set is included by default.

High-Level Design
- Image: aifo-rust-toolchain, built on top of official rust:<tag> images, with clippy/fmt/rust-src/llvm-tools components and cargo-nextest baked in.
- Caches: Prefer mounting host $HOME/.cargo/registry and $HOME/.cargo/git into /usr/local/cargo/{registry,git}. Fallback to named volumes.
- CARGO_HOME=/usr/local/cargo; PATH includes /usr/local/cargo/bin (in official images by default).
- Runtime options via environment variables to control image selection, cache strategy, SSH agent forwarding, sccache, linkers, proxy behavior, etc.
- Fallback bootstrap: If the official rust image (not our toolchain) is used, auto-install missing cargo-nextest/clippy at first invocation, caching under CARGO_HOME.

Image Specification (aifo-rust-toolchain)
Base
- FROM ${REGISTRY_PREFIX}rust:<RUST_TAG> (bookworm or slim variants). Multi-arch: amd64 and arm64.

Rust components (preinstalled via rustup)
- clippy
- rustfmt
- rust-src
- llvm-tools-preview

Cargo tools (preinstalled; pinned via --locked where applicable)
- cargo-nextest
- Optionally (CI flavor or future variants):
  - cargo-llvm-cov (requires llvm-tools)
  - cargo-edit (cargo add/rm/set-version)
  - cargo-outdated, cargo-udeps
  - cargo-deny, cargo-audit
  - cargo-expand

System packages (to satisfy common crate build requirements)
- build-essential (gcc, g++, make), pkg-config
- cmake, ninja
- clang, libclang-dev (bindgen)
- python3 (some build scripts)
- libssl-dev, zlib1g-dev, libsqlite3-dev, libcurl4-openssl-dev
- git, ca-certificates, curl, tzdata, locales (UTF-8)

Environment defaults inside image
- LANG=C.UTF-8
- CARGO_HOME=/usr/local/cargo (PATH already includes /usr/local/cargo/bin in official rust images)
- Sensible defaults encouraged by documentation (not enforced): CARGO_NET_GIT_FETCH_WITH_CLI=true, CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse

Runtime Behavior (Sidecar)
Mounts and cache strategy
- Workdir:
  - -v <host_repo>:/workspace
  - -w /workspace
- Cargo caches (unless disabled via --no-toolchain-cache or AIFO_TOOLCHAIN_NO_CACHE=1):
  - Default (host caches preferred):
    - If $HOME/.cargo/registry exists: -v $HOME/.cargo/registry:/usr/local/cargo/registry
    - If $HOME/.cargo/git exists: -v $HOME/.cargo/git:/usr/local/cargo/git
    - For any missing host path, fall back to named volume for that path:
      - -v aifo-cargo-registry:/usr/local/cargo/registry
      - -v aifo-cargo-git:/usr/local/cargo/git
  - Forced volumes (opt-in): If AIFO_TOOLCHAIN_RUST_USE_DOCKER_VOLUMES=1, use the named volumes even when host caches exist.
- Optional mounts:
  - If AIFO_TOOLCHAIN_RUST_USE_HOST_CONFIG=1 and $HOME/.cargo/config or config.toml exists, mount read-only into /usr/local/cargo/config.toml (or a merge-friendly path) to honor mirrors and custom configuration.
  - SSH agent forward (opt-in): If AIFO_TOOLCHAIN_SSH_FORWARD=1 and SSH_AUTH_SOCK is set, mount the socket path and -e SSH_AUTH_SOCK=..., enabling git+ssh dependency access.
  - sccache (opt-in): If AIFO_RUST_SCCACHE=1, mount a named volume aifo-sccache or a host directory $AIFO_RUST_SCCACHE_DIR, set RUSTC_WRAPPER=sccache, configure SCCACHE_DIR.

Environment variables passed into the sidecar (selected)
- Always:
  - HOME=/home/coder (set by image entrypoint)
  - GNUPGHOME=/home/coder/.gnupg
  - CARGO_HOME=/usr/local/cargo
- Optional pass-through (if defined on host/agent process):
  - HTTP_PROXY, HTTPS_PROXY, NO_PROXY
  - CARGO_NET_GIT_FETCH_WITH_CLI, CARGO_REGISTRIES_CRATES_IO_PROTOCOL
  - RUST_BACKTRACE (default to 1 if not set; best-effort)
  - SSH_AUTH_SOCK (if AIFO_TOOLCHAIN_SSH_FORWARD=1)
  - sccache related envs (if AIFO_RUST_SCCACHE=1)
  - AIFO_SESSION_NETWORK (already used to join the aifo-net-<sid> network)

Image selection logic
- Environment overrides:
  - AIFO_RUST_TOOLCHAIN_IMAGE: full image reference override (e.g., your registry mirror).
  - AIFO_RUST_TOOLCHAIN_VERSION: tag to select (e.g., 1.80, 1.80.1); default "latest".
- Default:
  - Use aifo-rust-toolchain:<version or latest>.
- Fallback:
  - If the toolchain image is unavailable or AIFO_RUST_TOOLCHAIN_USE_OFFICIAL=1 is set, use rust:<version>-slim (or rust:<major>-bookworm) and apply runtime fallback bootstrap to install missing cargo-nextest/clippy.

Fallback bootstrap (when using official rust images)
- On the first cargo exec within the running sidecar:
  - If cargo-nextest missing: cargo install cargo-nextest --locked.
  - If clippy missing: rustup component add clippy rustfmt.
- Cache goes to /usr/local/cargo; mounts ensure persistence; network failures surface as clear errors.
- Only engaged when our toolchain image is not used.

Operational knobs (environment variables)
- AIFO_RUST_TOOLCHAIN_IMAGE
- AIFO_RUST_TOOLCHAIN_VERSION
- AIFO_RUST_TOOLCHAIN_USE_OFFICIAL=1 (force fallback to official rust image)
- AIFO_TOOLCHAIN_NO_CACHE=1 (honors existing --no-toolchain-cache)
- AIFO_TOOLCHAIN_RUST_USE_HOST_CARGO=1 (default: enabled; prefer host caches)
- AIFO_TOOLCHAIN_RUST_USE_DOCKER_VOLUMES=1 (force named volumes)
- AIFO_TOOLCHAIN_RUST_USE_HOST_CONFIG=1 (mount host cargo config read-only if present)
- AIFO_TOOLCHAIN_SSH_FORWARD=1 (enable SSH agent socket mount)
- AIFO_RUST_SCCACHE=1 (enable sccache; optional AIFO_RUST_SCCACHE_DIR to pick cache location)
- AIFO_RUST_LINKER=lld|mold (opt-in fast linker; set RUSTFLAGS accordingly)
- HTTP_PROXY, HTTPS_PROXY, NO_PROXY
- CARGO_NET_GIT_FETCH_WITH_CLI, CARGO_REGISTRIES_CRATES_IO_PROTOCOL

Security and isolation
- No privileged mode; no Docker socket mounts.
- AppArmor/seccomp/cgroupns behavior unchanged (respect desired apparmor profile if available).
- SSH agent forwarding is opt-in; known_hosts must be handled appropriately (mount known_hosts or configure a toolchain-scoped known_hosts file).
- Only $HOME/.cargo subdirs are mounted by default; avoid mounting entire home directories.

Phased Plan

Phase 0 — Image creation
- Add toolchains/rust/Dockerfile:
  - ARG RUST_TAG (default e.g., 1-bookworm or 1.80-bookworm).
  - Install rustup components: clippy, rustfmt, rust-src, llvm-tools-preview.
  - cargo install cargo-nextest --locked.
  - Install system packages: build-essential, pkg-config, cmake, ninja, clang, libclang-dev, python3, libssl-dev, zlib1g-dev, libsqlite3-dev, libcurl4-openssl-dev, git, ca-certificates, curl, tzdata, locales.
  - Set LANG=C.UTF-8.
  - Ensure PATH includes /usr/local/cargo/bin (default for official rust images).
- Build multi-arch image (amd64, arm64).

Phase 1 — Makefile integration (build/publish)
- Add targets:
  - build-toolchain-rust: builds aifo-rust-toolchain:latest (and optionally aifo-rust-toolchain:<version> when configured).
  - rebuild-toolchain-rust: same with --no-cache.
  - publish-toolchain-rust: buildx multi-arch and push if REGISTRY is set; otherwise produce an OCI archive in dist/.
- Mirror structure and behavior from existing publish-toolchain-cpp targets.

Phase 2 — Runtime image selection in code
- In src/toolchain.rs:
  - default_toolchain_image("rust"):
    - If AIFO_RUST_TOOLCHAIN_IMAGE set: use it.
    - Else if AIFO_RUST_TOOLCHAIN_VERSION set: aifo-rust-toolchain:<version>.
    - Else: aifo-rust-toolchain:latest.
  - default_toolchain_image_for_version("rust", v):
    - aifo-rust-toolchain:<v>.
  - Provide graceful fallback to official rust images if our toolchain image is absent or AIFO_RUST_TOOLCHAIN_USE_OFFICIAL=1.

Phase 3 — Mount strategy (writable caches) and env propagation
- In build_sidecar_run_preview(kind="rust"):
  - Keep existing CARGO_HOME=/usr/local/cargo.
  - If no_cache: do not mount caches.
  - Else if AIFO_TOOLCHAIN_RUST_USE_DOCKER_VOLUMES=1: use named volumes for registry/git.
  - Else (default): try host mounts $HOME/.cargo/{registry,git}; fallback per-path to named volumes if host path missing.
  - Support optional mounts:
    - Host cargo config (read-only) when AIFO_TOOLCHAIN_RUST_USE_HOST_CONFIG=1.
    - SSH agent socket when AIFO_TOOLCHAIN_SSH_FORWARD=1.
    - sccache cache with RUSTC_WRAPPER when AIFO_RUST_SCCACHE=1.
  - Pass through proxy envs when defined.

Phase 4 — Fallback bootstrap on official rust images (optional)
- In build_sidecar_exec_preview(kind="rust"):
  - If image looks like rust:<...> (heuristic or env flag), wrap the requested command with a short shell snippet:
    - Check cargo nextest/clippy availability; install missing ones (respect --locked for nextest; rustup for clippy/fmt).
  - Cache to /usr/local/cargo; messages should be clear in verbose mode; errors bubble up.

Phase 5 — Linker and coverage (opt-in)
- If AIFO_RUST_LINKER=lld|mold:
  - Set RUSTFLAGS appropriately (e.g., -Clinker=clang -Clink-arg=-fuse-ld=lld).
- For coverage (CI flavor or later version):
  - Provide cargo-llvm-cov in image variant; otherwise document usage and prerequisites (llvm-tools).
  - Optionally mount a named volume (aifo-llvm) if large data needs caching.

Phase 6 — Testing and validation
- Manual validation inside an agent pane with --toolchain rust:
  - /run cargo nextest run --no-fail-fast
  - /run cargo test --no-fail-fast
  - /run cargo clippy --all-targets --all-features -- -D warnings
  - /run cargo fmt -- --check
- E2E (ignored tests or scripts):
  - Start rust sidecar (toolchain_start_session) and via toolexec:
    - cargo -V; cargo nextest -V; rustup component list | grep clippy
  - Exercise both cache modes (host mounts; forced volumes).
  - Exercise SSH forwarding against a private git dep (optional).
  - Validate on amd64 and arm64.

Phase 7 — Documentation
- Update docs/TOOLCHAINS.md:
  - Image selection rules and env overrides.
  - Cache mount strategy and how to force volumes.
  - SSH agent forwarding and known_hosts guidance.
  - sccache enablement and cache location.
  - Proxy env pass-through and sparse/CLI git fetching.
  - Troubleshooting section for permissions (ensure host $HOME/.cargo exists or force volumes).

Phase 8 — Rollout
- Build and publish aifo-rust-toolchain (latest + optional versioned tags).
- Adjust defaults in code to prefer our toolchain image.
- Monitor for regressions; retain fallback bootstrap for official images to avoid breaking existing deployments.

Error Handling and Exit Codes
- Image not found: clear error pointing to Makefile targets (build-toolchain-rust) or env override (AIFO_RUST_TOOLCHAIN_IMAGE).
- Permission failures on /usr/local/cargo/*:
  - Guidance to enable host cache mounts (default) or create host $HOME/.cargo directories; or set AIFO_TOOLCHAIN_RUST_USE_DOCKER_VOLUMES=1.
- Network errors during fallback bootstrap: surface cleanly; recommend using our toolchain image to avoid on-the-fly installs.

Security Posture
- No new privileges; same AppArmor/seccomp/cgroupns behavior as existing sidecars.
- SSH forwarding is explicit opt-in; avoid mounting arbitrary $HOME by default.
- Supply chain: use --locked for cargo installs; consider pinning versions for CI variants.

Acceptance Criteria
- In agent panes with --toolchain rust:
  - cargo nextest run --no-fail-fast works (cargo-nextest present).
  - cargo test --no-fail-fast works without registry permission errors.
  - cargo clippy --all-targets --all-features -- -D warnings works (clippy installed).
- Default behavior prefers host cache mounts and falls back to volumes gracefully.
- Behavior consistent across amd64 and arm64.

Risks and Mitigations
- Host cache mount absent: per-path fallback to volumes.
- Official rust image use: engage fallback bootstrap; recommend switching to aifo-rust-toolchain.
- SSH agent/known_hosts complexity: keep opt-in and documented.
- Disk usage growth: sccache opt-in and volume-based; document cleanup commands.

Versioning
- This document defines v1 of the Rust toolchain sidecar.
- Future variants:
  - :slim (fewer system deps)
  - :ci (adds cargo-* QA tools, linkers, coverage)
- Subsequent revisions (v1.1, v2) should document backward compatibility and any behavioral changes.

Appendix: Summary of Key Environment Variables
- AIFO_RUST_TOOLCHAIN_IMAGE, AIFO_RUST_TOOLCHAIN_VERSION, AIFO_RUST_TOOLCHAIN_USE_OFFICIAL
- AIFO_TOOLCHAIN_NO_CACHE
- AIFO_TOOLCHAIN_RUST_USE_HOST_CARGO (default on), AIFO_TOOLCHAIN_RUST_USE_DOCKER_VOLUMES
- AIFO_TOOLCHAIN_RUST_USE_HOST_CONFIG
- AIFO_TOOLCHAIN_SSH_FORWARD, SSH_AUTH_SOCK
- AIFO_RUST_SCCACHE, AIFO_RUST_SCCACHE_DIR
- AIFO_RUST_LINKER
- HTTP_PROXY, HTTPS_PROXY, NO_PROXY
- CARGO_NET_GIT_FETCH_WITH_CLI, CARGO_REGISTRIES_CRATES_IO_PROTOCOL
