AIFO Coder: Rust Toolchain Sidecar (v4) – Comprehensive, Production‑Ready Specification

Status
- Stage: v4 specification (implementation-ready; consolidates v3 and clarifications into a precise plan)
- Scope: Rust toolchain image supply, runtime mounts/env, fallback bootstrap, developer tooling, cross‑platform behavior
- Compatibility: Backward compatible with existing sidecar model; safe additive improvements with explicit fallbacks and migration notes
- Platforms: Linux/macOS/Windows (Docker-based; Linux adds optional unix socket transport)

Motivation
Agent-driven Rust workflows (Aider/Crush/Codex and generic toolchain “run” mode) must be reliable, fast, and reproducible:
- cargo nextest run --no-fail-fast succeeds (cargo-nextest available).
- cargo test --no-fail-fast succeeds with writable, persistent cargo registry and git caches.
- cargo clippy --all-targets --all-features -- -D warnings succeeds (clippy installed).
We also need explicit, opt-in support for SSH-based git dependencies, sccache, fast linkers (lld/mold), coverage tooling, and corporate proxies.

Guiding Principles
- Zero-surprise defaults: non-root uid:gid works; caches are writable and persistent.
- Prefer host caches when safe, falling back per-path to named Docker volumes.
- Explicit, discoverable knobs for optional mounts and behaviors; cross-arch correctness (amd64/arm64).
- Clear errors with minimal noise; verbose mode provides actionable diagnostics.
- Security posture unchanged: no privileged containers; no Docker socket mounts; AppArmor profile when available.

Key v4 Clarifications vs v3
- CARGO_HOME inside sidecar is /home/coder/.cargo (not /usr/local/cargo).
  - Avoids permission issues with non-root uid:gid.
  - PATH MUST prepend $CARGO_HOME/bin and SHOULD retain /usr/local/cargo/bin as fallback.
- Host caches are preferred per-path by default:
  - If $HOME/.cargo/registry exists: mount to /home/coder/.cargo/registry; else use aifo-cargo-registry volume.
  - If $HOME/.cargo/git exists: mount to /home/coder/.cargo/git; else use aifo-cargo-git volume.
  - Windows hosts: named volumes recommended by default; host cache mounts opt-in when path semantics are known-safe.
- Image selection and fallbacks:
  - Default to aifo-coder-toolchain-rust:<version|latest>. Overrides via AIFO_RUST_TOOLCHAIN_IMAGE (full ref) and AIFO_RUST_TOOLCHAIN_VERSION (tag).
  - Fallback to official rust:<version>-slim (or rust:<major>-bookworm) when overridden or when our toolchain image is unavailable; engage runtime bootstrap.
- Optional mounts/env (opt-in):
  - Host cargo config (read-only), SSH agent socket, sccache, proxies, fast linkers (lld/mold via RUSTFLAGS).
- Bootstrap precisely defined and idempotent when using official rust images (cargo-nextest + clippy/rustfmt).
- RUST_BACKTRACE defaults to 1 (best-effort) for better diagnostics.
- Networking on Linux: host.docker.internal:host-gateway is added on demand to reach the host agent proxy when using TCP.

Goals
- First-party toolchain image: aifo-coder-toolchain-rust:<tag>, preinstalling:
  - rustup components: clippy, rustfmt, rust-src, llvm-tools-preview.
  - cargo-nextest via cargo install --locked.
  - System dependencies for common crates: gcc/g++/make, pkg-config, cmake, ninja, clang/libclang-dev, python3, libssl-dev, zlib1g-dev, libsqlite3-dev, libcurl4-openssl-dev, git, ca-certificates, curl, tzdata, locales.
- Writable, persistent cargo caches by default; host cache preference with per-path fallback to named volumes.
- Security posture maintained; no privileged mode; no Docker socket mounts; AppArmor profile when available.
- Robust opt-ins: SSH agent forwarding, sccache, fast linkers (lld/mold), coverage, proxies.
- Cross-platform correctness (Linux/macOS/Windows; amd64/arm64).

Non-Goals
- Changing shim protocol, proxy model, or agent orchestration mechanics.
- Managing project-specific rustup toolchains beyond honoring rust-toolchain.toml inside the sidecar.
- Shipping an exhaustive cargo-* suite beyond a curated set (nextest is mandatory; others in CI variants).

