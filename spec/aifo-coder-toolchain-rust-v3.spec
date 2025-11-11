AIFO Coder: Rust Toolchain Sidecar (v3) – Comprehensive, Production‑Ready Specification

Status
- Stage: v3 specification (implementation-ready; aligns codebase and ops with clarified v2 intent)
- Scope: Rust toolchain image supply, sidecar runtime mounts/env, fallback bootstrap, developer tooling, cross‑platform behavior
- Compatibility: Backward compatible with the existing sidecar model; additive improvements and safe fallbacks
- Platforms: Linux/macOS/Windows (Docker-based; Linux adds optional unix socket transport)

Motivation
Agent-driven Rust workflows (Aider/Crush/Codex, and toolchain run mode) must “just work”:
- cargo nextest run --no-fail-fast succeeds (cargo-nextest available).
- cargo test --no-fail-fast succeeds with writable, persistent cargo registry and git caches.
- cargo clippy --all-targets --all-features -- -D warnings succeeds (clippy installed).
We also want robust, explicit opt-ins for SSH-based git dependencies, sccache, fast linkers, coverage tooling, and corporate proxies.

Guiding Principles
- Zero-surprise defaults: non-root uid:gid works; caches are writable and persistent.
- Prefer host caches when safe; fallback per-path to named Docker volumes.
- Explicit, discoverable knobs for optional mounts and behaviors; testable on amd64/arm64.
- Clear errors, minimal noise; verbose mode provides actionable diagnostics.
- Security posture unchanged: no privileged containers; no Docker socket mounts.

Key v3 Clarifications vs v2
- CARGO_HOME inside sidecar is /home/coder/.cargo (not /usr/local/cargo).
  - Avoids permission issues with non-root uid:gid.
  - PATH MUST prepend $CARGO_HOME/bin and SHOULD retain /usr/local/cargo/bin as secondary.
- Host caches preferred by default, per path:
  - If $HOME/.cargo/registry exists: mount to /home/coder/.cargo/registry; else use aifo-cargo-registry volume.
  - If $HOME/.cargo/git exists: mount to /home/coder/.cargo/git; else use aifo-cargo-git volume.
  - On Windows hosts, named volumes are recommended by default; host cache mounts may be enabled when path semantics are safe.
- Image selection defaults to aifo-coder-toolchain-rust:<version|latest>; environment overrides supported; graceful fallback to official rust images with bootstrap.
- Optional mounts/env supported explicitly: host cargo config (read-only), SSH agent, sccache, proxy envs, fast linkers (lld/mold via RUSTFLAGS).
- Fallback bootstrap precisely defined and idempotent for official rust images (cargo-nextest + clippy/rustfmt).
- RUST_BACKTRACE defaults to 1 (best-effort) for better diagnostics.

Goals
- First-party Rust toolchain image:
  - rustfmt, clippy, rust-src, llvm-tools-preview via rustup.
  - cargo-nextest via cargo install --locked.
  - System dependencies for common crates: gcc/g++/make, pkg-config, cmake, ninja, clang/libclang-dev, python3, libssl-dev, zlib1g-dev, libsqlite3-dev, libcurl4-openssl-dev, git, ca-certificates, curl, tzdata, locales.
- Writable, persistent cargo caches by default; host-cache preference with per-path fallback to named volumes.
- Security posture maintained; no privileged mode; no Docker socket mounts.
- Robust opt-ins: SSH agent forwarding, sccache, fast linkers, coverage, proxies.
- Cross-platform correctness (Linux/macOS/Windows; amd64/arm64).

Non-Goals
- Changing shim protocol, proxy model, or agent orchestration mechanics.
- Managing project-specific rustup toolchains (rust-toolchain.toml behavior is up to users).
- Shipping exhaustive cargo-* commands by default beyond a curated set.

High-Level Design
- Toolchain image: aifo-coder-toolchain-rust:<tag>, based on official rust:<tag> images.
  - Preinstalled rustup components: clippy, rustfmt, rust-src, llvm-tools-preview.
  - Preinstalled cargo tools: cargo-nextest (pinned via --locked).
  - PATH includes /home/coder/.cargo/bin and /usr/local/cargo/bin; CARGO_HOME=/home/coder/.cargo.
- Caches:
  - Default: mount host $HOME/.cargo/{registry,git} into /home/coder/.cargo/{registry,git}, per-path fallback to named volumes aifo-cargo-registry and aifo-cargo-git.
  - Forced volumes (opt-in): AIFO_TOOLCHAIN_RUST_USE_DOCKER_VOLUMES=1.
