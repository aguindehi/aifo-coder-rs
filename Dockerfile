# Multi-stage Dockerfile for aifo-coder, producing one image per agent while
# sharing identical parent layers for maximum cache and storage reuse.

# Default working directory at /workspace: the host project will be mounted there

ARG REGISTRY_PREFIX

# --- Base layer: Rust image ---
FROM ${REGISTRY_PREFIX}rust:1-bookworm AS rust-base
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get -y upgrade \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /workspace

# --- Rust target builder for Linux, Windows & macOS ---
FROM rust-base AS rust-builder
WORKDIR /workspace
ENV DEBIAN_FRONTEND=noninteractive
ENV PATH="/usr/local/cargo/bin:${PATH}"
RUN apt-get update \
    && apt-get -y upgrade \
    && apt-get -o APT::Keep-Downloaded-Packages=false install -y --no-install-recommends \
        gcc-mingw-w64-x86-64 \
        g++-mingw-w64-x86-64 \
        pkg-config \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && /usr/local/cargo/bin/rustup target add x86_64-pc-windows-gnu

# Build the Rust aifo-shim binary for the current build platform
COPY Cargo.toml .
COPY src ./src
RUN cargo build --release --bin aifo-shim

# --- Base layer: Node image + common OS tools used by all agents ---
FROM ${REGISTRY_PREFIX}node:22-bookworm-slim AS base
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
    git gnupg pinentry-curses ca-certificates curl ripgrep dumb-init emacs-nox vim nano mg nvi libnss-wrapper file \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /workspace

# embed compiled Rust PATH shim into agent images, but do not yet add to PATH
RUN install -d -m 0755 /opt/aifo/bin
COPY --from=rust-builder /workspace/target/release/aifo-shim /opt/aifo/bin/aifo-shim
RUN chmod 0755 /opt/aifo/bin/aifo-shim && \
    for t in cargo rustc node npm npx tsc ts-node python pip pip3 gcc g++ clang clang++ make cmake ninja pkg-config go gofmt; do ln -sf aifo-shim "/opt/aifo/bin/$t"; done
# will get added by the top layer
#ENV PATH="/opt/aifo/bin:${PATH}"