High-Level Design
- Toolchain image: aifo-coder-toolchain-rust:<tag>, derived from official rust:<tag> images.
  - Preinstalled rust components: clippy, rustfmt, rust-src, llvm-tools-preview.
  - cargo-nextest installed via cargo install --locked.
  - PATH includes /home/coder/.cargo/bin (prepend) and keeps /usr/local/cargo/bin as fallback.
  - CARGO_HOME=/home/coder/.cargo.
- Caches:
  - Default: mount host $HOME/.cargo/{registry,git} into /home/coder/.cargo/{registry,git}, with per-path fallback to named volumes aifo-cargo-registry and aifo-cargo-git.
  - Forced volumes: AIFO_TOOLCHAIN_RUST_USE_DOCKER_VOLUMES=1 uses named volumes even when host caches exist.
  - Windows hosts: default to named volumes; host cache mounts only if explicitly opted in and path handling is safe.
- Environment defaults:
  - HOME=/home/coder; GNUPGHOME=/home/coder/.gnupg; CARGO_HOME=/home/coder/.cargo.
  - PATH="$CARGO_HOME/bin:/usr/local/cargo/bin:$PATH".
  - RUST_BACKTRACE defaults to 1 when unset.
- Optional mounts/env:
  - Host cargo config (read-only).
  - SSH agent socket (bind).
  - sccache cache directory or named volume; set RUSTC_WRAPPER and SCCACHE_DIR.
  - Proxy envs pass-through.
  - Fast linkers: AIFO_RUST_LINKER=lld|mold via RUSTFLAGS.

Image Specification (aifo-coder-toolchain-rust)
Base
- FROM ${REGISTRY_PREFIX}rust:<RUST_TAG> (bookworm/slim variants). Multi-arch: amd64 and arm64.

Rust components (preinstalled via rustup)
- clippy
- rustfmt
- rust-src
- llvm-tools-preview

Cargo tools (preinstalled; pinned where applicable)
- cargo-nextest (cargo install --locked)

Optional CI variant (future)
- cargo-llvm-cov (requires llvm-tools), cargo-edit, cargo-outdated, cargo-udeps, cargo-deny, cargo-audit, cargo-expand
- lld and/or mold installed for linker experiments
- sccache (optional; runtime opt-in supported regardless)

System packages (typical crate requirements)
- build-essential (gcc, g++, make), pkg-config
- cmake, ninja
- clang, libclang-dev (bindgen)
- python3
- libssl-dev, zlib1g-dev, libsqlite3-dev, libcurl4-openssl-dev
- git, ca-certificates, curl, tzdata, locales (UTF-8)

Environment defaults inside the image
- LANG=C.UTF-8
- CARGO_HOME=/home/coder/.cargo
- PATH includes /home/coder/.cargo/bin (prepend) and /usr/local/cargo/bin (fallback)

Runtime Behavior (Sidecar)
Mounts and cache strategy
- Workdir:
  - -v <host_repo>:/workspace
  - -w /workspace
- Cargo caches (unless disabled by --no-toolchain-cache or AIFO_TOOLCHAIN_NO_CACHE=1):
  - Default: host caches preferred, per-path:
    - If $HOME/.cargo/registry exists: -v $HOME/.cargo/registry:/home/coder/.cargo/registry
      Else: -v aifo-cargo-registry:/home/coder/.cargo/registry
    - If $HOME/.cargo/git exists: -v $HOME/.cargo/git:/home/coder/.cargo/git
      Else: -v aifo-cargo-git:/home/coder/.cargo/git
  - Forced volumes: AIFO_TOOLCHAIN_RUST_USE_DOCKER_VOLUMES=1 forces named volumes even if host caches exist.
  - Windows hosts: default to named volumes; enable host mounts only when path semantics are verified safe.