- Environment defaults:
  - HOME=/home/coder; GNUPGHOME=/home/coder/.gnupg; CARGO_HOME=/home/coder/.cargo.
  - PATH="$CARGO_HOME/bin:/usr/local/cargo/bin:$PATH".
  - RUST_BACKTRACE=1 by default when unset.
- Optional mounts/env:
  - Host cargo config (read-only), SSH agent socket, sccache, proxy envs, fast linkers (lld/mold via RUSTFLAGS).
- Fallback bootstrap (official images):
  - On first exec, install cargo-nextest (cargo install --locked) and rustup components clippy/rustfmt if missing; idempotent; terse by default, verbose logs on request.

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
- Optional variants (:ci flavor, future):
  - cargo-llvm-cov (requires llvm-tools), cargo-edit, cargo-outdated, cargo-udeps, cargo-deny, cargo-audit, cargo-expand
  - sccache (optional; if not included, runtime opt-in works with host-provided sccache)

System packages (to satisfy common crate requirements)
- build-essential (gcc, g++, make), pkg-config
- cmake, ninja
- clang, libclang-dev (bindgen)
- python3 (for build.rs scripts)
- libssl-dev, zlib1g-dev, libsqlite3-dev, libcurl4-openssl-dev
- git, ca-certificates, curl, tzdata, locales (UTF-8)

Environment defaults inside the image
- LANG=C.UTF-8
- CARGO_HOME=/home/coder/.cargo
- PATH includes /home/coder/.cargo/bin (prepend) and /usr/local/cargo/bin (fallback path)

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
  - Forced volumes (opt-in): AIFO_TOOLCHAIN_RUST_USE_DOCKER_VOLUMES=1 forces named volumes even if host caches exist.
  - Windows hosts: prefer named volumes by default; enable host cache mounts only when path semantics are safe within your environment.
- Optional mounts/env:
  - Host cargo config (read-only): If AIFO_TOOLCHAIN_RUST_USE_HOST_CONFIG=1 and $HOME/.cargo/config(.toml) exists, mount as /home/coder/.cargo/config.toml.
  - SSH agent forward: If AIFO_TOOLCHAIN_SSH_FORWARD=1 and SSH_AUTH_SOCK is set, mount the socket (bind) and export SSH_AUTH_SOCK.
  - sccache (opt-in): If AIFO_RUST_SCCACHE=1,
    - If $AIFO_RUST_SCCACHE_DIR set: -v $AIFO_RUST_SCCACHE_DIR:/home/coder/.cache/sccache
    - Else: -v aifo-sccache:/home/coder/.cache/sccache
    - Export RUSTC_WRAPPER=sccache and SCCACHE_DIR=/home/coder/.cache/sccache
  - Proxies: If HTTP_PROXY/HTTPS_PROXY/NO_PROXY defined on host, pass through into sidecar.
  - Fast linkers: If AIFO_RUST_LINKER=lld|mold, export RUSTFLAGS appropriately (e.g., -Clinker=clang -Clink-arg=-fuse-ld=lld). Ensure image contains the requested linker in CI variants.
- Other env:
  - Always set HOME=/home/coder, GNUPGHOME=/home/coder/.gnupg
  - Set CARGO_HOME=/home/coder/.cargo
  - Ensure PATH="$CARGO_HOME/bin:/usr/local/cargo/bin:$PATH"
  - Default RUST_BACKTRACE=1 if unset for better diagnostics

Environment variables passed into the sidecar (selected)
- Always:
  - HOME=/home/coder
  - GNUPGHOME=/home/coder/.gnupg
  - CARGO_HOME=/home/coder/.cargo
  - PATH includes $CARGO_HOME/bin
- Optional pass-through (if set on host/agent):
  - HTTP_PROXY, HTTPS_PROXY, NO_PROXY
  - RUST_BACKTRACE (default to 1 if not set)
  - CARGO_NET_GIT_FETCH_WITH_CLI, CARGO_REGISTRIES_CRATES_IO_PROTOCOL
  - SSH_AUTH_SOCK (if AIFO_TOOLCHAIN_SSH_FORWARD=1)
  - sccache envs (if AIFO_RUST_SCCACHE=1)
  - AIFO_SESSION_NETWORK (sidecars join the aifo-net-<sid> network)
  - AIFO_RUST_LINKER (lld|mold)

Image selection logic
- Environment overrides:
  - AIFO_RUST_TOOLCHAIN_IMAGE: full image reference override (e.g., your registry mirror).
  - AIFO_RUST_TOOLCHAIN_VERSION: tag selector (e.g., 1.80, 1.80.1); default "latest".
- Default:
  - Use aifo-coder-toolchain-rust:<version|latest>.
- Fallback:
  - If our toolchain image is unavailable or AIFO_RUST_TOOLCHAIN_USE_OFFICIAL=1, use rust:<version>-slim (or rust:<major>-bookworm) and engage runtime fallback bootstrap.

