# Multi-stage Dockerfile for aifo-coder, producing one image per agent while
# sharing identical parent layers for maximum cache and storage reuse.

# Default working directory at /workspace: the host project will be mounted there

ARG REGISTRY_PREFIX
# CI builds use Kaniko --use-new-run; keep RUN --mount (secrets/cache); avoid COPY --link/--chmod.

# --- Base layer: Rust image ---
FROM ${REGISTRY_PREFIX}rust:1-slim-bookworm AS rust-base
ENV DEBIAN_FRONTEND=noninteractive
RUN --mount=type=secret,id=migros_root_ca,target=/run/secrets/migros_root_ca,required=false sh -lc 'set -e; if [ -f /run/secrets/migros_root_ca ]; then install -m 0644 /run/secrets/migros_root_ca /usr/local/share/ca-certificates/migros-root-ca.crt || true; command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; fi; apt-get update; rm -rf /var/lib/apt/lists/* /usr/share/doc/* /usr/share/man/* /usr/share/info/* /usr/share/locale/*; if [ -f /usr/local/share/ca-certificates/migros-root-ca.crt ]; then rm -f /usr/local/share/ca-certificates/migros-root-ca.crt; command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; fi'
WORKDIR /workspace

# --- Rust target builder for Linux, Windows & macOS ---
FROM rust-base AS rust-builder
ARG WITH_WIN=0
ARG CLEAN_CARGO=0
WORKDIR /workspace
ENV DEBIAN_FRONTEND=noninteractive
ENV PATH="/usr/local/cargo/bin:${PATH}"
ARG NEXTEST_VERSION=0.9.114
RUN --mount=type=secret,id=migros_root_ca,target=/run/secrets/migros_root_ca,required=false sh -lc 'set -e; \
    CAF=/run/secrets/migros_root_ca; \
    if [ -f "$CAF" ]; then \
        install -m 0644 "$CAF" /usr/local/share/ca-certificates/migros-root-ca.crt || true; \
        command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; \
        # Use the consolidated system CA bundle (includes enterprise CA) for all TLS clients \
        export SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt; \
        export SSL_CERT_DIR=/etc/ssl/certs; \
        export CARGO_HTTP_CAINFO=/etc/ssl/certs/ca-certificates.crt; \
        export CURL_CA_BUNDLE=/etc/ssl/certs/ca-certificates.crt; \
        export RUSTUP_USE_CURL=1; \
    fi; \
    apt-get update && apt-get -o APT::Keep-Downloaded-Packages=false install -y --no-install-recommends make git pkg-config git-lfs ca-certificates sccache; \
    if [ "${WITH_WIN:-0}" = "1" ]; then \
        apt-get -o APT::Keep-Downloaded-Packages=false install -y --no-install-recommends gcc-mingw-w64-x86-64 g++-mingw-w64-x86-64; \
        /usr/local/cargo/bin/rustup target add x86_64-pc-windows-gnu; \
    fi; \
    apt-get clean; rm -rf /var/lib/apt/lists/*; \
    /usr/local/cargo/bin/rustup set profile minimal; \
    /usr/local/cargo/bin/rustup component add llvm-tools-preview; \
    /usr/local/cargo/bin/rustup component add clippy rustfmt; \
    rm -rf /usr/local/rustup/downloads /usr/local/rustup/tmp; \
    if [ -f /usr/local/share/ca-certificates/migros-root-ca.crt ]; then \
        rm -f /usr/local/share/ca-certificates/migros-root-ca.crt; \
        command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; \
    fi'

# Pre-install cargo-nextest to speed up tests inside this container
RUN --mount=type=secret,id=migros_root_ca,target=/run/secrets/migros_root_ca,required=false --mount=type=cache,target=/usr/local/cargo/registry --mount=type=cache,target=/usr/local/cargo/git sh -lc 'set -e; \
    CAF=/run/secrets/migros_root_ca; \
    if [ -f "$CAF" ]; then \
        install -m 0644 "$CAF" /usr/local/share/ca-certificates/migros-root-ca.crt || true; \
        command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; \
        export SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt; \
        export CARGO_HTTP_CAINFO=/etc/ssl/certs/ca-certificates.crt; \
        export CURL_CA_BUNDLE=/etc/ssl/certs/ca-certificates.crt; \
    fi; \
    # Prefer prebuilt cargo-nextest to avoid heavy compile under QEMU; fallback to cargo install \
    arch="$(uname -m)"; \
    case "$arch" in x86_64|amd64) tgt="x86_64-unknown-linux-gnu" ;; aarch64|arm64) tgt="aarch64-unknown-linux-gnu" ;; *) tgt="" ;; esac; \
    ok=0; \
    if [ -n "$tgt" ]; then \
      url="https://github.com/nextest-rs/nextest/releases/download/cargo-nextest-${NEXTEST_VERSION}/cargo-nextest-${tgt}.tar.xz"; \
      if curl -fsSL "$url" -o /tmp/nextest.tar.xz; then \
        mkdir -p /tmp/nextest && tar -C /tmp/nextest -xf /tmp/nextest.tar.xz; \
        bin="$(find /tmp/nextest -type f -name cargo-nextest -print -quit)"; \
        if [ -n "$bin" ]; then install -m 0755 "$bin" /usr/local/cargo/bin/cargo-nextest; strip /usr/local/cargo/bin/cargo-nextest 2>/dev/null || true; ok=1; fi; \
        rm -rf /tmp/nextest /tmp/nextest.tar.xz; \
      fi; \
    fi; \
    if [ "$ok" -ne 1 ]; then /usr/local/cargo/bin/cargo install cargo-nextest --locked; fi; \
    /usr/local/cargo/bin/cargo install grcov --locked; \
    strip /usr/local/cargo/bin/cargo-nextest /usr/local/cargo/bin/grcov 2>/dev/null || true; \
    if [ "${CLEAN_CARGO:-0}" = "1" ]; then \
        find /usr/local/cargo/registry -mindepth 1 -maxdepth 1 -exec rm -rf {} + 2>/dev/null || true; \
        find /usr/local/cargo/git -mindepth 1 -maxdepth 1 -exec rm -rf {} + 2>/dev/null || true; \
    fi; \
    if [ -f /usr/local/share/ca-certificates/migros-root-ca.crt ]; then \
        rm -f /usr/local/share/ca-certificates/migros-root-ca.crt; \
        command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; \
    fi'

# --- Shim compile stage (throwaway; contains sources only) ---
FROM rust-base AS shim-builder
ARG CLEAN_CARGO=0
WORKDIR /workspace
ENV DEBIAN_FRONTEND=noninteractive
# Build the Rust aifo-shim binary for the current build platform without baking sources into rust-builder
COPY Cargo.toml .
COPY build.rs .
COPY src ./src
RUN --mount=type=secret,id=migros_root_ca,target=/run/secrets/migros_root_ca,required=false --mount=type=cache,target=/usr/local/cargo/registry --mount=type=cache,target=/usr/local/cargo/git sh -lc 'set -e; \
    CAF=/run/secrets/migros_root_ca; \
    if [ -f "$CAF" ]; then \
        install -m 0644 "$CAF" /usr/local/share/ca-certificates/migros-root-ca.crt || true; \
        command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; \
        export SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt; \
        export CARGO_HTTP_CAINFO=/etc/ssl/certs/ca-certificates.crt; \
        export CURL_CA_BUNDLE=/etc/ssl/certs/ca-certificates.crt; \
    fi; \
    /usr/local/cargo/bin/cargo build --release --bin aifo-shim; \
    install -d -m 0755 /workspace/out; \
    cp target/release/aifo-shim /workspace/out/aifo-shim; \
    strip /workspace/out/aifo-shim 2>/dev/null || true; \
    if [ "${CLEAN_CARGO:-0}" = "1" ]; then \
        find /usr/local/cargo/registry -mindepth 1 -maxdepth 1 -exec rm -rf {} + 2>/dev/null || true; \
        find /usr/local/cargo/git -mindepth 1 -maxdepth 1 -exec rm -rf {} + 2>/dev/null || true; \
    fi; \
    rm -rf target; \
    if [ -f /usr/local/share/ca-certificates/migros-root-ca.crt ]; then \
        rm -f /usr/local/share/ca-certificates/migros-root-ca.crt; \
        command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; \
    fi'

