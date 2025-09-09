# AIFO Rust Toolchain (v7)

This document explains how to use the Rust toolchain sidecar in AIFO Coder. It covers image selection, cache strategy and mounts, required environment defaults, optional integrations (SSH, sccache, proxies, fast linkers), Windows-specific behavior, and ownership initialization of named volumes. Migration notes and troubleshooting tips are provided at the end.

Quick start
- Default behavior “just works” on Linux/macOS when launching an agent with the rust toolchain:
  - CARGO_HOME=/home/coder/.cargo
  - PATH=$CARGO_HOME/bin:/usr/local/cargo/bin:$PATH
  - cargo nextest, clippy, rustfmt are available by default when using aifo-rust-toolchain images.
- Caches:
  - Host-preferred cargo caches if $HOME/.cargo/{registry,git} exist.
  - Otherwise fall back to named Docker volumes aifo-cargo-registry and aifo-cargo-git.
- Windows defaults to named volumes for cargo caches.

Image selection
- Prefer the first-party toolchain image by default:
  - aifo-rust-toolchain:<version|latest>
- Environment variables:
  - AIFO_RUST_TOOLCHAIN_IMAGE: Full image override (highest precedence).
  - AIFO_RUST_TOOLCHAIN_VERSION: Preferred tag for aifo-rust-toolchain; defaults to latest when unset.
  - AIFO_RUST_TOOLCHAIN_USE_OFFICIAL=1: Force official rust:<version>-bookworm/slim and enable the bootstrap wrapper.
- Fallback:
  - If aifo-rust-toolchain is unavailable, we fall back to an official rust image and engage the bootstrap to install cargo-nextest and rustup components when needed.

Toolchain image contents (aifo-rust-toolchain)
- Preinstalled rustup components:
  - clippy, rustfmt, rust-src, llvm-tools-preview
- Cargo tools:
  - cargo-nextest (installed with cargo install --locked)
- System packages commonly needed by crates:
  - build-essential, pkg-config, cmake, ninja, clang, libclang-dev
  - python3, libssl-dev, zlib1g-dev, libsqlite3-dev, libcurl4-openssl-dev
  - git, ca-certificates, curl, tzdata, locales
- Environment in image:
  - CARGO_HOME=/home/coder/.cargo
  - PATH=$CARGO_HOME/bin:/usr/local/cargo/bin:$PATH
  - LANG/LC_ALL=C.UTF-8

Runtime environment in sidecar
- Always set in rust sidecar:
  - HOME=/home/coder
  - GNUPGHOME=/home/coder/.gnupg
  - CARGO_HOME=/home/coder/.cargo
  - PATH=$CARGO_HOME/bin:/usr/local/cargo/bin:$PATH
  - Default RUST_BACKTRACE=1 when unset
- Networking:
  - Sidecars join a session network: aifo-net-<sid>.
  - Linux: when AIFO_TOOLEEXEC_ADD_HOST=1, add --add-host host.docker.internal:host-gateway for host connectivity.
  - The agent proxy can optionally use a unix socket transport on Linux when AIFO_TOOLEEXEC_USE_UNIX=1 (mounted at /run/aifo).

Cache strategy and mounts
- Linux/macOS defaults (unless caches disabled):
  - If $HOME/.cargo/registry exists: mount to /home/coder/.cargo/registry; else use named volume aifo-cargo-registry at that path.
  - If $HOME/.cargo/git exists: mount to /home/coder/.cargo/git; else use named volume aifo-cargo-git at that path.
  - For compatibility with older workflows, named volumes are also mounted at legacy paths:
    - /usr/local/cargo/registry and /usr/local/cargo/git
- Windows defaults:
  - Always use named volumes by default:
    - aifo-cargo-registry -> /home/coder/.cargo/registry
    - aifo-cargo-git -> /home/coder/.cargo/git
- Disabling caches:
  - AIFO_TOOLCHAIN_NO_CACHE=1 disables cargo cache mounts entirely.
- Forcing named volumes:
  - AIFO_TOOLCHAIN_RUST_USE_DOCKER_VOLUMES=1 forces aifo-cargo-{registry,git} even if host caches are present.

Ownership initialization
- When named volumes are used for cargo caches, the sidecar performs best-effort “ownership initialization” to ensure writability by the mapped uid:gid:
  - Creates /home/coder/.cargo/{registry,git} if missing.
  - chown -R to uid:gid inside a short-lived helper container.
  - Writes a stamp file to avoid repeat work:
    - /home/coder/.cargo/<subdir>/.aifo-init-done
- This runs once per volume, re-attempting if the stamp is missing or the directory remains unwritable.
- Verbose mode (AIFO_TOOLCHAIN_VERBOSE=1) prints the helper invocation and concise diagnostics.
- Troubleshooting ownership:
  - Remove volumes then retry to reinitialize: docker volume rm -f aifo-cargo-registry aifo-cargo-git
  - Re-run the tool; initialization should recreate and chown the paths.