Fallback bootstrap (when using official rust images)
- On the first cargo exec within the running sidecar:
  - If cargo-nextest missing: cargo install cargo-nextest --locked.
  - If clippy or rustfmt missing: rustup component add clippy rustfmt.
- Caching under /home/coder/.cargo (respects mounted caches).
- Idempotent: does nothing when tools are present.
- Error handling:
  - Network or install failure: non-zero exit with concise message; verbose mode prints full steps.
  - Recommend switching to aifo-coder-toolchain-rust to avoid on-the-fly installs.

Operational knobs (environment variables)
- AIFO_RUST_TOOLCHAIN_IMAGE
- AIFO_RUST_TOOLCHAIN_VERSION
- AIFO_RUST_TOOLCHAIN_USE_OFFICIAL=1
- AIFO_TOOLCHAIN_NO_CACHE=1
- AIFO_TOOLCHAIN_RUST_USE_HOST_CARGO=1 (default: on)
- AIFO_TOOLCHAIN_RUST_USE_DOCKER_VOLUMES=1
- AIFO_TOOLCHAIN_RUST_USE_HOST_CONFIG=1
- AIFO_TOOLCHAIN_SSH_FORWARD=1, SSH_AUTH_SOCK
- AIFO_RUST_SCCACHE=1, AIFO_RUST_SCCACHE_DIR
- AIFO_RUST_LINKER=lld|mold
- HTTP_PROXY, HTTPS_PROXY, NO_PROXY
- CARGO_NET_GIT_FETCH_WITH_CLI, CARGO_REGISTRIES_CRATES_IO_PROTOCOL
- AIFO_TOOLCHAIN_VERBOSE=1 (extra logging during sidecar setup)
- RUST_BACKTRACE (default to 1 if unset)

Security and isolation
- No privileged mode; no Docker socket mounts.
- AppArmor/seccomp/cgroupns behavior unchanged (desired AppArmor profile applied when available).
- SSH agent forwarding is explicit opt-in; known_hosts configuration is user-controlled (mount or container-side configuration).
- Avoid broad $HOME mounts; only mount $HOME/.cargo subdirs by default.

Phased Plan

Phase 0 — Image creation
- Add toolchains/rust/Dockerfile:
  - ARG RUST_TAG (e.g., 1-bookworm or 1.80-bookworm).
  - Install rustup components: clippy, rustfmt, rust-src, llvm-tools-preview.
  - cargo install cargo-nextest --locked.
  - Install system packages: build-essential, pkg-config, cmake, ninja, clang, libclang-dev, python3, libssl-dev, zlib1g-dev, libsqlite3-dev, libcurl4-openssl-dev, git, ca-certificates, curl, tzdata, locales.
  - Set LANG=C.UTF-8; ensure PATH includes /home/coder/.cargo/bin and /usr/local/cargo/bin.
- Build multi-arch image (amd64, arm64).

Phase 1 — Makefile integration (build/publish)
- Add targets:
  - build-toolchain-rust: builds aifo-coder-toolchain-rust:latest or :<version> when configured.
  - rebuild-toolchain-rust: same with --no-cache.
  - publish-toolchain-rust: buildx multi-arch and push if REGISTRY is set; otherwise produce an OCI archive in dist/.
- Mirror patterns and behavior from existing publish-toolchain-cpp targets; honor REGISTRY_PREFIX probing and tagging behavior.

Phase 2 — Runtime image selection in code
- In src/toolchain.rs:
  - default_toolchain_image("rust"):
    - If AIFO_RUST_TOOLCHAIN_IMAGE set: use it.
    - Else if AIFO_RUST_TOOLCHAIN_VERSION set: aifo-coder-toolchain-rust:<version>.
    - Else: aifo-coder-toolchain-rust:latest.
  - default_toolchain_image_for_version("rust", v):
    - aifo-coder-toolchain-rust:<v>.
  - Provide graceful fallback to official rust images if our toolchain image is absent or AIFO_RUST_TOOLCHAIN_USE_OFFICIAL=1.