# --- Shim outputs stage (deduplicates wrapper/entrypoint) ---
FROM ${REGISTRY_PREFIX}node:22-bookworm-slim AS shim-common
WORKDIR /
# Build once: sh/bash/dash wrappers, tool symlinks, and aifo-entrypoint; consume via COPY in base/base-slim
RUN install -d -m 0755 /opt/aifo/bin
COPY --from=shim-builder /workspace/out/aifo-shim /opt/aifo/bin/aifo-shim
# Install sh wrappers and entrypoint in one layer to reduce image layers
# hadolint ignore=SC2016,SC2026,SC2145
RUN chmod 0755 /opt/aifo/bin/aifo-shim && \
  printf '%s\n' \
  '#!/bin/sh' \
  '# aifo-coder sh wrapper: auto-exit after -c/-lc commands and avoid lingering shells on Ctrl-C.' \
  '# Opt-out: AIFO_SH_WRAP_DISABLE=1' \
  'if [ "${AIFO_SH_WRAP_DISABLE:-0}" = "1" ]; then' \
  '  exec /bin/sh "$@"' \
  'fi' \
  '' \
  '# If interactive and this TTY was used for a recent tool exec, exit immediately.' \
  'if { [ -t 0 ] || [ -t 1 ] || [ -t 2 ]; }; then' \
  '  TTY_PATH="$(readlink -f "/proc/$$/fd/0" 2>/dev/null || readlink -f "/proc/$$/fd/1" 2>/dev/null || readlink -f "/proc/$$/fd/2" 2>/dev/null || true)"' \
  '  NOW="$(date +%s)"' \
  '  RECENT="${AIFO_SH_RECENT_SECS:-10}"' \
  '  if [ -n "$TTY_PATH" ] && [ -d "$HOME/.aifo-exec" ]; then' \
  '    for d in "$HOME"/.aifo-exec/*; do' \
  '      [ -d "$d" ] || continue' \
  '      if [ -f "$d/no_shell_on_tty" ] && [ -f "$d/tty" ] && [ "$(cat "$d/tty" 2>/dev/null)" = "$TTY_PATH" ]; then' \
  '        MTIME="$(stat -c %Y "$d" 2>/dev/null || stat -f %m "$d" 2>/dev/null || echo 0)"' \
  '        AGE="$((NOW - MTIME))"' \
  '        if [ "$AGE" -le "$RECENT" ] 2>/dev/null; then exit 0; fi' \
  '      fi' \
  '    done' \
  '  fi' \
  'fi' \
  '' \
  '# Normalize -lc to -c for dash/posix shells; do not append ; exit.' \
  'if [ "$#" -ge 2 ] && { [ "$1" = "-c" ] || [ "$1" = "-lc" ]; }; then' \
  '  flag="$1"' \
  '  cmd="$2"' \
  '  shift 2' \
  '  [ "$flag" = "-lc" ] && flag="-c"' \
  '  exec /bin/sh "$flag" "$cmd" "$@"' \
  'fi' \
  '' \
  'exec /bin/sh "$@"' \
  > /opt/aifo/bin/sh && chmod 0755 /opt/aifo/bin/sh && \
  sed 's#/bin/sh#/bin/bash#g' /opt/aifo/bin/sh > /opt/aifo/bin/bash && chmod 0755 /opt/aifo/bin/bash && \
  sed 's#/bin/sh#/bin/dash#g' /opt/aifo/bin/sh > /opt/aifo/bin/dash && chmod 0755 /opt/aifo/bin/dash && \
  for t in cargo rustc node npm npx yarn pnpm deno tsc ts-node python pip pip3 gcc g++ cc c++ clang clang++ make cmake ninja pkg-config go gofmt say; do ln -sf aifo-shim "/opt/aifo/bin/$t"; done && \
  install -d -m 0755 /usr/local/bin && \
  printf '%s\n' '#!/bin/sh' 'set -e' \
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
 'CFG_HOST="${AIFO_CONFIG_HOST_DIR:-$HOME/.aifo-config-host}"' \
 'CFG_DST="${AIFO_CONFIG_DST_DIR:-$HOME/.aifo-config}"' \
 'CFG_ENABLE="${AIFO_CONFIG_ENABLE:-1}"' \
 'CFG_MAX="${AIFO_CONFIG_MAX_SIZE:-262144}"' \
 'CFG_EXT="${AIFO_CONFIG_ALLOW_EXT:-json,toml,yaml,yml,ini,conf,crt,pem,key,token}"' \
 'CFG_HINTS="${AIFO_CONFIG_SECRET_HINTS:-token,secret,key,pem}"' \
 'CFG_COPY_ALWAYS="${AIFO_CONFIG_COPY_ALWAYS:-0}"' \
 'export AIFO_CODER_CONFIG_DIR="$CFG_DST"' \
 'if [ "$CFG_ENABLE" = "1" ]; then' \
 '  install -d -m 0700 "$CFG_DST" || true' \
 '  if [ -d "$CFG_HOST" ]; then' \
 '    STAMP="$CFG_DST/.copied"' \
 '    SHOULD=1' \
 '    if [ "$CFG_COPY_ALWAYS" != "1" ] && [ -f "$STAMP" ]; then' \
 '      max_src=0' \
 '      for f in "$CFG_HOST"/* "$CFG_HOST"/global/* "$CFG_HOST"/*/*; do [ -e "$f" ] || continue; mt="$(stat -c %Y "$f" 2>/dev/null || stat -f %m "$f" 2>/dev/null || echo 0)"; [ "$mt" -gt "$max_src" ] && max_src="$mt"; done' \
 '      dst_mt="$(stat -c %Y "$STAMP" 2>/dev/null || stat -f %m "$STAMP" 2>/dev/null || echo 0)"' \
 '      if [ "$max_src" -le "$dst_mt" ]; then SHOULD=0; fi' \
 '    fi' \
 '    if [ "$SHOULD" = "1" ] && [ "${AIFO_TOOLCHAIN_VERBOSE:-0}" = "1" ]; then echo "aifo-entrypoint: config: copying files from $CFG_HOST to $CFG_DST"; fi' \
 '    if [ "$SHOULD" != "1" ] && [ "${AIFO_TOOLCHAIN_VERBOSE:-0}" = "1" ]; then echo "aifo-entrypoint: config: skip copy (up-to-date)"; fi' \
 '    if [ "$SHOULD" = "1" ]; then' \
 '    copy_one() {' \
 '      src="$1"; base="$(basename "$src")";' \
 '      case "$base" in' \
 '        *[!A-Za-z0-9._-]*|"") [ "${AIFO_TOOLCHAIN_VERBOSE:-0}" = "1" ] && echo "aifo-entrypoint: config: skip invalid name: $base"; return ;;' \
 '      esac;' \
 '      ext="${base##*.}"; ext_lc="$(printf "%s" "$ext" | tr "A-Z" "a-z")";' \
 '      ok=0; IFS=,; for e in $CFG_EXT; do [ "$ext_lc" = "$(printf "%s" "$e" | tr "A-Z" "a-z")" ] && ok=1 && break; done; unset IFS; if [ "$ok" -ne 1 ]; then [ "${AIFO_TOOLCHAIN_VERBOSE:-0}" = "1" ] && echo "aifo-entrypoint: config: skip disallowed extension: $base"; return; fi' \
 '      [ -h "$src" ] && { [ "${AIFO_TOOLCHAIN_VERBOSE:-0}" = "1" ] && echo "aifo-entrypoint: config: skip symlink: $base"; return; }; [ -f "$src" ] || { [ "${AIFO_TOOLCHAIN_VERBOSE:-0}" = "1" ] && echo "aifo-entrypoint: config: skip non-regular: $base"; return; }; sz="$(wc -c < "$src" 2>/dev/null || echo 0)"; if [ "$sz" -gt "$CFG_MAX" ]; then [ "${AIFO_TOOLCHAIN_VERBOSE:-0}" = "1" ] && echo "aifo-entrypoint: config: skip oversized (sz=$sz): $base"; return; fi' \
 '      mode=0644;' \
 '      case "$ext_lc" in pem|key|token) mode=0600 ;; esac;' \
 '      hn="$(printf "%s" "$CFG_HINTS" | tr "A-Z" "a-z")"; nm="$(printf "%s" "$base" | tr "A-Z" "a-z")";' \
 '      IFS=,; for h in $hn; do case "$nm" in *"$h"*) mode=0600 ;; esac; done; unset IFS;' \
 '      install -m "$mode" "$src" "$CFG_DST/global/$base" >/dev/null 2>&1 || true' \
 '    }' \
 '    if [ -d "$CFG_HOST/global" ]; then install -d -m 0700 "$CFG_DST/global" >/dev/null 2>&1 || true; for f in "$CFG_HOST"/global/.* "$CFG_HOST"/global/*; do [ -e "$f" ] || continue; copy_one "$f"; done; fi' \
 '    for d in "$CFG_HOST"/*; do [ -d "$d" ] || continue; name="$(basename "$d")"; [ "$name" = "global" ] && continue; install -d -m 0700 "$CFG_DST/$name" >/dev/null 2>&1 || true; for f in "$d"/.* "$d"/*; do [ -e "$f" ] || continue; [ -h "$f" ] && { [ "${AIFO_TOOLCHAIN_VERBOSE:-0}" = "1" ] && echo "aifo-entrypoint: config: skip symlink: $f"; continue; }; [ -f "$f" ] || { [ "${AIFO_TOOLCHAIN_VERBOSE:-0}" = "1" ] && echo "aifo-entrypoint: config: skip non-regular: $f"; continue; }; base="$(basename "$f")"; case "$base" in *[!A-Za-z0-9._-]*|"") [ "${AIFO_TOOLCHAIN_VERBOSE:-0}" = "1" ] && echo "aifo-entrypoint: config: skip invalid name: $name/$base"; continue ;; esac; ext="${base##*.}"; ext_lc="$(printf "%s" "$ext" | tr "A-Z" "a-z")"; ok=0; IFS=,; for e in $CFG_EXT; do [ "$ext_lc" = "$(printf "%s" "$e" | tr "A-Z" "a-z")" ] && ok=1 && break; done; unset IFS; if [ "$ok" -ne 1 ]; then [ "${AIFO_TOOLCHAIN_VERBOSE:-0}" = "1" ] && echo "aifo-entrypoint: config: skip disallowed extension: $name/$base"; continue; fi; sz="$(wc -c < "$f" 2>/dev/null || echo 0)"; if [ "$sz" -gt "$CFG_MAX" ]; then [ "${AIFO_TOOLCHAIN_VERBOSE:-0}" = "1" ] && echo "aifo-entrypoint: config: skip oversized (sz=$sz): $name/$base"; continue; fi; mode=0644; case "$ext_lc" in pem|key|token) mode=0600 ;; esac; hn="$(printf "%s" "$CFG_HINTS" | tr "A-Z" "a-z")"; nm="$(printf "%s" "$base" | tr "A-Z" "a-z")"; IFS=,; for h in $hn; do case "$nm" in *"$h"*) mode=0600 ;; esac; done; unset IFS; install -m "$mode" "$f" "$CFG_DST/$name/$base" >/dev/null 2>&1 || true; done; done' \
 '    for bf in ".aider.conf.yml" ".aider.model.settings.yml" ".aider.model.metadata.json"; do' \
 '      if [ -f "$CFG_DST/aider/$bf" ]; then install -m 0644 "$CFG_DST/aider/$bf" "$HOME/$bf" >/dev/null 2>&1 || true; fi' \
 '    done' \
 '    touch "$CFG_DST/.copied" >/dev/null 2>&1 || true' \
 '    fi' \
 '  fi' \
 'fi' \
 'exec "$@"' > /usr/local/bin/aifo-entrypoint \
 && chmod +x /usr/local/bin/aifo-entrypoint