- Optional mounts/env:
  - Host cargo config (read-only): If AIFO_TOOLCHAIN_RUST_USE_HOST_CONFIG=1 and $HOME/.cargo/config(.toml) exists, mount as /home/coder/.cargo/config.toml.
  - SSH agent forward: If AIFO_TOOLCHAIN_SSH_FORWARD=1 and SSH_AUTH_SOCK is set, bind-mount the socket path and export SSH_AUTH_SOCK.
  - sccache: If AIFO_RUST_SCCACHE=1,
    - If $AIFO_RUST_SCCACHE_DIR set: -v $AIFO_RUST_SCCACHE_DIR:/home/coder/.cache/sccache
    - Else: -v aifo-sccache:/home/coder/.cache/sccache
    - Export RUSTC_WRAPPER=sccache and SCCACHE_DIR=/home/coder/.cache/sccache
  - Proxies: If HTTP_PROXY/HTTPS_PROXY/NO_PROXY defined on host, pass through into sidecar.
  - Fast linkers: If AIFO_RUST_LINKER=lld|mold, export RUSTFLAGS (e.g., -Clinker=clang -Clink-arg=-fuse-ld=lld). CI images MUST include requested linker.
- Other env:
  - Always set HOME=/home/coder, GNUPGHOME=/home/coder/.gnupg.
  - Set CARGO_HOME=/home/coder/.cargo.
  - Ensure PATH="$CARGO_HOME/bin:/usr/local/cargo/bin:$PATH".
  - Default RUST_BACKTRACE=1 if unset.

Image selection logic
- Environment overrides:
  - AIFO_RUST_TOOLCHAIN_IMAGE: full image reference override (e.g., internal registry mirror).
  - AIFO_RUST_TOOLCHAIN_VERSION: tag selector (e.g., 1.80, 1.80.1); default "latest".
  - AIFO_RUST_TOOLCHAIN_USE_OFFICIAL=1: force use of official rust image and enable bootstrap.
- Default:
  - Use aifo-coder-toolchain-rust:<version|latest>.
- Fallback:
  - If our toolchain image is unavailable or AIFO_RUST_TOOLCHAIN_USE_OFFICIAL=1 is set, use rust:<version>-slim (or rust:<major>-bookworm) and engage runtime fallback bootstrap.

Fallback bootstrap (official rust images only)
- On first cargo exec within the running sidecar:
  - If cargo-nextest missing: cargo install cargo-nextest --locked.
  - If clippy or rustfmt missing: rustup component add clippy rustfmt.
- Caching under /home/coder/.cargo (respects mounted caches).
- Idempotent: subsequent execs detect presence and do nothing.
- Error handling:
  - Network or install failure: non-zero exit with concise message; verbose mode prints explicit commands and stderr.
  - Exit codes: 1 for bootstrap install errors; 86 remains reserved for shim/protocol.

Operational knobs (environment variables)
- Image:
  - AIFO_RUST_TOOLCHAIN_IMAGE
  - AIFO_RUST_TOOLCHAIN_VERSION
  - AIFO_RUST_TOOLCHAIN_USE_OFFICIAL
- Caches:
  - AIFO_TOOLCHAIN_NO_CACHE
  - AIFO_TOOLCHAIN_RUST_USE_HOST_CARGO (default on)
  - AIFO_TOOLCHAIN_RUST_USE_DOCKER_VOLUMES
  - AIFO_TOOLCHAIN_RUST_USE_HOST_CONFIG
- SSH:
  - AIFO_TOOLCHAIN_SSH_FORWARD, SSH_AUTH_SOCK
- sccache:
  - AIFO_RUST_SCCACHE, AIFO_RUST_SCCACHE_DIR
- Linkers:
  - AIFO_RUST_LINKER=lld|mold
- Proxies:
  - HTTP_PROXY, HTTPS_PROXY, NO_PROXY
- Cargo networking:
  - CARGO_NET_GIT_FETCH_WITH_CLI, CARGO_REGISTRIES_CRATES_IO_PROTOCOL
- Diagnostics:
  - AIFO_TOOLCHAIN_VERBOSE (extra logging during sidecar setup)
  - RUST_BACKTRACE (default to 1)

Security and isolation
- No privileged mode; no Docker socket mounts.
- AppArmor/seccomp/cgroupns behavior unchanged (apply AppArmor profile when available).
- SSH agent forwarding is explicit opt-in; known_hosts remains user-controlled (mount or container config).
- Avoid broad $HOME mounts; mount only $HOME/.cargo subdirs by default.

