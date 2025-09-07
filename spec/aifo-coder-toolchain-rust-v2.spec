AIFO Coder: Rust Toolchain Sidecar (v2) – Comprehensive, Production‑Ready Specification

Status
- Stage: v2 specification (implementation-ready; clarifies v1 and aligns with codebase intent)
- Scope: Rust toolchain image supply, sidecar runtime mounts/env, fallback bootstrap, developer tooling
- Compatibility: Backward compatible with the existing sidecar model; additive improvements and safe fallbacks
- Platforms: Linux/macOS/Windows (Docker-based)

Motivation
Agent-driven Rust workflows (Aider/Crush/Codex) must “just work” inside the Rust toolchain sidecar:
- cargo nextest run --no-fail-fast succeeds (cargo-nextest available).
- cargo test --no-fail-fast succeeds (writable, persistent cargo registry and git caches).
- cargo clippy --all-targets --all-features -- -D warnings succeeds (clippy installed).
We also want sensible opt-ins for SSH-based git dependencies, sccache, fast linkers, coverage, and corporate proxies.

Key v2 Clarifications vs v1
- Default CARGO_HOME path inside the sidecar is /home/coder/.cargo (not /usr/local/cargo). This avoids permission problems when running as a non-root uid:gid (the default).
- Host caches are preferred by default for registry and git; per-path fallback to named volumes.
- Image selection honors environment overrides and prefers aifo-rust-toolchain[:tag], with graceful fallback to official rust images and a runtime bootstrap for missing tools.
- Optional mounts and env are explicitly supported: host cargo config, SSH agent forwarding, sccache, fast linkers, proxy envs.
- Testable, cross-platform behavior; clear error handling and acceptance criteria.

Goals
- Provide a first-party Rust toolchain image with:
  - rustfmt, clippy, rust-src, llvm-tools-preview preinstalled via rustup.
  - cargo-nextest preinstalled via cargo install --locked.
  - Common system dependencies for typical Rust crates.
- Ensure writable and persistent cargo caches by default, with host-cache preference.
- Maintain existing security posture (no privileged containers; no Docker socket).
- Provide practical opt-ins for SSH agent forwarding, sccache, fast linkers (lld/mold), and coverage tools.

Non-Goals
- Changing the agent-shim-to-sidecar invocation model or proxy protocol.
- Managing project-specific rustup toolchains or rust-toolchain.toml behavior (left to users).
- Shipping an exhaustive set of cargo subcommands in the base toolchain; only a curated set is included.

High-Level Design
- Toolchain image: aifo-rust-toolchain:<tag>, built on official rust:<tag> images, with rustfmt/clippy/rust-src/llvm-tools and cargo-nextest baked in.
- Caches: Prefer mounting host $HOME/.cargo/registry and $HOME/.cargo/git into /home/coder/.cargo/{registry,git}. Per-path fallback to named volumes aifo-cargo-registry and aifo-cargo-git.
- Environment: CARGO_HOME=/home/coder/.cargo; PATH includes /usr/local/cargo/bin (present by default in official rust images).
- Optional mounts/env: SSH agent forwarding, sccache, cargo config (read-only), proxy env, fast linkers.
- Fallback bootstrap: If an official rust image (not our toolchain) is used, first exec attempts to install cargo-nextest and clippy into CARGO_HOME.

Image Specification (aifo-rust-toolchain)
Base
- FROM ${REGISTRY_PREFIX}rust:<RUST_TAG> (bookworm or slim variants). Multi-arch: amd64 and arm64.

Rust components (preinstalled via rustup)
- clippy
- rustfmt
- rust-src
- llvm-tools-preview

Cargo tools (preinstalled; versions pinned via --locked where applicable)
- cargo-nextest
- Optional (CI flavor or future variants):
  - cargo-llvm-cov (requires llvm-tools)
  - cargo-edit
  - cargo-outdated, cargo-udeps
  - cargo-deny, cargo-audit
  - cargo-expand
  - sccache (if included, configure via env and mounts at runtime)