# --- Base layer: Node image + common OS tools used by all agents ---
FROM ${REGISTRY_PREFIX}node:22-bookworm-slim AS base
ENV DEBIAN_FRONTEND=noninteractive
RUN --mount=type=secret,id=migros_root_ca,target=/run/secrets/migros_root_ca,required=false sh -lc 'set -e; if [ -f /run/secrets/migros_root_ca ]; then install -m 0644 /run/secrets/migros_root_ca /usr/local/share/ca-certificates/migros-root-ca.crt || true; command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; fi; apt-get update && apt-get -o APT::Keep-Downloaded-Packages=false install -y --no-install-recommends git gnupg pinentry-curses ca-certificates curl ripgrep dumb-init procps emacs-nox vim nano mg nvi libnss-wrapper file; rm -rf /var/lib/apt/lists/* /usr/share/doc/* /usr/share/man/* /usr/share/info/* /usr/share/locale/*; if [ -f /usr/local/share/ca-certificates/migros-root-ca.crt ]; then rm -f /usr/local/share/ca-certificates/migros-root-ca.crt; command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; fi'
WORKDIR /workspace

# Copy shims and wrappers from shim-common
COPY --from=shim-common /opt/aifo/bin /opt/aifo/bin
ENV PATH="/opt/aifo/bin:${PATH}"

# Copy entrypoint from shim-common and ensure HOME exists
COPY --from=shim-common /usr/local/bin/aifo-entrypoint /usr/local/bin/aifo-entrypoint
RUN install -d -m 1777 /home/coder

# Common process entry point
ENTRYPOINT ["dumb-init", "--", "/usr/local/bin/aifo-entrypoint"]
CMD ["bash"]

# --- Codex image (adds only Codex CLI on top of base) ---
FROM base AS codex
ARG KEEP_APT=0
ENV KEEP_APT=${KEEP_APT}
# Codex docs: npm i -g @openai/codex
RUN --mount=type=secret,id=migros_root_ca,target=/run/secrets/migros_root_ca,required=false sh -lc 'set -e; CAF=/run/secrets/migros_root_ca; if [ -f "$CAF" ]; then install -m 0644 "$CAF" /usr/local/share/ca-certificates/migros-root-ca.crt || true; command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; export NODE_EXTRA_CA_CERTS="$CAF"; export NODE_OPTIONS="${NODE_OPTIONS:+$NODE_OPTIONS }--use-openssl-ca"; export SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt; fi; export NPM_CONFIG_CACHE=/tmp/npm-cache; npm install -g --omit=dev --no-audit --no-fund --no-update-notifier --no-optional @openai/codex; rm -rf /tmp/npm-cache /root/.npm /root/.cache; if [ -f /usr/local/share/ca-certificates/migros-root-ca.crt ]; then rm -f /usr/local/share/ca-certificates/migros-root-ca.crt; command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; fi; if [ "$KEEP_APT" = "0" ]; then apt-get remove -y procps || true; apt-get autoremove -y; apt-get clean; apt-get remove --purge -y --allow-remove-essential apt || true; rm -rf /tmp/npm-cache /root/.npm /root/.cache; rm -rf /usr/share/doc/* /usr/share/man/* /usr/share/info/* /usr/share/locale/*; rm -rf /var/lib/apt/lists/*; rm -rf /var/cache/apt/apt-file/; rm -f /usr/local/bin/npm /usr/local/bin/npx /usr/local/bin/yarn /usr/local/bin/yarnpkg; rm -rf /usr/local/lib/node_modules/npm/bin/npm-cli.js /usr/local/lib/node_modules/npm/bin/npx-cli.js; rm -rf /opt/yarn-v1.22.22; fi'
# Inherit /opt/aifo/bin PATH from base
# Cleanup merged into install RUN above (conditional via KEEP_APT)