Networking and proxying
- Sidecars join a session network aifo-net-<sid>.
- On Linux with TCP proxy, add host.docker.internal:host-gateway when AIFO_TOOLEEXEC_ADD_HOST=1.
- Optional unix socket proxy on Linux (AIFO_TOOLEEXEC_USE_UNIX=1) mounts a host socket directory at /run/aifo.

Phased Plan

Phase 0 — Image creation
- Add toolchains/rust/Dockerfile:
  - ARG RUST_TAG (e.g., 1-bookworm or 1.80-bookworm).
  - Preinstall rustup components: clippy, rustfmt, rust-src, llvm-tools-preview.
  - cargo install cargo-nextest --locked.
  - Install system deps: build-essential, pkg-config, cmake, ninja, clang, libclang-dev, python3, libssl-dev, zlib1g-dev, libsqlite3-dev, libcurl4-openssl-dev, git, ca-certificates, curl, tzdata, locales.
  - Set LANG=C.UTF-8; ensure PATH includes /home/coder/.cargo/bin and /usr/local/cargo/bin; CARGO_HOME=/home/coder/.cargo.
- Build multi-arch image (amd64, arm64).

Phase 1 — Makefile integration (build/publish)
- Add targets:
  - build-toolchain-rust: builds aifo-coder-toolchain-rust:latest or :<version>.
  - rebuild-toolchain-rust: same with --no-cache.
  - publish-toolchain-rust: buildx multi-arch and push if REGISTRY is set; otherwise produce an OCI archive in dist/.
- Mirror structure/behavior from existing publish-toolchain-cpp; honor REGISTRY/PLATFORMS/PUSH and REGISTRY_PREFIX normalization.

Phase 2 — Runtime image selection in code
- src/toolchain.rs updates:
  - default_toolchain_image("rust"):
    - If AIFO_RUST_TOOLCHAIN_IMAGE set: use it.
    - Else if AIFO_RUST_TOOLCHAIN_VERSION set: aifo-coder-toolchain-rust:<version>.
    - Else: aifo-coder-toolchain-rust:latest.
  - default_toolchain_image_for_version("rust", v): aifo-coder-toolchain-rust:<v>.
  - Provide graceful fallback to official rust images if our toolchain image is absent or AIFO_RUST_TOOLCHAIN_USE_OFFICIAL=1.

Phase 3 — Mount strategy and env propagation
- In build_sidecar_run_preview(kind="rust"):
  - Set CARGO_HOME=/home/coder/.cargo.
  - Export PATH="$CARGO_HOME/bin:/usr/local/cargo/bin:$PATH".
  - If no_cache: do not mount caches.
  - Else if AIFO_TOOLCHAIN_RUST_USE_DOCKER_VOLUMES=1:
    - Use named volumes aifo-cargo-registry:/home/coder/.cargo/registry and aifo-cargo-git:/home/coder/.cargo/git.
  - Else (default): try host mounts $HOME/.cargo/{registry,git}; fallback per-path to named volumes if host path missing.
  - Optional mounts:
    - Host cargo config (read-only) when AIFO_TOOLCHAIN_RUST_USE_HOST_CONFIG=1; mount to /home/coder/.cargo/config.toml.
    - SSH agent socket when AIFO_TOOLCHAIN_SSH_FORWARD=1 and SSH_AUTH_SOCK is defined.
    - sccache cache with RUSTC_WRAPPER when AIFO_RUST_SCCACHE=1; named volume or host dir as per env.
  - Pass through proxy envs when present.
  - If AIFO_RUST_LINKER=lld|mold: export RUSTFLAGS accordingly.
  - Default RUST_BACKTRACE=1 when unset.
- In build_sidecar_exec_preview(kind="rust"):
  - Export CARGO_HOME and PATH as above.
  - On official rust images (heuristic or env flag), engage bootstrap wrapper prior to executing user args (idempotent).

Phase 4 — Fallback bootstrap on official rust images
- In build_sidecar_exec_preview(kind="rust") or a helper:
  - Wrap the requested command with:
    - cargo nextest -V >/dev/null 2>&1 || cargo install cargo-nextest --locked
    - rustup component list | grep -q '^clippy ' || rustup component add clippy rustfmt
  - Respect proxies; terse logs by default; verbose prints steps; map install failures to non-zero exit; do nothing when already installed.