Phase 3 — Mount strategy (writable caches) and env propagation
- In build_sidecar_run_preview(kind="rust"):
  - Set CARGO_HOME=/home/coder/.cargo.
  - Export PATH="$CARGO_HOME/bin:/usr/local/cargo/bin:$PATH".
  - If no_cache: do not mount caches.
  - Else if AIFO_TOOLCHAIN_RUST_USE_DOCKER_VOLUMES=1:
    - Use named volumes aifo-cargo-registry:/home/coder/.cargo/registry and aifo-cargo-git:/home/coder/.cargo/git.
  - Else (default): try host mounts $HOME/.cargo/{registry,git}; fallback per-path to named volumes if host path missing.
  - Optional mounts:
    - Host cargo config (read-only) when AIFO_TOOLCHAIN_RUST_USE_HOST_CONFIG=1; mount to /home/coder/.cargo/config.toml.
    - SSH agent socket when AIFO_TOOLCHAIN_SSH_FORWARD=1 and SSH_AUTH_SOCK is defined (bind the socket path).
    - sccache cache with RUSTC_WRAPPER when AIFO_RUST_SCCACHE=1; named volume or host dir as per env.
  - Pass through proxy envs when present.
  - If AIFO_RUST_LINKER=lld|mold: export RUSTFLAGS accordingly.
  - Default RUST_BACKTRACE=1 when unset.
- In build_sidecar_exec_preview(kind="rust"):
  - Export CARGO_HOME and PATH as above.
  - On official rust images (heuristic or env flag), engage bootstrap wrapper prior to executing user args (idempotent).

Phase 4 — Fallback bootstrap on official rust images
- In build_sidecar_exec_preview(kind="rust") or a dedicated helper:
  - Wrap the requested command with:
    - cargo nextest -V >/dev/null 2>&1 || cargo install cargo-nextest --locked
    - rustup component list | grep -q '^clippy ' || rustup component add clippy rustfmt
  - Use terse logs; verbose prints steps; map failures to non-zero exit; do nothing when already installed.

Phase 5 — PATH and ownership
- Ensure PATH includes $CARGO_HOME/bin (and /usr/local/cargo/bin) across run and exec.
- Continue running as --user uid:gid (host).
- Ensure /home/coder/.cargo is owned/writable by uid:gid; avoid root-owned artifacts.

Phase 6 — Testing and validation
- Unit tests:
  - build_sidecar_run_preview(kind="rust") includes:
    - CARGO_HOME=/home/coder/.cargo
    - PATH containing "$CARGO_HOME/bin:"
    - Cache mounts to /home/coder/.cargo/{registry,git} with correct named volumes fallback
  - build_sidecar_exec_preview(kind="rust") exports CARGO_HOME/PATH; triggers bootstrap wrapper preview on official images.
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
  - SSH agent forwarding and known_hosts.
  - sccache enablement and cache locations.
  - Proxy env pass-through and sparse/CLI git fetching.
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
  - cargo-nextest install or rustup component add failures return non-zero; suggest using aifo-coder-toolchain-rust for stability; verbose for details.
- Network errors during bootstrap: surface cleanly; recommend using aifo-coder-toolchain-rust to avoid on-the-fly installs.
- Shim/protocol: unchanged (exit 86 reserved for shim-side errors).
- Orchestrator not found: unchanged.

Security Posture
- No new privileges; same AppArmor/seccomp/cgroupns behavior as existing sidecars.
- SSH forwarding is explicit opt-in; avoid mounting broad home directories.
- Supply chain hardening:
  - Use --locked for cargo installs; consider pinning versions for CI variants.
  - Consider SBOM generation for toolchain image variants.

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
- This document defines v3 of the Rust toolchain sidecar specification.
- Future variants:
  - :slim (fewer system deps)
  - :ci (adds cargo-* QA tools, linkers, coverage)
- Subsequent revisions should document backward compatibility and behavioral changes.

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
  - toolchains/rust/Dockerfile (v3 image as specified)
- Makefile:
  - Add: build-toolchain-rust, rebuild-toolchain-rust, publish-toolchain-rust (mirror c-cpp targets structure; support REGISTRY/PLATFORMS/PUSH)
- src/toolchain.rs:
  - Update image selection (AIFO_RUST_TOOLCHAIN_* envs; fallback to official).
  - Adjust rust mounts to /home/coder/.cargo/{registry,git}; host-preferred with per-path fallback; forced volumes toggle.
  - Export CARGO_HOME=/home/coder/.cargo and PATH="$CARGO_HOME/bin:/usr/local/cargo/bin:$PATH" in run/exec previews.
  - Optional mounts/env: SSH agent, sccache, proxies, host cargo config, RUSTFLAGS for linkers, RUST_BACKTRACE default.
  - Bootstrap wrapper on official images (idempotent first-exec).
- Tests (new or extended under tests/):
  - toolchain_rust_mounts.rs: verify mounts/env for run/exec preview.
  - toolchain_rust_image_selection.rs: verify env override and fallback behavior.
  - toolchain_rust_knobs.rs: verify SSH/sccache/proxy/linker toggles.
  - toolchain_rust_bootstrap.rs (ignored): verify bootstrap on official image.
- Documentation:
  - docs/TOOLCHAINS.md updates per Phase 7.