System packages (to satisfy common crate requirements)
- build-essential (gcc, g++, make), pkg-config
- cmake, ninja
- clang, libclang-dev (bindgen)
- python3 (many build.rs scripts)
- libssl-dev, zlib1g-dev, libsqlite3-dev, libcurl4-openssl-dev
- git, ca-certificates, curl, tzdata, locales (UTF-8)

Environment defaults inside the image
- LANG=C.UTF-8
- CARGO_HOME=/home/coder/.cargo
- PATH includes /usr/local/cargo/bin by default (official rust images)

Runtime Behavior (Sidecar)
Mounts and cache strategy
- Workdir:
  - -v <host_repo>:/workspace
  - -w /workspace
- Cargo caches (unless disabled by --no-toolchain-cache or AIFO_TOOLCHAIN_NO_CACHE=1):
  - Default (host caches preferred, per-path):
    - If $HOME/.cargo/registry exists: -v $HOME/.cargo/registry:/home/coder/.cargo/registry
    - Else: -v aifo-cargo-registry:/home/coder/.cargo/registry
    - If $HOME/.cargo/git exists: -v $HOME/.cargo/git:/home/coder/.cargo/git
    - Else: -v aifo-cargo-git:/home/coder/.cargo/git
  - Forced volumes (opt-in): If AIFO_TOOLCHAIN_RUST_USE_DOCKER_VOLUMES=1, use named volumes even if host caches exist.
- Optional mounts/env:
  - Host cargo config (read-only): If AIFO_TOOLCHAIN_RUST_USE_HOST_CONFIG=1 and $HOME/.cargo/config or config.toml exists, mount as /home/coder/.cargo/config.toml inside the container.
  - SSH agent forward: If AIFO_TOOLCHAIN_SSH_FORWARD=1 and SSH_AUTH_SOCK is set, mount the socket path and export SSH_AUTH_SOCK (git+ssh deps).
  - sccache (opt-in): If AIFO_RUST_SCCACHE=1, mount aifo-sccache (named volume) or $AIFO_RUST_SCCACHE_DIR (host path) and export RUSTC_WRAPPER=sccache and SCCACHE_DIR.
  - Proxies: if HTTP_PROXY/HTTPS_PROXY/NO_PROXY defined on host/agent, pass through to the sidecar.
  - Fast linkers: If AIFO_RUST_LINKER=lld|mold, export RUSTFLAGS to select the linker (e.g., -Clinker=clang -Clink-arg=-fuse-ld=lld).

Environment variables passed into the sidecar (selected)
- Always:
  - HOME=/home/coder
  - GNUPGHOME=/home/coder/.gnupg
  - CARGO_HOME=/home/coder/.cargo
- Optional pass-through (if set on host/agent process):
  - HTTP_PROXY, HTTPS_PROXY, NO_PROXY
  - CARGO_NET_GIT_FETCH_WITH_CLI, CARGO_REGISTRIES_CRATES_IO_PROTOCOL
  - RUST_BACKTRACE (default to 1 if not set; best-effort)
  - SSH_AUTH_SOCK (if AIFO_TOOLCHAIN_SSH_FORWARD=1)
  - sccache envs (if AIFO_RUST_SCCACHE=1)
  - AIFO_SESSION_NETWORK (sidecars join the aifo-net-<sid> network)

Image selection logic
- Environment overrides:
  - AIFO_RUST_TOOLCHAIN_IMAGE: full image reference override (e.g., your registry mirror).
  - AIFO_RUST_TOOLCHAIN_VERSION: tag selector (e.g., 1.80, 1.80.1); default "latest".
- Default:
  - Use aifo-rust-toolchain:<version|latest>.