# Install a tiny entrypoint to prep GnuPG runtime and launch gpg-agent if available
RUN install -d -m 0755 /usr/local/bin \
 && printf '%s\n' '#!/bin/sh' 'set -e' \
 'if [ -z "$HOME" ]; then export HOME="/home/coder"; fi' \
 'if [ ! -d "$HOME" ]; then mkdir -p "$HOME"; fi' \
 'if [ -z "$GNUPGHOME" ]; then export GNUPGHOME="$HOME/.gnupg"; fi' \
 'mkdir -p "$GNUPGHOME"; chmod 700 "$GNUPGHOME" || true' \
 '# Ensure a private runtime dir for gpg-agent sockets if system one is unavailable' \
 'if [ -z "$XDG_RUNTIME_DIR" ]; then export XDG_RUNTIME_DIR="/tmp/runtime-$(id -u)"; fi' \
 'mkdir -p "$XDG_RUNTIME_DIR/gnupg"; chmod 700 "$XDG_RUNTIME_DIR" "$XDG_RUNTIME_DIR/gnupg" || true' \
 '# Copy keyrings from mounted host dir if present and not already in place' \
 'if [ -d "$HOME/.gnupg-host" ]; then' \
 '  for f in pubring.kbx trustdb.gpg gpg.conf gpg-agent.conf; do' \
 '    if [ -f "$HOME/.gnupg-host/$f" ] && [ ! -f "$GNUPGHOME/$f" ]; then cp -a "$HOME/.gnupg-host/$f" "$GNUPGHOME/$f"; fi' \
 '  done' \
 '  for d in private-keys-v1.d openpgp-revocs.d; do' \
 '    if [ -d "$HOME/.gnupg-host/$d" ] && [ ! -e "$GNUPGHOME/$d" ]; then cp -a "$HOME/.gnupg-host/$d" "$GNUPGHOME/$d"; fi' \
 '  done' \
 'fi' \
 '# Configure pinentry if not set' \
 'if [ ! -f "$GNUPGHOME/gpg-agent.conf" ] && command -v pinentry-curses >/dev/null 2>&1; then printf "pinentry-program /usr/bin/pinentry-curses\n" > "$GNUPGHOME/gpg-agent.conf"; fi' \
 'grep -q "^allow-loopback-pinentry" "$GNUPGHOME/gpg-agent.conf" 2>/dev/null || echo "allow-loopback-pinentry" >> "$GNUPGHOME/gpg-agent.conf"' \
 'grep -q "^default-cache-ttl " "$GNUPGHOME/gpg-agent.conf" 2>/dev/null || echo "default-cache-ttl 7200" >> "$GNUPGHOME/gpg-agent.conf"' \
 'grep -q "^max-cache-ttl " "$GNUPGHOME/gpg-agent.conf" 2>/dev/null || echo "max-cache-ttl 86400" >> "$GNUPGHOME/gpg-agent.conf"' \
 '# Prefer a TTY for pinentry' \
 'if [ -t 0 ] || [ -t 1 ]; then export GPG_TTY="${GPG_TTY:-/dev/tty}"; fi' \
 'unset GPG_AGENT_INFO' \
 '# Launch gpg-agent' \
 'if command -v gpgconf >/dev/null 2>&1; then gpgconf --kill gpg-agent >/dev/null 2>&1 || true; gpgconf --launch gpg-agent >/dev/null 2>&1 || true; else gpg-agent --daemon >/dev/null 2>&1 || true; fi' \
 'exec "$@"' > /usr/local/bin/aifo-entrypoint \
 && chmod +x /usr/local/bin/aifo-entrypoint \
 && install -d -m 1777 /home/coder

# Common process entry point
ENTRYPOINT ["dumb-init", "--", "/usr/local/bin/aifo-entrypoint"]
CMD ["bash"]

