# AIFO Node Toolchain image (Phase 0)
# - Preinstalls: yarn, pnpm (via corepack), deno
# - Sets cache envs under XDG_CACHE_HOME for precise volume mounting
# - Prepares a writable HOME for arbitrary uid:gid mappings

ARG REGISTRY_PREFIX
FROM ${REGISTRY_PREFIX}node:22-bookworm-slim

ENV DEBIAN_FRONTEND=noninteractive

# Minimal system deps for modern Node workflows (deno installer requires unzip)
RUN set -eux; \
    apt-get update; \
    apt-get install -y --no-install-recommends \
      ca-certificates \
      curl \
      git \
      unzip; \
    rm -rf /var/lib/apt/lists/*

# Home, caches and PATH defaults
ENV HOME=/home/coder \
    XDG_CACHE_HOME=/home/coder/.cache \
    NPM_CONFIG_CACHE=/home/coder/.cache/npm \
    YARN_CACHE_FOLDER=/home/coder/.cache/yarn \
    PNPM_STORE_PATH=/home/coder/.cache/pnpm-store \
    PNPM_HOME=/home/coder/.local/share/pnpm \
    DENO_DIR=/home/coder/.cache/deno \
    PATH="/usr/local/bin:/home/coder/.local/share/pnpm/bin:${PATH}"

# Prepare world-writable directories for arbitrary uid:gid at runtime
RUN set -eux; \
    mkdir -p /home/coder/.cache /home/coder/.local/share/pnpm /home/coder/.npm; \
    chmod -R 0777 /home/coder || true

# Enable corepack and activate yarn/pnpm channels
RUN set -eux; \
    corepack enable; \
    corepack prepare yarn@stable --activate; \
    corepack prepare pnpm@latest --activate

# Install Deno system-wide to /usr/local/bin/deno
RUN set -eux; \
    curl -fsSL https://deno.land/install.sh | sh -s -- -d /usr/local

# Default workdir mirrors sidecar runtime
WORKDIR /workspace

# Sleep by default; proxy will docker exec into this container
CMD ["sleep", "infinity"]