- Fallback:
  - If our toolchain image is unavailable or AIFO_RUST_TOOLCHAIN_USE_OFFICIAL=1 is set, use rust:<version>-slim (or rust:<major>-bookworm) and engage runtime fallback bootstrap (below).

Fallback bootstrap (when using official rust images)
- On the first cargo exec within the running sidecar:
  - If cargo-nextest missing: cargo install cargo-nextest --locked.
  - If clippy missing: rustup component add clippy rustfmt.
- Cache is under /home/coder/.cargo; mounts ensure persistence; network failures surface as clear errors.
- Only engaged when our toolchain image is not used; messaging is terse unless verbose.

Operational knobs (environment variables)
- AIFO_RUST_TOOLCHAIN_IMAGE
- AIFO_RUST_TOOLCHAIN_VERSION
- AIFO_RUST_TOOLCHAIN_USE_OFFICIAL=1 (force fallback to official rust image)
- AIFO_TOOLCHAIN_NO_CACHE=1 (honors existing --no-toolchain-cache)
- AIFO_TOOLCHAIN_RUST_USE_HOST_CARGO=1 (default: enabled; prefer host caches)
- AIFO_TOOLCHAIN_RUST_USE_DOCKER_VOLUMES=1 (force named volumes)
- AIFO_TOOLCHAIN_RUST_USE_HOST_CONFIG=1 (mount host cargo config read-only if present)
- AIFO_TOOLCHAIN_SSH_FORWARD=1 (enable SSH agent socket mount if SSH_AUTH_SOCK is set)
- AIFO_RUST_SCCACHE=1 (enable sccache; optional AIFO_RUST_SCCACHE_DIR)
- AIFO_RUST_LINKER=lld|mold (opt-in fast linker; sets RUSTFLAGS)
- HTTP_PROXY, HTTPS_PROXY, NO_PROXY
- CARGO_NET_GIT_FETCH_WITH_CLI, CARGO_REGISTRIES_CRATES_IO_PROTOCOL
- AIFO_TOOLCHAIN_VERBOSE=1 (extra logging during sidecar setup; maps to existing verbose)

Security and isolation
- No privileged mode; no Docker socket mounts.
- AppArmor/seccomp/cgroupns behavior unchanged (desired apparmor profile applied when available).
- SSH agent forwarding is explicit opt-in; known_hosts must be handled explicitly (mount known_hosts or configure container-scoped known_hosts).
- Only $HOME/.cargo subdirs are mounted by default; avoid mounting the entire home.

Phased Plan

Phase 0 — Image creation
- Add toolchains/rust/Dockerfile:
  - ARG RUST_TAG (default e.g., 1-bookworm or 1.80-bookworm).
  - Install rustup components: clippy, rustfmt, rust-src, llvm-tools-preview.
  - cargo install cargo-nextest --locked.
  - Install system packages: build-essential, pkg-config, cmake, ninja, clang, libclang-dev, python3, libssl-dev, zlib1g-dev, libsqlite3-dev, libcurl4-openssl-dev, git, ca-certificates, curl, tzdata, locales.
  - Set LANG=C.UTF-8.
  - Verify PATH includes /usr/local/cargo/bin (official rust images provide this).
- Build multi-arch image (amd64, arm64).

Phase 1 — Makefile integration (build/publish)
- Add targets:
  - build-toolchain-rust: builds aifo-rust-toolchain:latest or :<version> when configured.
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
  - Provide graceful fallback to official rust images if our toolchain image is absent or AIFO_RUST_TOOLCHAIN_USE_OFFICIAL=1 is set.