# --- Codex image (adds only Codex CLI on top of base) ---
FROM base AS codex
# Codex docs: npm i -g @openai/codex
RUN npm install -g --omit=dev --no-audit --no-fund --no-update-notifier --no-optional @openai/codex
ENV PATH="/opt/aifo/bin:${PATH}"
ARG KEEP_APT=0
# Optionally drop apt/procps from final image to reduce footprint
RUN if [ "$KEEP_APT" = "0" ]; then \
    apt-get remove -y procps || true; \
    apt-get autoremove -y; \
    apt-get clean; \
    apt-get remove --purge -y apt apt-get; \
    npm install -g  --omit=dev --no-audit --no-fund --no-update-notifier --no-optional; \
    npm prune -g --omit=dev; \
    npm cache clean --force; \
    rm -rf /root/.npm /root/.cache; \
    rm -rf /usr/share/doc/* /usr/share/man/* /usr/share/info/* /usr/share/locale/*; \
    rm -rf /var/lib/apt/lists/*; \
    rm -rf /var/cache/apt/apt-file/; \
    rm -f /usr/local/bin/node /usr/local/bin/nodejs /usr/local/bin/npm /usr/local/bin/npx /usr/local/bin/yarn /usr/local/bin/yarnpkg; \
    rm -rf /usr/local/lib/node_modules/npm/bin/npm-cli.js /usr/local/lib/node_modules/npm/bin/npx-cli.js; \
    rm -rf /opt/yarn-v1.22.22; \
  fi

# --- Crush image (adds only Crush CLI on top of base) ---
FROM base AS crush
# Crush docs: npm i -g @charmland/crush
RUN npm install -g --omit=dev --no-audit --no-fund --no-update-notifier --no-optional @charmland/crush
ENV PATH="/opt/aifo/bin:${PATH}"
ARG KEEP_APT=0
# Optionally drop apt/procps from final image to reduce footprint
RUN if [ "$KEEP_APT" = "0" ]; then \
    apt-get remove -y procps || true; \
    apt-get autoremove -y; \
    apt-get clean; \
    apt-get remove --purge -y apt apt-get; \
    npm install -g  --omit=dev --no-audit --no-fund --no-update-notifier --no-optional; \
    npm prune -g --omit=dev; \
    npm cache clean --force; \
    rm -rf /root/.npm /root/.cache; \
    rm -rf /usr/share/doc/* /usr/share/man/* /usr/share/info/* /usr/share/locale/*; \
    rm -rf /var/lib/apt/lists/*; \
    rm -rf /var/cache/apt/apt-file/; \
    rm -f /usr/local/bin/node /usr/local/bin/nodejs /usr/local/bin/npm /usr/local/bin/npx /usr/local/bin/yarn /usr/local/bin/yarnpkg; \
    rm -rf /usr/local/lib/node_modules/npm/bin/npm-cli.js /usr/local/lib/node_modules/npm/bin/npx-cli.js; \
    rm -rf /opt/yarn-v1.22.22; \
  fi

# --- Aider builder stage (with build tools, not shipped in final) ---
FROM base AS aider-builder
RUN apt-get update \
    && apt-get -y upgrade \
    && apt-get install -y --no-install-recommends \
    python3 python3-venv python3-pip build-essential pkg-config libssl-dev \
 && rm -rf /var/lib/apt/lists/*
# Python: Aider via uv (PEP 668-safe)
RUN curl -LsSf https://astral.sh/uv/install.sh | sh && \
    mv /root/.local/bin/uv /usr/local/bin/uv && \
    uv venv /opt/venv && \
    uv pip install --python /opt/venv/bin/python --upgrade pip && \
    uv pip install --python /opt/venv/bin/python aider-chat && \
    find /opt/venv -name 'pycache' -type d -exec rm -rf {} +; find /opt/venv -name '*.pyc' -delete && \
    rm -rf /root/.cache/uv /root/.cache/pip

# --- Aider runtime stage (no compilers; only Python runtime + venv) ---
FROM base AS aider
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
    python3-minimal \
 && rm -rf /var/lib/apt/lists/*
COPY --from=aider-builder /opt/venv /opt/venv
ENV PATH="/opt/venv/bin:${PATH}"
ENV PATH="/opt/aifo/bin:${PATH}"
ARG KEEP_APT=0
# Optionally drop apt/procps from final image to reduce footprint
RUN if [ "$KEEP_APT" = "0" ]; then \
    apt-get remove -y procps || true; \
    apt-get autoremove -y; \
    apt-get clean; \
    apt-get remove --purge -y apt apt-get; \
    rm -rf /var/lib/apt/lists/*; \
    rm -rf /var/cache/apt/apt-file/; \
    rm -f /var/lib/apt/lists/*; \
  fi

# --- Slim base (minimal tools, no editors/ripgrep) ---
FROM ${REGISTRY_PREFIX}node:22-bookworm-slim AS base-slim
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
    git gnupg pinentry-curses ca-certificates curl dumb-init mg nvi libnss-wrapper file \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /workspace

# embed compiled Rust PATH shim into slim images, but do not yet add to PATH
RUN install -d -m 0755 /opt/aifo/bin
COPY --from=rust-builder /workspace/target/release/aifo-shim /opt/aifo/bin/aifo-shim
RUN chmod 0755 /opt/aifo/bin/aifo-shim && \
    for t in cargo rustc node npm npx tsc ts-node python pip pip3 gcc g++ clang clang++ make cmake ninja pkg-config go gofmt; do ln -sf aifo-shim "/opt/aifo/bin/$t"; done
# will get added by the top layer
#ENV PATH="/opt/aifo/bin:${PATH}"

# Install a tiny entrypoint to prep GnuPG runtime and launch gpg-agent if available
RUN install -d -m 0755 /usr/local/bin \
 && printf '%s\n' '#!/bin/sh' 'set -e' \
 'if [ -z "$HOME" ]; then export HOME="/home/coder"; fi' \
 'if [ ! -d "$HOME" ]; then mkdir -p "$HOME"; fi' \
 'if [ -z "$GNUPGHOME" ]; then export GNUPGHOME="$HOME/.gnupg"; fi' \
 'mkdir -p "$GNUPGHOME"; chmod 700 "$GNUPGHOME" || true' \
 '# Ensure a private runtime dir for gpg-agent sockets if system one is unavailable' \
 'if [ -z "$XDG_RUNTIME_DIR" ]; then export XDG_RUNTIME_DIR="/tmp/runtime-$(id -u)"; fi' \
 'mkdir -p "$XDG_RUNTIME_DIR/gnupg"; chmod 700 "$XDG_RUNTIME_DIR" "$XDG_RUNTIME_DIR/gnupg" || true' \
 '# Copy keyrings from mounted host dir if present and not already in place' \
 'if [ -d "$HOME/.gnupg-host" ]; then' \
 '  for f in pubring.kbx trustdb.gpg gpg.conf gpg-agent.conf; do' \
 '    if [ -f "$HOME/.gnupg-host/$f" ] && [ ! -f "$GNUPGHOME/$f" ]; then cp -a "$HOME/.gnupg-host/$f" "$GNUPGHOME/$f"; fi' \
 '  done' \
 '  for d in private-keys-v1.d openpgp-revocs.d; do' \
 '    if [ -d "$HOME/.gnupg-host/$d" ] && [ ! -e "$GNUPGHOME/$d" ]; then cp -a "$HOME/.gnupg-host/$d" "$GNUPGHOME/$d"; fi' \
 '  done' \
 'fi' \
 '# Configure pinentry if not set' \
 'if [ ! -f "$GNUPGHOME/gpg-agent.conf" ] && command -v pinentry-curses >/dev/null 2>&1; then printf "pinentry-program /usr/bin/pinentry-curses\n" > "$GNUPGHOME/gpg-agent.conf"; fi' \
 'grep -q "^allow-loopback-pinentry" "$GNUPGHOME/gpg-agent.conf" 2>/dev/null || echo "allow-loopback-pinentry" >> "$GNUPGHOME/gpg-agent.conf"' \
 'grep -q "^default-cache-ttl " "$GNUPGHOME/gpg-agent.conf" 2>/dev/null || echo "default-cache-ttl 7200" >> "$GNUPGHOME/gpg-agent.conf"' \
 'grep -q "^max-cache-ttl " "$GNUPGHOME/gpg-agent.conf" 2>/dev/null || echo "max-cache-ttl 86400" >> "$GNUPGHOME/gpg-agent.conf"' \
 '# Prefer a TTY for pinentry' \
 'if [ -t 0 ] || [ -t 1 ]; then export GPG_TTY="${GPG_TTY:-/dev/tty}"; fi' \
 'unset GPG_AGENT_INFO' \
 '# Launch gpg-agent' \
 'if command -v gpgconf >/dev/null 2>&1; then gpgconf --kill gpg-agent >/dev/null 2>&1 || true; gpgconf --launch gpg-agent >/dev/null 2>&1 || true; else gpg-agent --daemon >/dev/null 2>&1 || true; fi' \
 'exec "$@"' > /usr/local/bin/aifo-entrypoint \
 && chmod +x /usr/local/bin/aifo-entrypoint \
 && install -d -m 1777 /home/coder

# Common process entry point
ENTRYPOINT ["dumb-init", "--", "/usr/local/bin/aifo-entrypoint"]
CMD ["bash"]

# --- Codex slim image ---
FROM base-slim AS codex-slim
RUN npm install -g --omit=dev --no-audit --no-fund --no-update-notifier --no-optional @openai/codex
ENV PATH="/opt/aifo/bin:${PATH}"
ARG KEEP_APT=0
# Optionally drop apt/procps from final image to reduce footprint
RUN if [ "$KEEP_APT" = "0" ]; then \
    apt-get remove -y procps || true; \
    apt-get autoremove -y; \
    apt-get clean; \
    apt-get remove --purge -y apt apt-get; \
    npm install -g  --omit=dev --no-audit --no-fund --no-update-notifier --no-optional; \
    npm prune -g --omit=dev; \
    npm cache clean --force; \
    rm -rf /root/.npm /root/.cache; \
    rm -rf /usr/share/doc/* /usr/share/man/* /usr/share/info/* /usr/share/locale/*; \
    rm -rf /var/lib/apt/lists/*; \
    rm -rf /var/cache/apt/apt-file/; \
    rm -f /usr/local/bin/node /usr/local/bin/nodejs /usr/local/bin/npm /usr/local/bin/npx /usr/local/bin/yarn /usr/local/bin/yarnpkg; \
    rm -rf /usr/local/lib/node_modules/npm/bin/npm-cli.js /usr/local/lib/node_modules/npm/bin/npx-cli.js; \
    rm -rf /opt/yarn-v1.22.22; \
  fi

# --- Crush slim image ---
FROM base-slim AS crush-slim
RUN npm install -g --omit=dev --no-audit --no-fund --no-update-notifier --no-optional @charmland/crush
ENV PATH="/opt/aifo/bin:${PATH}"
ARG KEEP_APT=0
# Optionally drop apt/procps from final image to reduce footprint
RUN if [ "$KEEP_APT" = "0" ]; then \
    apt-get remove -y procps || true; \
    apt-get autoremove -y; \
    apt-get clean; \
    apt-get remove --purge -y apt apt-get; \
    npm install -g  --omit=dev --no-audit --no-fund --no-update-notifier --no-optional; \
    npm prune -g --omit=dev; \
    npm cache clean --force; \
    rm -rf /root/.npm /root/.cache; \
    rm -rf /usr/share/doc/* /usr/share/man/* /usr/share/info/* /usr/share/locale/*; \
    rm -rf /var/lib/apt/lists/*; \
    rm -rf /var/cache/apt/apt-file/; \
    rm -f /usr/local/bin/node /usr/local/bin/nodejs /usr/local/bin/npm /usr/local/bin/npx /usr/local/bin/yarn /usr/local/bin/yarnpkg; \
    rm -rf /usr/local/lib/node_modules/npm/bin/npm-cli.js /usr/local/lib/node_modules/npm/bin/npx-cli.js; \
    rm -rf /opt/yarn-v1.22.22; \
  fi

# --- Aider slim builder stage ---
FROM base-slim AS aider-builder-slim
RUN apt-get update \
    && apt-get -y upgrade \
    && apt-get install -y --no-install-recommends \
    python3 python3-venv python3-pip build-essential pkg-config libssl-dev \
 && rm -rf /var/lib/apt/lists/*
# Python: Aider via uv (PEP 668-safe)
RUN curl -LsSf https://astral.sh/uv/install.sh | sh && \
    mv /root/.local/bin/uv /usr/local/bin/uv && \
    uv venv /opt/venv && \
    uv pip install --python /opt/venv/bin/python --upgrade pip && \
    uv pip install --python /opt/venv/bin/python aider-chat && \
    find /opt/venv -name 'pycache' -type d -exec rm -rf {} +; find /opt/venv -name '*.pyc' -delete && \
    rm -rf /root/.cache/uv /root/.cache/pip

# --- Aider slim runtime stage ---
FROM base-slim AS aider-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends python3-minimal && rm -rf /var/lib/apt/lists/*
COPY --from=aider-builder-slim /opt/venv /opt/venv
ENV PATH="/opt/venv/bin:${PATH}"
ENV PATH="/opt/aifo/bin:${PATH}"
ARG KEEP_APT=0
# Optionally drop apt/procps from final image to reduce footprint
RUN if [ "$KEEP_APT" = "0" ]; then \
    apt-get remove -y procps || true; \
    apt-get autoremove -y; \
    apt-get clean; \
    apt-get remove --purge -y apt apt-get; \
    rm -rf /var/lib/apt/lists/*; \
    rm -rf /var/cache/apt/apt-file/; \
  fi