# --- Crush image (adds only Crush CLI on top of base) ---
FROM base AS crush
ARG KEEP_APT=0
ENV KEEP_APT=${KEEP_APT}
# Crush docs: npm i -g @charmland/crush
RUN --mount=type=secret,id=migros_root_ca,target=/run/secrets/migros_root_ca,required=false sh -lc 'set -e; CAF=/run/secrets/migros_root_ca; if [ -f "$CAF" ]; then install -m 0644 "$CAF" /usr/local/share/ca-certificates/migros-root-ca.crt || true; command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; export NODE_EXTRA_CA_CERTS="$CAF"; export NODE_OPTIONS="${NODE_OPTIONS:+$NODE_OPTIONS }--use-openssl-ca"; export SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt; fi; export NPM_CONFIG_CACHE=/tmp/npm-cache; npm install -g --omit=dev --no-audit --no-fund --no-update-notifier --no-optional @charmland/crush; rm -rf /tmp/npm-cache /root/.npm /root/.cache; if [ -f /usr/local/share/ca-certificates/migros-root-ca.crt ]; then rm -f /usr/local/share/ca-certificates/migros-root-ca.crt; command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; fi; if [ "$KEEP_APT" = "0" ]; then apt-get remove -y procps || true; apt-get autoremove -y; apt-get clean; apt-get remove --purge -y --allow-remove-essential apt || true; rm -rf /tmp/npm-cache /root/.npm /root/.cache; rm -rf /usr/share/doc/* /usr/share/man/* /usr/share/info/* /usr/share/locale/*; rm -rf /var/lib/apt/lists/*; rm -rf /var/cache/apt/apt-file/; rm -f /usr/local/bin/npm /usr/local/bin/npx /usr/local/bin/yarn /usr/local/bin/yarnpkg; rm -rf /usr/local/lib/node_modules/npm/bin/npm-cli.js /usr/local/lib/node_modules/npm/bin/npx-cli.js; rm -rf /opt/yarn-v1.22.22; fi'
# Inherit /opt/aifo/bin PATH from base
# Cleanup merged into install RUN above (conditional via KEEP_APT)

# --- Aider builder stage (with build tools, not shipped in final) ---
FROM base AS aider-builder
RUN --mount=type=secret,id=migros_root_ca,target=/run/secrets/migros_root_ca,required=false sh -lc 'set -e; if [ -f /run/secrets/migros_root_ca ]; then install -m 0644 /run/secrets/migros_root_ca /usr/local/share/ca-certificates/migros-root-ca.crt || true; command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; fi; apt-get update && apt-get -o APT::Keep-Downloaded-Packages=false install -y --no-install-recommends python3 python3-venv python3-pip build-essential pkg-config libssl-dev; rm -rf /var/lib/apt/lists/* /usr/share/doc/* /usr/share/man/* /usr/share/info/* /usr/share/locale/*; if [ -f /usr/local/share/ca-certificates/migros-root-ca.crt ]; then rm -f /usr/local/share/ca-certificates/migros-root-ca.crt; command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; fi'
# Python: Aider via uv (PEP 668-safe)
ARG WITH_PLAYWRIGHT=1
ARG KEEP_APT=0
ENV KEEP_APT=${KEEP_APT}
RUN --mount=type=secret,id=migros_root_ca,target=/run/secrets/migros_root_ca,required=false sh -lc 'set -e; \
    CAF=/run/secrets/migros_root_ca; \
    if [ -f "$CAF" ]; then \
        install -m 0644 "$CAF" /usr/local/share/ca-certificates/migros-root-ca.crt || true; \
        command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; \
        export CURL_CA_BUNDLE=/etc/ssl/certs/ca-certificates.crt; \
        export REQUESTS_CA_BUNDLE=/etc/ssl/certs/ca-certificates.crt; \
        export SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt; \
        export UV_NATIVE_TLS=1; \
    fi; \
    if command -v curl >/dev/null 2>&1; then \
        curl -LsSf https://astral.sh/uv/install.sh -o /tmp/uv.sh; \
    else \
        python3 -c "import urllib.request; open(\"/tmp/uv.sh\",\"wb\").write(urllib.request.urlopen(\"https://astral.sh/uv/install.sh\").read())"; \
    fi; \
    sh /tmp/uv.sh; \
    mv /root/.local/bin/uv /usr/local/bin/uv; \
    uv venv /opt/venv; \
    uv pip install --native-tls --python /opt/venv/bin/python --upgrade pip; \
    uv pip install --native-tls --python /opt/venv/bin/python aider-chat; \
    if [ "$WITH_PLAYWRIGHT" = "1" ]; then \
        uv pip install --native-tls --python /opt/venv/bin/python "aider-chat[playwright]"; \
        uv pip install --native-tls --python /opt/venv/bin/python playwright; \
        /opt/venv/bin/python -c "import playwright" >/dev/null 2>&1 || { echo "error: playwright module missing in venv" >&2; exit 3; }; \
    fi; \
    find /opt/venv -name "pycache" -type d -exec rm -rf {} +; find /opt/venv -name "*.pyc" -delete; \
    rm -rf /root/.cache/uv /root/.cache/pip; \
    if [ -f /usr/local/share/ca-certificates/migros-root-ca.crt ]; then \
        rm -f /usr/local/share/ca-certificates/migros-root-ca.crt; \
        command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; \
    fi; \
    if [ "$KEEP_APT" = "0" ]; then \
        apt-get remove -y procps || true; \
        apt-get autoremove -y; \
        apt-get clean; \
        apt-get remove --purge -y --allow-remove-essential apt || true; \
        rm -rf /tmp/npm-cache /root/.npm /root/.cache; \
        rm -rf /usr/share/doc/* /usr/share/man/* /usr/share/info/* /usr/share/locale/*; \
        rm -rf /var/lib/apt/lists/*; \
        rm -rf /var/cache/apt/apt-file/; \
        rm -f /usr/local/bin/node /usr/local/bin/nodejs /usr/local/bin/npm /usr/local/bin/npx /usr/local/bin/yarn /usr/local/bin/yarnpkg; \
        rm -rf /usr/local/lib/node_modules/npm/bin/npm-cli.js /usr/local/lib/node_modules/npm/bin/npx-cli.js; \
        rm -rf /opt/yarn-v1.22.22; \
    fi'