Phase 3 — Mount strategy (writable caches) and env propagation
- In build_sidecar_run_preview(kind="rust"):
  - Set CARGO_HOME=/home/coder/.cargo.
  - If no_cache: do not mount caches.
  - Else if AIFO_TOOLCHAIN_RUST_USE_DOCKER_VOLUMES=1: use named volumes aifo-cargo-registry:/home/coder/.cargo/registry and aifo-cargo-git:/home/coder/.cargo/git.
  - Else (default): try host mounts $HOME/.cargo/{registry,git}; fallback per-path to named volumes if host path missing.
  - Support optional mounts:
    - Host cargo config (read-only) when AIFO_TOOLCHAIN_RUST_USE_HOST_CONFIG=1 (mounted to /home/coder/.cargo/config.toml).
    - SSH agent socket when AIFO_TOOLCHAIN_SSH_FORWARD=1 and SSH_AUTH_SOCK is defined.
    - sccache cache with RUSTC_WRAPPER when AIFO_RUST_SCCACHE=1.
  - Pass through proxy envs when defined.
  - If AIFO_RUST_LINKER=lld|mold: export appropriate RUSTFLAGS.

Phase 4 — Fallback bootstrap on official rust images
- In build_sidecar_exec_preview(kind="rust") or exec path:
  - If image is rust:<...> (heuristic or env flag), wrap the requested command with:
    - Check cargo nextest/clippy availability; install missing ones (respect --locked; rustup for clippy/fmt).
  - Cache to /home/coder/.cargo; messages are terse unless verbose; errors bubble up.

Phase 5 — Validation of PATH and ownership
- Ensure PATH includes /usr/local/cargo/bin (official images do).
- Running as --user uid:gid (host) remains the default. Using /home/coder/.cargo avoids root-owned permission issues.

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
  - Cache mount strategy (host-cache default; per-path fallback to named volumes).
  - SSH agent forwarding and known_hosts guidance.
  - sccache enablement and cache location.
  - Proxy env pass-through and sparse/CLI git fetching.
  - Troubleshooting for permissions (create host $HOME/.cargo/{registry,git} or force volumes).

Phase 8 — Rollout
- Build and publish aifo-rust-toolchain (latest + optional versioned tags).
- Adjust defaults in code to prefer our toolchain image and /home/coder/.cargo for CARGO_HOME.
- Monitor for regressions; retain fallback bootstrap for official images to avoid breaking existing deployments.

Error Handling and Exit Codes
- Image not found: clear error pointing to Makefile targets (build-toolchain-rust) or env override (AIFO_RUST_TOOLCHAIN_IMAGE).
- Permission failures on cargo caches:
  - Prefer host cache mounts by default; otherwise instruct to create host ~/.cargo/{registry,git} or set AIFO_TOOLCHAIN_RUST_USE_DOCKER_VOLUMES=1.
- Network errors during fallback bootstrap: surface cleanly; recommend using our toolchain image to avoid on-the-fly installs.
- Orchestrator not found (for agents): unchanged from current behavior.

Security Posture
- No new privileges; same AppArmor/seccomp/cgroupns behavior as existing sidecars.
- SSH forwarding is opt-in; avoid mounting arbitrary $HOME by default.
- Supply chain: use --locked for cargo installs; consider pinning versions for CI variants.

Acceptance Criteria
- In agent panes with --toolchain rust:
  - cargo nextest run --no-fail-fast works (cargo-nextest present).
  - cargo test --no-fail-fast works without registry permission errors.
  - cargo clippy --all-targets --all-features -- -D warnings works (clippy installed).
- Default behavior prefers host cache mounts and falls back to volumes per-path gracefully.
- Behavior consistent across amd64 and arm64.

Risks and Mitigations
- Host cache mount absent: per-path fallback to volumes.
- Official rust image use: engage fallback bootstrap; recommend switching to aifo-rust-toolchain.
- SSH agent/known_hosts complexity: keep opt-in and documented.
- Disk usage growth: sccache opt-in and volume-based; document cleanup commands.

Versioning
- This document defines v2 of the Rust toolchain sidecar specification.
- Future variants:
  - :slim (fewer system deps)
  - :ci (adds cargo-* QA tools, linkers, coverage)
- Subsequent revisions should document backward compatibility and any behavioral changes.

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
