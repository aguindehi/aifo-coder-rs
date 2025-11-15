# AIFO Node Toolchain image (Phase 0)
# - Preinstalls: yarn, pnpm (via corepack), deno
# - Sets cache envs under XDG_CACHE_HOME for precise volume mounting
# - Prepares a writable HOME for arbitrary uid:gid mappings

ARG REGISTRY_PREFIX
FROM ${REGISTRY_PREFIX}node:22-bookworm-slim

ARG KEEP_APT

ENV DEBIAN_FRONTEND=noninteractive

# Minimal system deps for modern Node workflows (deno installer requires unzip)
# hadolint ignore=DL3008
RUN set -eux; \
    apt-get update; \
    apt-get install -y --no-install-recommends \
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
    PATH="/usr/local/bin:/home/coder/.local/share/pnpm/bin:${PATH}"

# Prepare world-writable directories for arbitrary uid:gid at runtime
RUN set -eux; \
    mkdir -p /home/coder/.cache /home/coder/.local/share/pnpm /home/coder/.npm; \
    chmod -R 0777 /home/coder || true

# Enable corepack and activate yarn/pnpm channels (with optional enterprise CA injection)
RUN --mount=type=secret,id=migros_root_ca,target=/run/secrets/migros_root_ca,required=false sh -lc 'set -e; \
    CAF=/run/secrets/migros_root_ca; \
    if [ -f "$CAF" ]; then \
      install -m 0644 "$CAF" /usr/local/share/ca-certificates/migros-root-ca.crt || true; \
      command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; \
      export NODE_EXTRA_CA_CERTS="$CAF"; \
      export NODE_OPTIONS="${NODE_OPTIONS:+$NODE_OPTIONS }--use-openssl-ca"; \
      export SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt; \
      export CURL_CA_BUNDLE=/etc/ssl/certs/ca-certificates.crt; \
    fi; \
    corepack enable; \
    corepack prepare yarn@stable --activate; \
    corepack prepare pnpm@latest --activate; \
    if [ -f /usr/local/share/ca-certificates/migros-root-ca.crt ]; then \
      rm -f /usr/local/share/ca-certificates/migros-root-ca.crt; \
      command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; \
    fi'

# Install Deno system-wide to /usr/local/bin/deno (with optional enterprise CA injection)
# hadolint ignore=DL3059
RUN --mount=type=secret,id=migros_root_ca,target=/run/secrets/migros_root_ca,required=false sh -lc 'set -eu; \
    CAF=/run/secrets/migros_root_ca; \
    if [ -f "$CAF" ]; then \
      install -m 0644 "$CAF" /usr/local/share/ca-certificates/migros-root-ca.crt || true; \
      command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; \
      export SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt; \
      export CURL_CA_BUNDLE=/etc/ssl/certs/ca-certificates.crt; \
    fi; \
    INSTALLED=0; \
    # Prefer direct binary download from dl.deno.land to avoid installer network issues \
    ver="$(curl -fsSL https://dl.deno.land/release-latest.txt | tr -d '\r' | tr -d '\n' || true)"; \
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
    if [ -f /usr/local/share/ca-certificates/migros-root-ca.crt ]; then \
      rm -f /usr/local/share/ca-certificates/migros-root-ca.crt; \
      command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; \
    fi'

# Default workdir mirrors sidecar runtime
WORKDIR /workspace

# Optionally drop procps from final image to reduce footprint
RUN if [ "$KEEP_APT" = "0" ]; then \
    apt-get remove -y procps || true; \
    apt-get autoremove -y; \
    apt-get clean; \
    rm -rf /var/lib/apt/lists/*; \
  fi
# Verify critical tools still present after optional cleanup (fail fast if missing)
RUN command -v curl >/dev/null && git --version >/dev/null

# Sleep by default; proxy will docker exec into this container
CMD ["sleep", "infinity"]