# --- Aider runtime stage (no compilers; only Python runtime + venv) ---
FROM base AS aider
RUN --mount=type=secret,id=migros_root_ca,target=/run/secrets/migros_root_ca,required=false sh -lc 'set -e; if [ -f /run/secrets/migros_root_ca ]; then install -m 0644 /run/secrets/migros_root_ca /usr/local/share/ca-certificates/migros-root-ca.crt || true; command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; fi; apt-get update && apt-get -o APT::Keep-Downloaded-Packages=false install -y --no-install-recommends python3; rm -rf /var/lib/apt/lists/* /usr/share/doc/* /usr/share/man/* /usr/share/info/* /usr/share/locale/*; if [ -f /usr/local/share/ca-certificates/migros-root-ca.crt ]; then rm -f /usr/local/share/ca-certificates/migros-root-ca.crt; command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; fi'
COPY --from=aider-builder /opt/venv /opt/venv
ENV PATH="/opt/venv/bin:${PATH}"
# Inherit /opt/aifo/bin PATH from base
ENV PLAYWRIGHT_BROWSERS_PATH="/ms-playwright"
ARG WITH_PLAYWRIGHT=1
ARG KEEP_APT=0
RUN --mount=type=secret,id=migros_root_ca,target=/run/secrets/migros_root_ca,required=false sh -lc 'set -e; \
    if [ "$WITH_PLAYWRIGHT" = "1" ]; then \
        CAF=/run/secrets/migros_root_ca; \
        if [ -f "$CAF" ]; then \
            install -m 0644 "$CAF" /usr/local/share/ca-certificates/migros-root-ca.crt || true; \
            command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; \
            export NODE_EXTRA_CA_CERTS="$CAF"; \
            export NODE_OPTIONS="${NODE_OPTIONS:+$NODE_OPTIONS }--use-openssl-ca"; \
            export SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt; \
        fi; \
        /opt/venv/bin/python -m playwright install --with-deps chromium; \
        if [ -f /usr/local/share/ca-certificates/migros-root-ca.crt ]; then \
            rm -f /usr/local/share/ca-certificates/migros-root-ca.crt; \
            command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; \
        fi; \
    fi; \
    if [ "$KEEP_APT" = "0" ]; then \
        apt-get remove -y procps || true; \
        apt-get autoremove -y; \
        apt-get clean; \
        apt-get remove --purge -y --allow-remove-essential apt || true; \
        rm -rf /tmp/npm-cache /root/.npm /root/.cache; \
        rm -rf /usr/share/doc/* /usr/share/man/* /usr/share/info/* /usr/share/locale/*; \
        rm -rf /var/lib/apt/lists/*; \
        rm -rf /var/cache/apt/apt-file/; \
        rm -f /usr/local/bin/node /usr/local/bin/nodejs /usr/local/bin/npm /usr/local/bin/npx /usr/local/bin/yarn /usr/local/bin/yarnpkg; \
        rm -rf /usr/local/lib/node_modules/npm/bin/npm-cli.js /usr/local/lib/node_modules/npm/bin/npx-cli.js; \
        rm -rf /opt/yarn-v1.22.22; \
    fi'
# Cleanup merged into Playwright install RUN above (conditional via KEEP_APT)

