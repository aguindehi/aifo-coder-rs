# Multi-stage Dockerfile for aifo-coder, producing one image per agent while
# sharing identical parent layers for maximum cache and storage reuse.

# Base layer: Node image + common OS tools used by all agents
FROM repository.migros.net/node:22-bookworm-slim AS base

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y --no-install-recommends \
    git gnupg pinentry-curses ca-certificates curl ripgrep dumb-init emacs-nox vim nano libnss-wrapper \
    build-essential pkg-config libssl-dev cargo rustc \
 && rm -rf /var/lib/apt/lists/*

# Default working directory; the host project will be mounted here
WORKDIR /workspace

# Install a tiny entrypoint to prep GnuPG runtime and launch gpg-agent if available
RUN install -d -m 0755 /usr/local/bin \
 && printf '%s\n' '#!/bin/sh' 'set -e' \
 'if [ -n "$HOME" ] && [ ! -d "$HOME" ]; then mkdir -p "$HOME"; fi' \
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
 '# Prefer a TTY for pinentry' \
 'if [ -t 0 ] || [ -t 1 ]; then export GPG_TTY="${GPG_TTY:-/dev/tty}"; fi' \
 '# Launch gpg-agent (best-effort)' \
 'if command -v gpgconf >/dev/null 2>&1; then gpgconf --launch gpg-agent >/dev/null 2>&1 || true; else gpg-agent --daemon >/dev/null 2>&1 || true; fi' \
 'exec "$@"' > /usr/local/bin/aifo-entrypoint \
 && chmod +x /usr/local/bin/aifo-entrypoint \
 && install -d -m 1777 /home/coder

# Common process entry point
ENTRYPOINT ["dumb-init", "--", "/usr/local/bin/aifo-entrypoint"]
CMD ["bash"]

# --- Codex image (adds only Codex CLI on top of base) ---
FROM base AS codex
# Codex docs: npm i -g @openai/codex
RUN npm install -g @openai/codex

# --- Crush image (adds only Crush CLI on top of base) ---
FROM base AS crush
# Crush docs: npm i -g @charmland/crush
RUN npm install -g @charmland/crush

# --- Aider image (adds Python + uv + aider-chat on top of base) ---
FROM base AS aider
# Install Python only for the Aider variant
RUN apt-get update && apt-get install -y --no-install-recommends \
    python3 python3-venv python3-pip \
 && rm -rf /var/lib/apt/lists/*
# Python: Aider via uv (PEP 668-safe)
RUN curl -LsSf https://astral.sh/uv/install.sh | sh && \
    mv /root/.local/bin/uv /usr/local/bin/uv && \
    uv venv /opt/venv && \
    uv pip install --python /opt/venv/bin/python --upgrade pip && \
    uv pip install --python /opt/venv/bin/python aider-chat
ENV PATH="/opt/venv/bin:${PATH}"
