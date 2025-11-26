# AIFO TypeScript Toolchain image (Phase 0)
# - Extends official Node image with a global TypeScript SDK
# - Ensures tsc is on PATH and `require("typescript")` works for deep probes
# - Aligns with Node toolchain conventions for HOME and cache directories

ARG REGISTRY_PREFIX
# CI builds use Kaniko --use-new-run; retain RUN --mount (secrets/cache). Avoid COPY --link/--chmod.
FROM ${REGISTRY_PREFIX}node:22-bookworm-slim

ARG KEEP_APT=0
ENV KEEP_APT=${KEEP_APT}

ENV DEBIAN_FRONTEND=noninteractive

# Minimal system deps for TS workflows
RUN set -eux; \
    apt-get update; \
    apt-get -o APT::Keep-Downloaded-Packages=false install -y --no-install-recommends \
      ca-certificates \
      curl \
      git; \
    apt-mark manual ca-certificates curl git >/dev/null 2>&1 || true; \
    rm -rf /var/lib/apt/lists/* /usr/share/doc/* /usr/share/man/* /usr/share/info/* /usr/share/locale/*

# Home, caches and PATH defaults
ENV HOME=/home/coder \
    XDG_CACHE_HOME=/home/coder/.cache \
    NPM_CONFIG_CACHE=/home/coder/.cache/npm \
    YARN_CACHE_FOLDER=/home/coder/.cache/yarn \
    PNPM_STORE_PATH=/home/coder/.cache/pnpm-store \
    PNPM_HOME=/home/coder/.local/share/pnpm \
    PATH="/usr/local/bin:/home/coder/.local/share/pnpm/bin:${PATH}"

# Prepare world-writable directories for arbitrary uid:gid at runtime
RUN set -eux; \
    mkdir -p /home/coder/.cache /home/coder/.local/share/pnpm /home/coder/.npm; \
    chmod -R 0777 /home/coder || true

# Install TypeScript (and keep image lean)
# hadolint ignore=DL3016
RUN set -eux; \
    npm install -g --omit=dev --no-audit --no-fund --no-update-notifier --no-optional typescript; \
    npm cache clean --force; \
    rm -rf /root/.npm /root/.cache

# Default workdir mirrors sidecar runtime
WORKDIR /workspace

# Optionally drop apt to reduce footprint
RUN if [ "${KEEP_APT:-0}" = "0" ]; then \
    apt-get clean; \
    rm -rf /var/lib/apt/lists/* /usr/share/doc/* /usr/share/man/* /usr/share/info/* /usr/share/locale/*; \
    rm /usr/bin/apt /usr/bin/apt-cache /usr/bin/apt-cdrom /usr/bin/apt-config /usr/bin/apt-get; \
    rm /usr/bin/apt-key /usr/bin/apt-mark || true; \
  fi

# Sleep by default; proxy will docker exec into this container
CMD ["sleep", "infinity"]
