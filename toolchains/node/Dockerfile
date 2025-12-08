# AIFO Node Toolchain image (Phase 0)
# - Preinstalls: yarn, pnpm (via corepack), deno
# - Sets cache envs under XDG_CACHE_HOME for precise volume mounting
# - Prepares a writable HOME for arbitrary uid:gid mappings

ARG REGISTRY_PREFIX
# CI builds use Kaniko --use-new-run; retain RUN --mount (secrets/cache). Avoid COPY --link/--chmod.
FROM ${REGISTRY_PREFIX}node:22-bookworm-slim

ARG KEEP_APT=0
ENV KEEP_APT=${KEEP_APT}

ENV DEBIAN_FRONTEND=noninteractive

# Minimal system deps for modern Node workflows (deno installer requires unzip)
RUN set -eux; \
    apt-get update; \
    apt-get -o APT::Keep-Downloaded-Packages=false install -y --no-install-recommends \
      ca-certificates \
      curl \
      git \
      unzip; \
    apt-mark manual ca-certificates curl git unzip >/dev/null 2>&1 || true; \
    rm -rf /var/lib/apt/lists/* /usr/share/doc/* /usr/share/man/* /usr/share/info/* /usr/share/locale/*

# Home, caches and PATH defaults
ENV HOME=/home/coder \
    XDG_CACHE_HOME=/home/coder/.cache \
    NPM_CONFIG_CACHE=/home/coder/.cache/npm \
    YARN_CACHE_FOLDER=/home/coder/.cache/yarn \
    PNPM_STORE_PATH=/home/coder/.cache/pnpm-store \
    PNPM_HOME=/home/coder/.local/share/pnpm \
    DENO_DIR=/home/coder/.cache/deno \
    BUN_CACHE_DIR=/home/coder/.cache/bun \
    PATH="/usr/local/bin:/home/coder/.local/share/pnpm/bin:${PATH}"

# Prepare world-writable directories for arbitrary uid:gid at runtime
RUN set -eux; \
    mkdir -p /home/coder/.cache /home/coder/.local/share/pnpm /home/coder/.npm; \
    chmod -R 0777 /home/coder || true

# Enable corepack (yarn/pnpm) and install deno in a single RUN (with optional enterprise CA injection)
# Guardrail: This uses BuildKit RUN --mount=type=secret for enterprise CA; CI relies on Kaniko --use-new-run.
# Best-effort CA handling: inject inside RUN and remove in the same RUN to avoid persisting CA in layers.
RUN --mount=type=secret,id=migros_root_ca,target=/run/secrets/migros_root_ca,required=false sh -lc 'set -eu; \
    CAF=/run/secrets/migros_root_ca; \
    if [ -f "$CAF" ]; then \
      install -m 0644 "$CAF" /usr/local/share/ca-certificates/migros-root-ca.crt || true; \
      export NODE_EXTRA_CA_CERTS="$CAF"; \
      export NODE_OPTIONS="${NODE_OPTIONS:+$NODE_OPTIONS }--use-openssl-ca"; \
      export SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt; \
      export CURL_CA_BUNDLE=/etc/ssl/certs/ca-certificates.crt; \
    fi; \
    corepack enable; \
    corepack prepare yarn@stable --activate; \
    corepack prepare pnpm@latest --activate; \
    # Deno install (direct binary preferred; fallback to installer) \
    INSTALLED=0; \
    ver="$(curl -fsSL https://dl.deno.land/release-latest.txt | tr -d '\''\r'\'' | tr -d '\''\n'\'' || true)"; \
    if [ -n "$ver" ]; then \
      arch="$(dpkg --print-architecture 2>/dev/null || uname -m)"; \
      case "$arch" in aarch64|arm64) triple="aarch64-unknown-linux-gnu" ;; x86_64|amd64) triple="x86_64-unknown-linux-gnu" ;; *) triple="";; esac; \
      if [ -n "$triple" ]; then \
        url="https://dl.deno.land/release/${ver}/deno-${triple}.zip"; \
        echo "Attempting direct download: $url" >&2; \
        mkdir -p /tmp/deno && cd /tmp/deno; \
        if curl -fsSL "$url" -o deno.zip; then \
          unzip -q deno.zip; \
          install -m 0755 deno /usr/local/bin/deno; \
          cd /; rm -rf /tmp/deno; \
          INSTALLED=1; \
        fi; \
      fi; \
    fi; \
    if [ "$INSTALLED" -ne 1 ]; then \
      tmp=/tmp/deno_install.sh; \
      if ! curl -fsSL https://deno.land/install.sh -o "$tmp"; then \
        echo "warning: https://deno.land/install.sh failed; trying x/install ..." >&2; \
        if ! curl -fsSL https://deno.land/x/install/install.sh -o "$tmp"; then \
          echo "error: failed to obtain deno installer script" >&2; \
          exit 1; \
        fi; \
      fi; \
      sh "$tmp" -d /usr/local; \
      rm -f "$tmp"; \
    fi; \
    # Best-effort strip to reduce size \
    strip /usr/local/bin/deno 2>/dev/null || true; \
    # Install TypeScript globally and clean npm caches \
    npm install -g --omit=dev --no-audit --no-fund --no-update-notifier --no-optional typescript; \
    npm cache clean --force || true; \
    rm -rf /root/.npm /root/.cache || true; \
    # Install Bun runtime (direct binary) \
    arch="$(dpkg --print-architecture 2>/dev/null || uname -m)"; \
    case "$arch" in aarch64|arm64) bun_zip="bun-linux-aarch64.zip" ;; x86_64|amd64) bun_zip="bun-linux-x64.zip" ;; *) bun_zip="" ;; esac; \
    if [ -n "$bun_zip" ]; then \
      mkdir -p /tmp/bun && cd /tmp/bun; \
      if curl -fsSL "https://github.com/oven-sh/bun/releases/latest/download/${bun_zip}" -o bun.zip; then \
        unzip -q bun.zip; \
        if [ -f bun-linux-aarch64/bun ]; then install -m 0755 bun-linux-aarch64/bun /usr/local/bin/bun; elif [ -f bun-linux-x64/bun ]; then install -m 0755 bun-linux-x64/bun /usr/local/bin/bun; elif [ -f bun ]; then install -m 0755 bun /usr/local/bin/bun; fi; \
        cd /; rm -rf /tmp/bun; \
        strip /usr/local/bin/bun 2>/dev/null || true; \
      fi; \
    fi; \
    # Remove injected enterprise CA (best-effort) \
    if [ -f /usr/local/share/ca-certificates/migros-root-ca.crt ]; then \
      rm -f /usr/local/share/ca-certificates/migros-root-ca.crt; \
      command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; \
    fi'

# Consolidated into the previous RUN (corepack + yarn/pnpm + deno + CA cleanup)

# Default workdir mirrors sidecar runtime
WORKDIR /workspace

# Optionally drop procps from final image to reduce footprint
RUN if [ "${KEEP_APT:-0}" = "0" ]; then \
    apt-get clean; \
    rm -rf /var/lib/apt/lists/* /usr/share/doc/* /usr/share/man/* /usr/share/info/* /usr/share/locale/*; \
    rm /usr/bin/apt /usr/bin/apt-cache /usr/bin/apt-cdrom /usr/bin/apt-config /usr/bin/apt-get; \
    rm /usr/bin/apt-key /usr/bin/apt-mark; \
  fi
# Verify critical tools still present after optional cleanup (fail fast if missing)
RUN command -v curl >/dev/null && git --version >/dev/null

# Sleep by default; proxy will docker exec into this container
CMD ["sleep", "infinity"]