Optional mounts and environment
- Host cargo config (read-only):
  - AIFO_TOOLCHAIN_RUST_USE_HOST_CONFIG=1 mounts $HOME/.cargo/config(.toml) as /home/coder/.cargo/config.toml:ro when present.
- SSH agent forwarding:
  - AIFO_TOOLCHAIN_SSH_FORWARD=1 and SSH_AUTH_SOCK set:
    - Binds the socket path and exports SSH_AUTH_SOCK inside the container.
- sccache:
  - AIFO_RUST_SCCACHE=1 enables sccache.
  - Mounts:
    - If AIFO_RUST_SCCACHE_DIR is set: -v $AIFO_RUST_SCCACHE_DIR:/home/coder/.cache/sccache
    - Else: -v aifo-sccache:/home/coder/.cache/sccache
  - Exports:
    - RUSTC_WRAPPER=sccache
    - SCCACHE_DIR=/home/coder/.cache/sccache
  - sccache can significantly accelerate rebuilds across sessions.
- Proxies and cargo networking:
  - Pass-through when set on the host:
    - HTTP_PROXY, HTTPS_PROXY, NO_PROXY and lowercase variants
    - CARGO_NET_GIT_FETCH_WITH_CLI
    - CARGO_REGISTRIES_CRATES_IO_PROTOCOL
- Fast linkers:
  - AIFO_RUST_LINKER=lld or mold appends to RUSTFLAGS:
    - lld: -Clinker=clang -Clink-arg=-fuse-ld=lld
    - mold: -Clinker=clang -Clink-arg=-fuse-ld=mold

Bootstrap wrapper (official rust images)
- When using official rust:<tag>-bookworm/slim images, the first exec is wrapped to ensure required developer tools:
  - cargo nextest -V succeeds or cargo install cargo-nextest --locked is run.
  - rustup component add clippy rustfmt when missing.
  - If AIFO_RUST_SCCACHE=1 and sccache is not installed, a concise warning is printed.
- Idempotent: subsequent execs are fast as tools are cached inside the container.
- Verbosity:
  - AIFO_TOOLCHAIN_VERBOSE=1 prints the commands executed by the wrapper.

Windows guidance
- Cargo caches use named volumes by default (host path semantics vary on Windows).
- All environment defaults remain the same:
  - CARGO_HOME=/home/coder/.cargo
  - PATH includes $CARGO_HOME/bin and /usr/local/cargo/bin.
- Optional features (sccache, proxies, linkers) are enabled via the same environment variables.

Environment variables (summary)
- Image selection:
  - AIFO_RUST_TOOLCHAIN_IMAGE
  - AIFO_RUST_TOOLCHAIN_VERSION
  - AIFO_RUST_TOOLCHAIN_USE_OFFICIAL
- Caches:
  - AIFO_TOOLCHAIN_NO_CACHE
  - AIFO_TOOLCHAIN_RUST_USE_DOCKER_VOLUMES
  - AIFO_TOOLCHAIN_RUST_USE_HOST_CONFIG
- SSH:
  - AIFO_TOOLCHAIN_SSH_FORWARD, SSH_AUTH_SOCK
- sccache:
  - AIFO_RUST_SCCACHE, AIFO_RUST_SCCACHE_DIR
- Linkers:
  - AIFO_RUST_LINKER
- Proxies/cargo networking:
  - HTTP_PROXY, HTTPS_PROXY, NO_PROXY (and lowercase)
  - CARGO_NET_GIT_FETCH_WITH_CLI, CARGO_REGISTRIES_CRATES_IO_PROTOCOL
- Diagnostics:
  - AIFO_TOOLCHAIN_VERBOSE, RUST_BACKTRACE

Migration notes
- CARGO_HOME is standardized as /home/coder/.cargo (was /usr/local/cargo in earlier versions).
- PATH must include $CARGO_HOME/bin and retain /usr/local/cargo/bin as fallback.
- Named volumes for registry and git cache remain aifo-cargo-registry and aifo-cargo-git.
- Legacy mounts at /usr/local/cargo/{registry,git} may also be present to ease transition for older tooling.

Troubleshooting
- cargo-nextest not found on official rust images:
  - Use AIFO_RUST_TOOLCHAIN_USE_OFFICIAL=0 (or unset) to prefer aifo-rust-toolchain, or rely on the bootstrap wrapper which installs nextest on first run.
- Permission denied under /home/coder/.cargo/{registry,git}:
  - Remove volumes (docker volume rm -f aifo-cargo-registry aifo-cargo-git) and retry, or run with AIFO_TOOLCHAIN_VERBOSE=1 to see ownership initialization attempts.
- Network access to host from sidecar on Linux:
  - Set AIFO_TOOLEEXEC_ADD_HOST=1 to add host.docker.internal:host-gateway to sidecar runs.
