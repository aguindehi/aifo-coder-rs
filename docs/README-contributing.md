# Contributing notes: Toolchain overrides and caches

This document summarizes how to override toolchain images, how caches are laid out and
mounted, and a few execution environment details for each supported toolchain.

Conventions and registries (IR vs MR)
- Toolchain overrides:
  - AIFO_<KIND>_TOOLCHAIN_IMAGE: full image reference (highest precedence)
  - AIFO_<KIND>_TOOLCHAIN_VERSION: version tag (maps to aifo-coder-toolchain-<kind>:{version})
- Internal registry (IR) prefix at runtime:
  - AIFO_CODER_INTERNAL_REGISTRY_PREFIX: normalized to include a single trailing "/" and
    prepended to our aifo-coder-* images. Empty/unset yields no prefix.
- Mirror registry (MR) prefix:
  - Build-time: Docker ARG REGISTRY_PREFIX remains for base pulls in CI/Makefile.
  - Runtime: AIFO_CODER_MIRROR_REGISTRY_PREFIX (normalized, trailing "/") prefixes unqualified
    third‑party images when IR is unset. Internal namespace is not applied to MR.
  - Internal namespace: AIFO_CODER_INTERNAL_REGISTRY_NAMESPACE controls the path segment used with IR
    (default: ai-foundation/prototypes/aifo-coder-rs).
- Agent image overrides (coding agents):
  - AIFO_CODER_AGENT_IMAGE: full image reference used verbatim (host/path:tag or @digest).
  - AIFO_CODER_AGENT_TAG: retags the default agent image (e.g., release-0.6.3).
  - Default tag: release-<version> (matches launcher version). Override with AIFO_CODER_IMAGE_TAG or AIFO_CODER_AGENT_TAG.
  - Automatic login: on permission-denied pulls, the launcher will prompt for `docker login` to
    the resolved registry and retry once (interactive only). Disable with AIFO_CODER_AUTO_LOGIN=0.

Toolchain image overrides

Rust
- Environment:
  - AIFO_RUST_TOOLCHAIN_IMAGE
  - AIFO_RUST_TOOLCHAIN_VERSION
  - AIFO_RUST_TOOLCHAIN_USE_OFFICIAL=1 forces official rust:<ver> images where supported.
- Notes:
  - Official images are used when requested or when policy dictates.
  - Defaults prefer our sidecar unless overridden.

Node (includes TypeScript)
- Environment:
  - AIFO_NODE_TOOLCHAIN_IMAGE
  - AIFO_NODE_TOOLCHAIN_VERSION
- Examples:
  - AIFO_NODE_TOOLCHAIN_IMAGE=aifo-coder-toolchain-node:latest
  - AIFO_NODE_TOOLCHAIN_VERSION=22

Python
- Environment:
  - AIFO_PYTHON_TOOLCHAIN_IMAGE
  - AIFO_PYTHON_TOOLCHAIN_VERSION

Go
- Environment:
  - AIFO_GO_TOOLCHAIN_IMAGE
  - AIFO_GO_TOOLCHAIN_VERSION

C/C++ (CCpp)
- Environment:
  - AIFO_CCPP_TOOLCHAIN_IMAGE
  - AIFO_CCPP_TOOLCHAIN_VERSION

Cache layout and mounts (common scheme)
- Unless noted, caches are consolidated under XDG_CACHE_HOME=/home/coder/.cache and a single
  named volume per toolchain is mounted at /home/coder/.cache.
- Purge behavior:
  - toolchain_purge_caches() removes per-toolchain cache volumes (and some legacy names) for a
    clean slate. Known volumes include aifo-node-cache and aifo-npm-cache; others follow the
    aifo-<kind>-cache naming scheme.

Per-toolchain cache details

Node
- XDG_CACHE_HOME=/home/coder/.cache
- NPM_CONFIG_CACHE=/home/coder/.cache/npm
- YARN_CACHE_FOLDER=/home/coder/.cache/yarn
- PNPM_STORE_PATH=/home/coder/.cache/pnpm-store
- DENO_DIR=/home/coder/.cache/deno
- PNPM_HOME=/home/coder/.local/share/pnpm
- Volume: aifo-node-cache:/home/coder/.cache
- Legacy volume (purged for back-compat): aifo-npm-cache

Rust
- CARGO_HOME=/home/coder/.cargo
- RUSTUP_HOME=/home/coder/.rustup
- SCCACHE_DIR=/home/coder/.cache/sccache (if sccache is enabled)
- XDG_CACHE_HOME=/home/coder/.cache
- Typical volume: aifo-rust-cache:/home/coder/.cache
- Named volumes under ~/.cargo:
  - aifo-cargo-registry:/home/coder/.cargo/registry
  - aifo-cargo-git:/home/coder/.cargo/git
- Ownership init:
  - When these mounts are selected, a short helper container chowns them to uid:gid and stamps
    .aifo-init-done to avoid repeated work (see init_rust_named_volumes_if_needed).

Python
- PIP_CACHE_DIR=/home/coder/.cache/pip
- XDG_CACHE_HOME=/home/coder/.cache
- Typical volume: aifo-python-cache:/home/coder/.cache

Go
- GOCACHE=/home/coder/.cache/go-build
- GOMODCACHE defaults under GOPATH; if normalized, it may be placed under XDG cache as well
- XDG_CACHE_HOME=/home/coder/.cache
- Typical volume: aifo-go-cache:/home/coder/.cache

C/C++
- If compiler cache is enabled:
  - ccache: /home/coder/.cache/ccache
  - sccache: /home/coder/.cache/sccache
- XDG_CACHE_HOME=/home/coder/.cache
- Typical volume: aifo-ccpp-cache:/home/coder/.cache

Exec environment notes

Node
- PNPM_HOME is ensured and PATH includes $PNPM_HOME/bin so pnpm-managed binaries resolve,
  even in pre-existing containers.

Rust
- PATH includes $CARGO_HOME/bin so cargo-installed tools are available.
- sccache policy is validated and warnings are surfaced in preview where applicable.

Python
- When a virtualenv is active in the workspace, PATH is adjusted so venv/bin takes precedence.

Go
- GOPATH and GOBIN are configured so go-installed tools are resolvable on PATH.

C/C++
- If compiler cache is used, environment is set for ccache/sccache to be effective.

Image build notes
- Each toolchain’s Dockerfile accepts ARG REGISTRY_PREFIX for base image selection.
- Build Node (with/without registry prefix) for validation:
  - docker build -f toolchains/node/Dockerfile -t aifo-coder-toolchain-node:22 .
  - docker build -f toolchains/node/Dockerfile -t aifo-coder-toolchain-node:22 \
    --build-arg REGISTRY_PREFIX=repository.migros.net/
- Similar patterns apply for other toolchains under toolchains/<kind>/Dockerfile.

References
- Exact environment variable names, precedence, and defaults are defined in src/toolchain/images.rs.
- Mounts and cache volume names are defined under src/toolchain/mounts.rs and related modules.