Phase 5 — PATH, ownership, and networking
- Ensure PATH includes $CARGO_HOME/bin and /usr/local/cargo/bin across run/exec.
- Run as --user uid:gid (host).
- Ensure /home/coder/.cargo and its subdirs are writable by uid:gid; avoid root-owned artifacts when mounting host caches.
- On Linux with TCP proxy: if AIFO_TOOLEEXEC_ADD_HOST=1, add host.docker.internal:host-gateway to sidecars as well.

Phase 6 — Testing and validation
- Unit tests:
  - build_sidecar_run_preview(kind="rust") includes:
    - CARGO_HOME=/home/coder/.cargo
    - PATH containing "$CARGO_HOME/bin:"
    - Cache mounts to /home/coder/.cargo/{registry,git} with correct per-path fallback or forced volumes.
  - build_sidecar_exec_preview(kind="rust") exports CARGO_HOME/PATH; triggers bootstrap wrapper for official images (behavior preview).
  - Image selection honors AIFO_RUST_TOOLCHAIN_IMAGE and AIFO_RUST_TOOLCHAIN_VERSION; falls back as specified.
  - Optional knobs produce expected flags: SSH mount/-e SSH_AUTH_SOCK; sccache mounts and env; proxy env passthrough; RUSTFLAGS for linkers.
- E2E (ignored by default):
  - Start rust sidecar (toolchain_start_session), run via toolexec:
    - cargo -V; cargo nextest -V; rustup component list | grep clippy
  - Exercise both cache modes (host mounts; forced volumes).
  - Validate on amd64 and arm64.
  - With official rust fallback, verify bootstrap installs nextest/clippy once.
- Manual validation inside --toolchain rust:
  - cargo nextest run --no-fail-fast
  - cargo test --no-fail-fast
  - cargo clippy --all-targets --all-features -- -D warnings
  - cargo fmt -- --check

Phase 7 — Documentation
- Update docs/TOOLCHAINS.md:
  - Image selection and env overrides.
  - Cache mount strategy (host cache default; per-path fallback; Windows guidance).
  - SSH agent forwarding and known_hosts considerations.
  - sccache enablement and cache locations.
  - Proxy env pass-through and cargo network tuning (sparse/CLI git fetching).
  - Fast linkers and RUSTFLAGS.
  - Troubleshooting for permissions (create host ~/.cargo/{registry,git} or set AIFO_TOOLCHAIN_RUST_USE_DOCKER_VOLUMES=1).

Phase 8 — Rollout
- Build and publish aifo-coder-toolchain-rust (latest + versioned tags).
- Default code paths to prefer aifo-coder-toolchain-rust and /home/coder/.cargo for CARGO_HOME.
- Retain fallback bootstrap for official images to avoid breaking existing deployments; monitor for regressions.

Error Handling and Exit Codes
- Image not found:
  - Clear error suggesting Makefile targets (build-toolchain-rust) or env override (AIFO_RUST_TOOLCHAIN_IMAGE).
- Permission failures on cargo caches:
  - Prefer host cache mounts by default; instruct to create host ~/.cargo/{registry,git} or set AIFO_TOOLCHAIN_RUST_USE_DOCKER_VOLUMES=1.
- Bootstrap failures:
  - cargo-nextest install or rustup component add failures return non-zero; recommend aifo-coder-toolchain-rust image for stability; verbose for details.
- Network errors during bootstrap:
  - Surface cleanly; recommend aifo-coder-toolchain-rust to avoid on-the-fly installs.
- Shim/protocol:
  - Unchanged (exit 86 reserved for shim-side errors).
- Orchestrator not found:
  - Unchanged.

Security Posture
- No new privileges; same AppArmor/seccomp/cgroupns behavior as existing sidecars.
- SSH forwarding explicit opt-in; avoid mounting broad home directories.
- Supply chain hardening:
  - Use --locked for cargo installs; consider pinning versions for CI variants.
  - Consider SBOM for toolchain images (optional).

Acceptance Criteria
- In agent panes with --toolchain rust:
  - cargo nextest run --no-fail-fast works (cargo-nextest present).
  - cargo test --no-fail-fast works without registry permission errors.
  - cargo clippy --all-targets --all-features -- -D warnings works (clippy installed).