# --- OpenHands image (uv tool install; shims-first PATH) ---
FROM base AS openhands
ARG OPENHANDS_CONSTRAINT=""
ARG KEEP_APT=0
ENV KEEP_APT=${KEEP_APT}
RUN --mount=type=secret,id=migros_root_ca,target=/run/secrets/migros_root_ca,required=false sh -lc 'set -e; \
  CAF=/run/secrets/migros_root_ca; \
  if [ -f "$CAF" ]; then \
    install -m 0644 "$CAF" /usr/local/share/ca-certificates/migros-root-ca.crt || true; \
    command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; \
    export CURL_CA_BUNDLE=/etc/ssl/certs/ca-certificates.crt; \
    export REQUESTS_CA_BUNDLE=/etc/ssl/certs/ca-certificates.crt; \
    export SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt; \
    export SSL_CERT_DIR=/etc/ssl/certs; \
  fi; \
  export UV_NATIVE_TLS=1; \
  curl -LsSf https://astral.sh/uv/install.sh -o /tmp/uv.sh; \
  sh /tmp/uv.sh; \
  mv /root/.local/bin/uv /usr/local/bin/uv; \
  PKG="openhands-ai"; \
  if [ -n "$OPENHANDS_CONSTRAINT" ]; then PKG="openhands-ai==$OPENHANDS_CONSTRAINT"; fi; \
  install -d -m 0755 /opt/uv-home; \
  HOME=/opt/uv-home uv venv -p 3.12 /opt/venv-openhands; \
  HOME=/opt/uv-home uv pip install --native-tls --python /opt/venv-openhands/bin/python --upgrade pip; \
  HOME=/opt/uv-home uv pip install --native-tls --python /opt/venv-openhands/bin/python "$PKG"; \
  ln -sf /opt/venv-openhands/bin/openhands /usr/local/bin/openhands; \
  if [ ! -x /opt/venv-openhands/bin/openhands ]; then ls -la /opt/venv-openhands/bin; echo "error: missing openhands console script"; exit 3; fi; \
  if [ ! -x /usr/local/bin/openhands ]; then ls -la /usr/local/bin; echo "error: missing openhands wrapper"; exit 2; fi; \
  # Ensure non-root can traverse uv-managed Python under /opt/uv-home (shebang interpreter resolution)
  find /opt/uv-home/.local/share/uv/python -type d -exec chmod 0755 {} + 2>/dev/null || true; \
  find /opt/uv-home/.local/share/uv/python -type f -name "python*" -exec chmod 0755 {} + 2>/dev/null || true; \
  rm -rf /root/.cache/uv /root/.cache/pip; \
  if [ -f /usr/local/share/ca-certificates/migros-root-ca.crt ]; then \
    rm -f /usr/local/share/ca-certificates/migros-root-ca.crt; \
    command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; \
  fi; \
  if [ "$KEEP_APT" = "0" ]; then \
    apt-get remove -y procps || true; \
    apt-get autoremove -y; \
    apt-get clean; \
    apt-get remove --purge -y --allow-remove-essential apt || true; \
    npm prune --omit=dev || true; \
    npm cache clean --force; \
    rm -rf /root/.npm /root/.cache; \
    rm -rf /usr/share/doc/* /usr/share/man/* /usr/share/info/* /usr/share/locale/*; \
    rm -rf /var/lib/apt/lists/*; \
    rm -rf /var/cache/apt/apt-file/; \
  fi'
# Inherit /opt/aifo/bin PATH from base
# Cleanup merged into install RUN above (conditional via KEEP_APT)

# --- OpenCode image (npm install; shims-first PATH) ---
FROM base AS opencode
ARG OPCODE_VERSION=latest
ARG KEEP_APT=0
ENV KEEP_APT=${KEEP_APT}
RUN --mount=type=secret,id=migros_root_ca,target=/run/secrets/migros_root_ca,required=false sh -lc 'set -e; \
  CAF=/run/secrets/migros_root_ca; \
  if [ -f "$CAF" ]; then \
    install -m 0644 "$CAF" /usr/local/share/ca-certificates/migros-root-ca.crt || true; \
    command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; \
    export NODE_EXTRA_CA_CERTS="$CAF"; \
    export NODE_OPTIONS="${NODE_OPTIONS:+$NODE_OPTIONS }--use-openssl-ca"; \
    export SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt; \
    export SSL_CERT_DIR=/etc/ssl/certs; \
    export CURL_CA_BUNDLE=/etc/ssl/certs/ca-certificates.crt; \
  fi; \
  export NPM_CONFIG_CACHE=/tmp/npm-cache; \
  npm install -g --omit=dev --no-audit --no-fund --no-update-notifier --no-optional "opencode-ai@${OPCODE_VERSION}"; \
  rm -rf /tmp/npm-cache /root/.npm /root/.cache; \
  if [ -f /usr/local/share/ca-certificates/migros-root-ca.crt ]; then \
    rm -f /usr/local/share/ca-certificates/migros-root-ca.crt; \
    command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; \
  fi; \
  if [ "$KEEP_APT" = "0" ]; then \
    apt-get remove -y procps || true; \
    apt-get autoremove -y; \
    apt-get clean; \
    apt-get remove --purge -y --allow-remove-essential apt || true; \
    rm -rf /tmp/npm-cache /root/.npm /root/.cache; \
    rm -rf /usr/share/doc/* /usr/share/man/* /usr/share/info/* /usr/share/locale/*; \
    rm -rf /var/lib/apt/lists/*; \
    rm -rf /var/cache/apt/apt-file/; \
  fi'
# Inherit /opt/aifo/bin PATH from base
# Cleanup merged into install RUN above (conditional via KEEP_APT)

# --- Plandex builder (Go) ---
FROM --platform=$BUILDPLATFORM ${REGISTRY_PREFIX}golang:1.23-bookworm AS plandex-builder
ARG BUILDPLATFORM
ARG TARGETPLATFORM
ARG TARGETOS
ARG TARGETARCH
ARG PLX_GIT_REF=main
WORKDIR /src
ENV DEBIAN_FRONTEND=noninteractive
ENV PATH="/usr/local/go/bin:${PATH}"
# Harden Go build; conservative flags to reduce concurrency and preemption
ENV GOTOOLCHAIN=local \
    GOFLAGS="-trimpath -mod=readonly -p=1" \
    GOMAXPROCS=1 \
    GODEBUG=asyncpreemptoff=1
RUN --mount=type=secret,id=migros_root_ca,target=/run/secrets/migros_root_ca,required=false sh -lc 'set -e; \
  CAF=/run/secrets/migros_root_ca; \
  if [ -f "$CAF" ]; then \
    install -m 0644 "$CAF" /usr/local/share/ca-certificates/migros-root-ca.crt || true; \
    command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; \
    export GIT_SSL_CAINFO=/etc/ssl/certs/ca-certificates.crt; \
    export SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt; \
    export CURL_CA_BUNDLE=/etc/ssl/certs/ca-certificates.crt; \
  fi; \
  apt-get update && apt-get install -y --no-install-recommends ca-certificates git && rm -rf /var/lib/apt/lists/*; \
  git clone https://github.com/plandex-ai/plandex.git .; \
  git -c advice.detachedHead=false checkout "$PLX_GIT_REF" || true; \
  mkdir -p /out; \
  cd app/cli; \
  export PATH="/usr/local/go/bin:${PATH}"; \
  export CGO_ENABLED=0; \
  export GOFLAGS="${GOFLAGS:- -trimpath -mod=readonly -p=1}"; \
  export GOMAXPROCS="${GOMAXPROCS:-1}"; \
  export GODEBUG="${GODEBUG:-asyncpreemptoff=1}"; \
  V="$([ -f version.txt ] && cat version.txt || echo dev)"; \
  LDFLAGS="-s -w -X plandex/version.Version=$V"; \
  case "${TARGETOS:-}" in "") GOOS="$(/usr/local/go/bin/go env GOOS)";; *) GOOS="$TARGETOS";; esac; \
  case "${TARGETARCH:-}" in "") GOARCH="$(/usr/local/go/bin/go env GOARCH)";; *) GOARCH="$TARGETARCH";; esac; \
  if [ "$GOARCH" = "amd64" ]; then export GOAMD64="${GOAMD64:-v1}"; fi; \
  GOOS="$GOOS" GOARCH="$GOARCH" /usr/local/go/bin/go build -ldflags "$LDFLAGS" -o /out/plandex .; \
  rm -rf /root/go/pkg /go/pkg/mod; \
  if [ -f /usr/local/share/ca-certificates/migros-root-ca.crt ]; then \
    rm -f /usr/local/share/ca-certificates/migros-root-ca.crt; \
    command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; \
  fi'
# --- Plandex runtime (copy binary; shims-first PATH) ---
FROM base AS plandex
COPY --from=plandex-builder /out/plandex /usr/local/bin/plandex
ARG KEEP_APT=0
ENV KEEP_APT=${KEEP_APT}
ENV KEEP_APT=${KEEP_APT}
RUN chmod 0755 /usr/local/bin/plandex; strip /usr/local/bin/plandex || true; \
    if [ "$KEEP_APT" = "0" ]; then \
      apt-get remove -y procps || true; \
      apt-get autoremove -y; \
      apt-get clean; \
      apt-get remove --purge -y --allow-remove-essential apt || true; \
      rm -rf /tmp/npm-cache /root/.npm /root/.cache; \
      rm -rf /usr/share/doc/* /usr/share/man/* /usr/share/info/*; \
      rm -rf /usr/share/locale/*; \
      rm -rf /var/lib/apt/lists/*; \
      rm -rf /var/cache/apt/apt-file/; \
    fi

# --- Slim base (minimal tools, no editors/ripgrep) ---
FROM ${REGISTRY_PREFIX}node:22-bookworm-slim AS base-slim
ENV DEBIAN_FRONTEND=noninteractive
RUN --mount=type=secret,id=migros_root_ca,target=/run/secrets/migros_root_ca,required=false sh -lc 'set -e; if [ -f /run/secrets/migros_root_ca ]; then install -m 0644 /run/secrets/migros_root_ca /usr/local/share/ca-certificates/migros-root-ca.crt || true; command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; fi; apt-get update && apt-get -o APT::Keep-Downloaded-Packages=false install -y --no-install-recommends git gnupg pinentry-curses ca-certificates curl dumb-init mg nvi libnss-wrapper file; rm -rf /var/lib/apt/lists/* /usr/share/doc/* /usr/share/man/* /usr/share/info/* /usr/share/locale/*; if [ -f /usr/local/share/ca-certificates/migros-root-ca.crt ]; then rm -f /usr/local/share/ca-certificates/migros-root-ca.crt; command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; fi'
WORKDIR /workspace

# Copy shims and wrappers from shim-common
COPY --from=shim-common /opt/aifo/bin /opt/aifo/bin
ENV PATH="/opt/aifo/bin:${PATH}"

# Copy entrypoint from shim-common and ensure HOME exists
COPY --from=shim-common /usr/local/bin/aifo-entrypoint /usr/local/bin/aifo-entrypoint
RUN install -d -m 1777 /home/coder

# Common process entry point
ENTRYPOINT ["dumb-init", "--", "/usr/local/bin/aifo-entrypoint"]
CMD ["bash"]

# --- Codex slim image ---
FROM base-slim AS codex-slim
ARG KEEP_APT=0
ENV KEEP_APT=${KEEP_APT}
RUN --mount=type=secret,id=migros_root_ca,target=/run/secrets/migros_root_ca,required=false sh -lc 'set -e; CAF=/run/secrets/migros_root_ca; if [ -f "$CAF" ]; then install -m 0644 "$CAF" /usr/local/share/ca-certificates/migros-root-ca.crt || true; command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; export NODE_EXTRA_CA_CERTS="$CAF"; export NODE_OPTIONS="${NODE_OPTIONS:+$NODE_OPTIONS }--use-openssl-ca"; export SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt; fi; export NPM_CONFIG_CACHE=/tmp/npm-cache; npm install -g --omit=dev --no-audit --no-fund --no-update-notifier --no-optional @openai/codex; rm -rf /tmp/npm-cache /root/.npm /root/.cache; if [ -f /usr/local/share/ca-certificates/migros-root-ca.crt ]; then rm -f /usr/local/share/ca-certificates/migros-root-ca.crt; command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; fi; if [ "$KEEP_APT" = "0" ]; then apt-get remove -y procps curl || true; apt-get autoremove -y; apt-get clean; apt-get remove --purge -y --allow-remove-essential apt || true; rm -rf /tmp/npm-cache /root/.npm /root/.cache; rm -rf /usr/share/doc/* /usr/share/man/* /usr/share/info/* /usr/share/locale/*; rm -rf /var/lib/apt/lists/*; rm -rf /var/cache/apt/apt-file/; rm -f /usr/local/bin/node /usr/local/bin/nodejs /usr/local/bin/npm /usr/local/bin/npx /usr/local/bin/yarn /usr/local/bin/yarnpkg; rm -rf /usr/local/lib/node_modules/npm/bin/npm-cli.js /usr/local/lib/node_modules/npm/bin/npx-cli.js; rm -rf /opt/yarn-v1.22.22; fi'
# Inherit /opt/aifo/bin PATH from base

# --- Crush slim image ---
FROM base-slim AS crush-slim
ARG KEEP_APT=0
ENV KEEP_APT=${KEEP_APT}
RUN --mount=type=secret,id=migros_root_ca,target=/run/secrets/migros_root_ca,required=false sh -lc 'set -e; CAF=/run/secrets/migros_root_ca; if [ -f "$CAF" ]; then install -m 0644 "$CAF" /usr/local/share/ca-certificates/migros-root-ca.crt || true; command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; export NODE_EXTRA_CA_CERTS="$CAF"; export NODE_OPTIONS="${NODE_OPTIONS:+$NODE_OPTIONS }--use-openssl-ca"; export SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt; fi; export NPM_CONFIG_CACHE=/tmp/npm-cache; npm install -g --omit=dev --no-audit --no-fund --no-update-notifier --no-optional @charmland/crush; rm -rf /tmp/npm-cache /root/.npm /root/.cache; if [ -f /usr/local/share/ca-certificates/migros-root-ca.crt ]; then rm -f /usr/local/share/ca-certificates/migros-root-ca.crt; command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; fi; if [ "$KEEP_APT" = "0" ]; then apt-get remove -y procps curl || true; apt-get autoremove -y; apt-get clean; apt-get remove --purge -y --allow-remove-essential apt || true; rm -rf /tmp/npm-cache /root/.npm /root/.cache; rm -rf /usr/share/doc/* /usr/share/man/* /usr/share/info/* /usr/share/locale/*; rm -rf /var/lib/apt/lists/*; rm -rf /var/cache/apt/apt-file/; rm -f /usr/local/bin/node /usr/local/bin/nodejs /usr/local/bin/npm /usr/local/bin/npx /usr/local/bin/yarn /usr/local/bin/yarnpkg; rm -rf /usr/local/lib/node_modules/npm/bin/npm-cli.js /usr/local/lib/node_modules/npm/bin/npx-cli.js; rm -rf /opt/yarn-v1.22.22; fi'
# Inherit /opt/aifo/bin PATH from base

# --- Aider slim builder stage ---
FROM base-slim AS aider-builder-slim
RUN --mount=type=secret,id=migros_root_ca,target=/run/secrets/migros_root_ca,required=false sh -lc 'set -e; if [ -f /run/secrets/migros_root_ca ]; then install -m 0644 /run/secrets/migros_root_ca /usr/local/share/ca-certificates/migros-root-ca.crt || true; command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; fi; apt-get update && apt-get -o APT::Keep-Downloaded-Packages=false install -y --no-install-recommends python3 python3-venv python3-pip build-essential pkg-config libssl-dev; rm -rf /var/lib/apt/lists/* /usr/share/doc/* /usr/share/man/* /usr/share/info/* /usr/share/locale/*; if [ -f /usr/local/share/ca-certificates/migros-root-ca.crt ]; then rm -f /usr/local/share/ca-certificates/migros-root-ca.crt; command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; fi'
# Python: Aider via uv (PEP 668-safe)
ARG WITH_PLAYWRIGHT=1
ARG KEEP_APT=0
RUN --mount=type=secret,id=migros_root_ca,target=/run/secrets/migros_root_ca,required=false sh -lc 'set -e; \
    CAF=/run/secrets/migros_root_ca; \
    if [ -f "$CAF" ]; then \
        install -m 0644 "$CAF" /usr/local/share/ca-certificates/migros-root-ca.crt || true; \
        command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; \
        export CURL_CA_BUNDLE=/etc/ssl/certs/ca-certificates.crt; \
        export REQUESTS_CA_BUNDLE=/etc/ssl/certs/ca-certificates.crt; \
        export SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt; \
        export UV_NATIVE_TLS=1; \
    fi; \
    if command -v curl >/dev/null 2>&1; then \
        curl -LsSf https://astral.sh/uv/install.sh -o /tmp/uv.sh; \
    else \
        python3 -c "import urllib.request; open(\"/tmp/uv.sh\",\"wb\").write(urllib.request.urlopen(\"https://astral.sh/uv/install.sh\").read())"; \
    fi; \
    sh /tmp/uv.sh; \
    mv /root/.local/bin/uv /usr/local/bin/uv; \
    uv venv /opt/venv; \
    uv pip install --native-tls --python /opt/venv/bin/python --upgrade pip; \
    uv pip install --native-tls --python /opt/venv/bin/python aider-chat; \
    if [ "$WITH_PLAYWRIGHT" = "1" ]; then \
        uv pip install --native-tls --python /opt/venv/bin/python "aider-chat[playwright]"; \
        uv pip install --native-tls --python /opt/venv/bin/python playwright; \
        /opt/venv/bin/python -c "import playwright" >/dev/null 2>&1 || { echo "error: playwright module missing in venv" >&2; exit 3; }; \
    fi; \
    find /opt/venv -name "pycache" -type d -exec rm -rf {} +; find /opt/venv -name "*.pyc" -delete; \
    rm -rf /root/.cache/uv /root/.cache/pip; \
    if [ -f /usr/local/share/ca-certificates/migros-root-ca.crt ]; then \
        rm -f /usr/local/share/ca-certificates/migros-root-ca.crt; \
        command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; \
    fi; \
    if [ "$KEEP_APT" = "0" ]; then \
        apt-get remove -y procps || true; \
        apt-get autoremove -y; \
        apt-get clean; \
        apt-get remove --purge -y --allow-remove-essential apt || true; \
        rm -rf /tmp/npm-cache /root/.npm /root/.cache; \
        rm -rf /usr/share/doc/* /usr/share/man/* /usr/share/info/* /usr/share/locale/*; \
        rm -rf /var/lib/apt/lists/*; \
        rm -rf /var/cache/apt/apt-file/; \
        rm -f /usr/local/bin/node /usr/local/bin/nodejs /usr/local/bin/npm /usr/local/bin/npx /usr/local/bin/yarn /usr/local/bin/yarnpkg; \
        rm -rf /usr/local/lib/node_modules/npm/bin/npm-cli.js /usr/local/lib/node_modules/npm/bin/npx-cli.js; \
        rm -rf /opt/yarn-v1.22.22; \
    fi'

# --- Aider slim runtime stage ---
FROM base-slim AS aider-slim
RUN --mount=type=secret,id=migros_root_ca,target=/run/secrets/migros_root_ca,required=false sh -lc 'set -e; if [ -f /run/secrets/migros_root_ca ]; then install -m 0644 /run/secrets/migros_root_ca /usr/local/share/ca-certificates/migros-root-ca.crt || true; command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; fi; apt-get update && apt-get -o APT::Keep-Downloaded-Packages=false install -y --no-install-recommends python3; rm -rf /var/lib/apt/lists/*; if [ -f /usr/local/share/ca-certificates/migros-root-ca.crt ]; then rm -f /usr/local/share/ca-certificates/migros-root-ca.crt; command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; fi'
COPY --from=aider-builder-slim /opt/venv /opt/venv
ENV PATH="/opt/venv/bin:${PATH}"
# Inherit /opt/aifo/bin PATH from base
ENV PLAYWRIGHT_BROWSERS_PATH="/ms-playwright"
ARG WITH_PLAYWRIGHT=1
ARG KEEP_APT=0
ENV KEEP_APT=${KEEP_APT}
RUN --mount=type=secret,id=migros_root_ca,target=/run/secrets/migros_root_ca,required=false sh -lc 'set -e; \
        if [ "$WITH_PLAYWRIGHT" = "1" ]; then \
            CAF=/run/secrets/migros_root_ca; \
            if [ -f "$CAF" ]; then \
                install -m 0644 "$CAF" /usr/local/share/ca-certificates/migros-root-ca.crt || true; \
                command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; \
                export NODE_EXTRA_CA_CERTS="$CAF"; \
                export NODE_OPTIONS="${NODE_OPTIONS:+$NODE_OPTIONS }--use-openssl-ca"; \
                export SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt; \
            fi; \
            /opt/venv/bin/python -m playwright install --with-deps chromium; \
            rm -rf /root/.cache; \
            if [ -f /usr/local/share/ca-certificates/migros-root-ca.crt ]; then \
                rm -f /usr/local/share/ca-certificates/migros-root-ca.crt; \
                command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; \
            fi; \
        fi; \
        if [ "$KEEP_APT" = "0" ]; then \
                apt-get remove -y procps curl || true; \
                apt-get autoremove -y; \
                apt-get clean; \
                apt-get remove --purge -y --allow-remove-essential apt || true; \
                rm -rf /tmp/npm-cache /root/.npm /root/.cache; \
                rm -rf /usr/share/doc/* /usr/share/man/* /usr/share/info/* /usr/share/locale/*; \
                rm -rf /var/lib/apt/lists/*; \
                rm -rf /var/cache/apt/apt-file/; \
                rm -f /usr/local/bin/node /usr/local/bin/nodejs /usr/local/bin/npm /usr/local/bin/npx /usr/local/bin/yarn /usr/local/bin/yarnpkg; \
                rm -rf /usr/local/lib/node_modules/npm/bin/npm-cli.js /usr/local/lib/node_modules/npm/bin/npx-cli.js; \
                rm -rf /opt/yarn-v1.22.22; \
        fi'
# Cleanup merged into Playwright install RUN above (conditional via KEEP_APT)

# --- OpenHands slim image (uv tool install; shims-first PATH) ---
FROM base-slim AS openhands-slim
ARG OPENHANDS_CONSTRAINT=""
ARG KEEP_APT=0
ENV KEEP_APT=${KEEP_APT}
RUN --mount=type=secret,id=migros_root_ca,target=/run/secrets/migros_root_ca,required=false sh -lc 'set -e; \
  CAF=/run/secrets/migros_root_ca; \
  if [ -f "$CAF" ]; then \
    install -m 0644 "$CAF" /usr/local/share/ca-certificates/migros-root-ca.crt || true; \
    command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; \
    export CURL_CA_BUNDLE=/etc/ssl/certs/ca-certificates.crt; \
    export REQUESTS_CA_BUNDLE=/etc/ssl/certs/ca-certificates.crt; \
    export SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt; \
    export SSL_CERT_DIR=/etc/ssl/certs; \
  fi; \
  export UV_NATIVE_TLS=1; \
  curl -LsSf https://astral.sh/uv/install.sh -o /tmp/uv.sh; \
  sh /tmp/uv.sh; \
  mv /root/.local/bin/uv /usr/local/bin/uv; \
  PKG="openhands-ai"; \
  if [ -n "$OPENHANDS_CONSTRAINT" ]; then PKG="openhands-ai==$OPENHANDS_CONSTRAINT"; fi; \
  install -d -m 0755 /opt/uv-home; \
  HOME=/opt/uv-home uv venv -p 3.12 /opt/venv-openhands; \
  HOME=/opt/uv-home uv pip install --native-tls --python /opt/venv-openhands/bin/python --upgrade pip; \
  HOME=/opt/uv-home uv pip install --native-tls --python /opt/venv-openhands/bin/python "$PKG"; \
  ln -sf /opt/venv-openhands/bin/openhands /usr/local/bin/openhands; \
  if [ ! -x /opt/venv-openhands/bin/openhands ]; then ls -la /opt/venv-openhands/bin; echo "error: missing openhands console script"; exit 3; fi; \
  if [ ! -x /usr/local/bin/openhands ]; then ls -la /usr/local/bin; echo "error: missing openhands wrapper"; exit 2; fi; \
  # Ensure non-root can traverse uv-managed Python under /opt/uv-home (shebang interpreter resolution)
  find /opt/uv-home/.local/share/uv/python -type d -exec chmod 0755 {} + 2>/dev/null || true; \
  find /opt/uv-home/.local/share/uv/python -type f -name "python*" -exec chmod 0755 {} + 2>/dev/null || true; \
  rm -rf /root/.cache/uv /root/.cache/pip; \
  if [ -f /usr/local/share/ca-certificates/migros-root-ca.crt ]; then \
    rm -f /usr/local/share/ca-certificates/migros-root-ca.crt; \
    command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; \
  fi; \
  if [ "$KEEP_APT" = "0" ]; then \
    apt-get remove -y procps curl || true; \
    apt-get autoremove -y; \
    apt-get clean; \
    apt-get remove --purge -y --allow-remove-essential apt || true; \
    npm prune --omit=dev || true; \
    npm cache clean --force; \
    rm -rf /root/.npm /root/.cache; \
    rm -rf /usr/share/doc/* /usr/share/man/* /usr/share/info/* /usr/share/locale/*; \
    rm -rf /var/lib/apt/lists/*; \
    rm -rf /var/cache/apt/apt-file/; \
  fi'
# Inherit /opt/aifo/bin PATH from base
# Cleanup merged into install RUN above (conditional via KEEP_APT)

# --- OpenCode slim image (npm install; shims-first PATH) ---
FROM base-slim AS opencode-slim
ARG OPCODE_VERSION=latest
ARG KEEP_APT=0
ENV KEEP_APT=${KEEP_APT}
RUN --mount=type=secret,id=migros_root_ca,target=/run/secrets/migros_root_ca,required=false sh -lc 'set -e; \
  CAF=/run/secrets/migros_root_ca; \
  if [ -f "$CAF" ]; then \
    install -m 0644 "$CAF" /usr/local/share/ca-certificates/migros-root-ca.crt || true; \
    command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; \
    export NODE_EXTRA_CA_CERTS="$CAF"; \
    export NODE_OPTIONS="${NODE_OPTIONS:+$NODE_OPTIONS }--use-openssl-ca"; \
    export SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt; \
    export SSL_CERT_DIR=/etc/ssl/certs; \
    export CURL_CA_BUNDLE=/etc/ssl/certs/ca-certificates.crt; \
  fi; \
  export NPM_CONFIG_CACHE=/tmp/npm-cache; \
  npm install -g --omit=dev --no-audit --no-fund --no-update-notifier --no-optional "opencode-ai@${OPCODE_VERSION}"; \
  rm -rf /tmp/npm-cache /root/.npm /root/.cache; \
  if [ -f /usr/local/share/ca-certificates/migros-root-ca.crt ]; then \
    rm -f /usr/local/share/ca-certificates/migros-root-ca.crt; \
    command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; \
  fi; \
  if [ "$KEEP_APT" = "0" ]; then \
    apt-get remove -y procps curl || true; \
    apt-get autoremove -y; \
    apt-get clean; \
    apt-get remove --purge -y --allow-remove-essential apt || true; \
    rm -rf /tmp/npm-cache /root/.npm /root/.cache; \
    rm -rf /usr/share/doc/* /usr/share/man/* /usr/share/info/* /usr/share/locale/*; \
    rm -rf /var/lib/apt/lists/*; \
    rm -rf /var/cache/apt/apt-file/; \
  fi'
# Inherit /opt/aifo/bin PATH from base
# Cleanup merged into install RUN above (conditional via KEEP_APT)

# --- Plandex slim image (copy binary; shims-first PATH) ---
FROM base-slim AS plandex-slim
COPY --from=plandex-builder /out/plandex /usr/local/bin/plandex
ARG KEEP_APT=0
RUN chmod 0755 /usr/local/bin/plandex; strip /usr/local/bin/plandex || true; \
    if [ "$KEEP_APT" = "0" ]; then \
      apt-get remove -y procps curl || true; \
      apt-get autoremove -y; \
      apt-get clean; \
      apt-get remove --purge -y --allow-remove-essential apt || true; \
      rm -rf /tmp/npm-cache /root/.npm /root/.cache; \
      rm -rf /usr/share/doc/* /usr/share/man/* /usr/share/info/* /usr/share/locale/*; \
      rm -rf /var/lib/apt/lists/*; \
      rm -rf /var/cache/apt/apt-file/; \
    fi
