# Contributing notes: Node toolchain overrides and caches

Node image overrides
- You can override the default Node toolchain image via environment variables:
  - AIFO_NODE_TOOLCHAIN_IMAGE: full image reference (takes precedence).
  - AIFO_NODE_TOOLCHAIN_VERSION: version tag (maps to aifo-node-toolchain:{version}).
- Examples:
  - AIFO_NODE_TOOLCHAIN_IMAGE=aifo-node-toolchain:latest
  - AIFO_NODE_TOOLCHAIN_VERSION=20

Cache layout and mounts
- Node caches are consolidated under XDG_CACHE_HOME=/home/coder/.cache:
  - NPM_CONFIG_CACHE=/home/coder/.cache/npm
  - YARN_CACHE_FOLDER=/home/coder/.cache/yarn
  - PNPM_STORE_PATH=/home/coder/.cache/pnpm-store
  - DENO_DIR=/home/coder/.cache/deno
  - PNPM_HOME=/home/coder/.local/share/pnpm
- When caching is enabled, a single named volume is mounted:
  - aifo-node-cache:/home/coder/.cache
- Purge behavior:
  - toolchain_purge_caches() removes aifo-node-cache and legacy aifo-npm-cache
    for back-compat cleanup, plus other toolchain volumes.

Exec environment
- Docker exec for Node ensures PNPM_HOME and PATH include $PNPM_HOME/bin so pnpm
  managed binaries resolve even for pre-existing containers.

Image build notes
- The toolchains/node/Dockerfile uses ARG REGISTRY_PREFIX for base image selection.
- Build with and without a registry prefix to validate:
  - docker build -f toolchains/node/Dockerfile -t aifo-node-toolchain:20 .
  - docker build -f toolchains/node/Dockerfile -t aifo-node-toolchain:20 \
    --build-arg REGISTRY_PREFIX=repository.migros.net/