- Default behavior prefers host cache mounts with per-path fallback to named volumes, across amd64 and arm64.
- PATH includes $CARGO_HOME/bin in both run and exec contexts.
- Fallback bootstrap is engaged only when using official rust images; idempotent on subsequent runs.

Risks and Mitigations
- Host cache path absent/unwritable: per-path fallback to named volumes.
- Official rust image use: fallback bootstrap; encourage aifo-coder-toolchain-rust for speed and reproducibility.
- SSH agent/known_hosts complexity: keep opt-in; document clearly.
- Disk usage growth: sccache opt-in and volume-based; document cleanup commands.

Versioning
- This document defines v4 of the Rust toolchain sidecar specification, consolidating v3 + clarifications.
- Future variants:
  - :slim (fewer system deps)
  - :ci (adds cargo-* QA tools, linkers, coverage)
- Subsequent revisions will document backward compatibility and behavioral changes.

Appendix A: Summary of Key Environment Variables
- AIFO_RUST_TOOLCHAIN_IMAGE, AIFO_RUST_TOOLCHAIN_VERSION, AIFO_RUST_TOOLCHAIN_USE_OFFICIAL
- AIFO_TOOLCHAIN_NO_CACHE
- AIFO_TOOLCHAIN_RUST_USE_HOST_CARGO (default on), AIFO_TOOLCHAIN_RUST_USE_DOCKER_VOLUMES
- AIFO_TOOLCHAIN_RUST_USE_HOST_CONFIG
- AIFO_TOOLCHAIN_SSH_FORWARD, SSH_AUTH_SOCK
- AIFO_RUST_SCCACHE, AIFO_RUST_SCCACHE_DIR
- AIFO_RUST_LINKER
- HTTP_PROXY, HTTPS_PROXY, NO_PROXY
- CARGO_NET_GIT_FETCH_WITH_CLI, CARGO_REGISTRIES_CRATES_IO_PROTOCOL
- AIFO_TOOLCHAIN_VERBOSE
- RUST_BACKTRACE

Appendix B: Implementation Mapping (repository integration)
- New files:
  - toolchains/rust/Dockerfile (v4 image as specified)
- Makefile:
  - Add: build-toolchain-rust, rebuild-toolchain-rust, publish-toolchain-rust (mirror c-cpp targets structure; support REGISTRY/PLATFORMS/PUSH)
- src/toolchain.rs:
  - Update image selection (AIFO_RUST_TOOLCHAIN_* envs; fallback to official).
  - Adjust rust mounts to /home/coder/.cargo/{registry,git}; host-preferred with per-path fallback; forced volumes toggle.
  - Export CARGO_HOME=/home/coder/.cargo and PATH="$CARGO_HOME/bin:/usr/local/cargo/bin:$PATH" in run/exec previews.
  - Optional mounts/env: SSH agent, sccache, proxies, host cargo config, RUSTFLAGS for linkers, RUST_BACKTRACE default.
  - Bootstrap wrapper on official images (idempotent first-exec).
- Tests (new or extended under tests/):
  - toolchain_rust_mounts.rs: verify mounts/env for run/exec preview (CARGO_HOME=/home/coder/.cargo).
  - toolchain_rust_image_selection.rs: verify env override and fallback behavior.
  - toolchain_rust_knobs.rs: verify SSH/sccache/proxy/linker toggles.
  - toolchain_rust_bootstrap.rs (ignored): verify bootstrap on official image.
- Documentation:
  - docs/TOOLCHAINS.md updates per Phase 7, including Windows-specific guidance and cache strategies.

Migration Notes (from current codebase)
- Current implementation mounts cargo caches at /usr/local/cargo and sets CARGO_HOME=/usr/local/cargo. v4 migrates to /home/coder/.cargo:
  - Update run/exec previews, tests, and any assertions to /home/coder/.cargo.
  - Keep named volume names (aifo-cargo-registry/git) unchanged for continuity.
  - Prepend PATH with $CARGO_HOME/bin in all rust sidecar contexts to expose cargo-installed tools.
- Introduce image selection envs and bootstrap wrapper as specified; add proxy pass-through and optional knobs incrementally if needed. 
