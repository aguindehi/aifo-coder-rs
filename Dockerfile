# Multi-stage Dockerfile for aifo-coder, producing one image per agent while
# sharing identical parent layers for maximum cache and storage reuse.

# Default working directory at /workspace: the host project will be mounted there

ARG REGISTRY_PREFIX

# --- Base layer: Rust image ---
FROM ${REGISTRY_PREFIX}rust:1-bookworm-slim AS rust-base
ENV DEBIAN_FRONTEND=noninteractive
RUN --mount=type=secret,id=migros_root_ca,target=/run/secrets/migros_root_ca,required=false sh -lc 'set -e; if [ -f /run/secrets/migros_root_ca ]; then install -m 0644 /run/secrets/migros_root_ca /usr/local/share/ca-certificates/migros-root-ca.crt || true; command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; fi; apt-get update; rm -rf /var/lib/apt/lists/*; if [ -f /usr/local/share/ca-certificates/migros-root-ca.crt ]; then rm -f /usr/local/share/ca-certificates/migros-root-ca.crt; command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; fi'
WORKDIR /workspace

# --- Rust target builder for Linux, Windows & macOS ---
FROM rust-base AS rust-builder
ARG WITH_WIN=0
WORKDIR /workspace
ENV DEBIAN_FRONTEND=noninteractive
ENV PATH="/usr/local/cargo/bin:${PATH}"
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
    apt-get update && apt-get -o APT::Keep-Downloaded-Packages=false install -y --no-install-recommends pkg-config git-lfs ca-certificates; \
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
# hadolint ignore=DL3059
RUN --mount=type=secret,id=migros_root_ca,target=/run/secrets/migros_root_ca,required=false --mount=type=cache,target=/usr/local/cargo/registry --mount=type=cache,target=/usr/local/cargo/git sh -lc 'set -e; \
    CAF=/run/secrets/migros_root_ca; \
    if [ -f "$CAF" ]; then \
        install -m 0644 "$CAF" /usr/local/share/ca-certificates/migros-root-ca.crt || true; \
        command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; \
        export SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt; \
        export CARGO_HTTP_CAINFO=/etc/ssl/certs/ca-certificates.crt; \
        export CURL_CA_BUNDLE=/etc/ssl/certs/ca-certificates.crt; \
    fi; \
    /usr/local/cargo/bin/cargo install cargo-nextest --locked; \
    /usr/local/cargo/bin/cargo install grcov --locked; \
    rm -rf /usr/local/cargo/registry /usr/local/cargo/git; \
    if [ -f /usr/local/share/ca-certificates/migros-root-ca.crt ]; then \
        rm -f /usr/local/share/ca-certificates/migros-root-ca.crt; \
        command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; \
    fi'

# Build the Rust aifo-shim binary for the current build platform
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
    rm -rf target /usr/local/cargo/registry /usr/local/cargo/git /usr/local/rustup/downloads /usr/local/rustup/tmp; \
    if [ -f /usr/local/share/ca-certificates/migros-root-ca.crt ]; then \
        rm -f /usr/local/share/ca-certificates/migros-root-ca.crt; \
        command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; \
    fi'

# --- macOS cross Rust builder (osxcross; no secrets) ---
FROM ${REGISTRY_PREFIX}rust:1-bookworm AS macos-cross-rust-builder
ENV DEBIAN_FRONTEND=noninteractive
# Minimal packages required to build osxcross and perform smoke checks
RUN apt-get update && apt-get install -y --no-install-recommends \
      clang llvm lld make cmake patch xz-utils unzip curl git python3 file ca-certificates \
      autoconf automake libtool pkg-config bison flex zlib1g-dev libxml2-dev libssl-dev \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /opt
# Filename of the Apple SDK tarball; CI places it under ci/osx/ before build (Phase 0)
ARG OSX_SDK_FILENAME=MacOSX.sdk.tar.xz
ARG OSXCROSS_REF
# Optional: pass the exact versioned tarball name (e.g., MacOSX13.3.sdk.tar.xz) for osxcross
ARG OSXCROSS_SDK_TARBALL
# Copy SDK from build context (decoded in CI) into osxcross tarballs
# Use a stable filename to avoid COPY src variable expansion issues in some builders (e.g., Kaniko)
COPY ci/osx/MacOSX.sdk.tar.xz /tmp/MacOSX.sdk.tar.xz
# Build osxcross unattended and install into /opt/osxcross
RUN set -e; \
    git clone --depth=1 https://github.com/tpoechtrager/osxcross.git osxcross; \
    if [ -n "${OSXCROSS_REF}" ]; then \
      cd osxcross && git fetch --depth=1 origin "${OSXCROSS_REF}" && git checkout FETCH_HEAD && cd ..; \
    fi; \
    SDK_TMP="/tmp/MacOSX.sdk.tar.xz"; \
    SDK_NAME="${OSXCROSS_SDK_TARBALL}"; \
    if [ -z "$SDK_NAME" ]; then \
      # Try to derive version from top-level directory inside the tarball: MacOSX<ver>.sdk/
      TOP="$( (tar -tf "$SDK_TMP" 2>/dev/null || xz -dc "$SDK_TMP" 2>/dev/null | tar -tf - 2>/dev/null) | head -n1 || true)"; \
      VER="$(printf '%s\n' "$TOP" | sed -n -E 's#^(\./)?MacOSX([0-9][0-9.]*)\.sdk(/.*)?$#\2#p' | tr -d ' \t\r\n')"; \
      if [ -n "$VER" ]; then SDK_NAME="MacOSX${VER}.sdk.tar.xz"; fi; \
    fi; \
    if [ -z "$SDK_NAME" ]; then \
      echo "warning: could not derive SDK version from ${OSX_SDK_FILENAME}; using original name (osxcross may reject it)"; \
      SDK_NAME="${OSX_SDK_FILENAME}"; \
    fi; \
    mkdir -p osxcross/tarballs; \
    mv "$SDK_TMP" "osxcross/tarballs/${SDK_NAME}"; \
    UNATTENDED=1 osxcross/build.sh; \
    mkdir -p /opt/osxcross/SDK; \
    printf '%s\n' "${SDK_NAME}" > /opt/osxcross/SDK/SDK_NAME.txt || true; \
    SDK_DIR="$(ls -d /opt/osxcross/target/SDK/MacOSX*.sdk 2>/dev/null | head -n1)"; \
    [ -n "$SDK_DIR" ] && printf '%s\n' "$SDK_DIR" > /opt/osxcross/SDK/SDK_DIR.txt || true
# Create stable tool aliases to avoid depending on Darwin minor suffixes
RUN set -e; cd /opt/osxcross/target/bin; \
    for t in ar ranlib strip; do \
      ln -sf "$(ls aarch64-apple-darwin*-$t 2>/dev/null | head -n1)" aarch64-apple-darwin-$t || true; \
      ln -sf "$(ls x86_64-apple-darwin*-$t 2>/dev/null | head -n1)"  x86_64-apple-darwin-$t  || true; \
    done; \
    # Create clang wrappers (always overwrite to ensure presence)
    printf '%s\n' '#!/bin/sh' \
      'SDK="$(cat /opt/osxcross/SDK/SDK_DIR.txt 2>/dev/null || ls -d /opt/osxcross/target/SDK/MacOSX*.sdk 2>/dev/null | head -n1)"' \
      'exec clang -target aarch64-apple-darwin --sysroot="$SDK" -B/opt/osxcross/target/bin "$@"' > oa64-clang; \
    chmod 0755 oa64-clang; \
    printf '%s\n' '#!/bin/sh' \
      'SDK="$(cat /opt/osxcross/SDK/SDK_DIR.txt 2>/dev/null || ls -d /opt/osxcross/target/SDK/MacOSX*.sdk 2>/dev/null | head -n1)"' \
      'exec clang -target x86_64-apple-darwin --sysroot="$SDK" -B/opt/osxcross/target/bin "$@"' > o64-clang; \
    chmod 0755 o64-clang; \
    # Provide a robust Mach-O-aware linker wrapper as 'ld' (always overwrite)
    printf '%s\n' '#!/bin/sh' \
      'set -e' \
      'SDK_DIR="$(cat /opt/osxcross/SDK/SDK_DIR.txt 2>/dev/null || true)"' \
      'HAVE_PV=0; OS_MIN="";' \
      'for a in "$@"; do' \
      '  case "$a" in' \
      '    -platform_version) HAVE_PV=1 ;;' \
      '    -mmacosx-version-min=*) OS_MIN="${a#-mmacosx-version-min=}" ;;' \
      '  esac' \
      'done' \
      'if [ "$HAVE_PV" -eq 0 ]; then' \
      '  [ -n "$OS_MIN" ] || OS_MIN="${MACOSX_DEPLOYMENT_TARGET:-11.0}"' \
      '  case "$OS_MIN" in *.*.*) : ;; *.*) OS_MIN="$OS_MIN.0" ;; *) OS_MIN="$OS_MIN.0.0" ;; esac' \
      '  SDK_VER=""' \
      '  if [ -n "$SDK_DIR" ]; then' \
      '    base="${SDK_DIR%/}"; base="${base##*/}";' \
      '    case "$base" in MacOSX*) SDK_VER="${base#MacOSX}"; SDK_VER="${SDK_VER%.sdk}";; esac' \
      '  fi' \
      '  [ -n "$SDK_VER" ] || SDK_VER="$OS_MIN"' \
      '  case "$SDK_VER" in *.*.*) : ;; *.*) SDK_VER="$SDK_VER.0" ;; *) SDK_VER="$SDK_VER.0.0" ;; esac' \
      '  set -- -platform_version macos "$OS_MIN" "$SDK_VER" "$@"' \
      'fi' \
      'HAVE_SR=0' \
      'for a in "$@"; do [ "$a" = "-syslibroot" ] && HAVE_SR=1 && break; done' \
      'if [ "$HAVE_SR" -eq 0 ] && [ -n "$SDK_DIR" ]; then set -- -syslibroot "$SDK_DIR" "$@"; fi' \
      'if [ -x "/opt/osxcross/target/bin/ld64" ]; then exec /opt/osxcross/target/bin/ld64 "$@"; fi' \
      'if command -v ld64.lld >/dev/null 2>&1; then exec "$(command -v ld64.lld)" "$@"; fi' \
      'if command -v ld.lld   >/dev/null 2>&1; then exec "$(command -v ld.lld)" -flavor darwin "$@"; fi' \
      'echo "error: Mach-O ld not found (need cctools ld64 or ld64.lld)" >&2; exit 127' \
      > ld; \
    chmod 0755 ld
# Environment for cargo/rustup and macOS arm64 cross-compilation (optional x86_64 below)
# Include /usr/local/cargo/bin explicitly because using ${PATH} here expands at build-time and can drop Rust's PATH.
ENV RUSTUP_HOME="/usr/local/rustup" \
    CARGO_HOME="/usr/local/cargo" \
    PATH="/opt/osxcross/target/bin:/usr/local/cargo/bin:/usr/local/rustup/bin:${PATH}" \
    MACOSX_DEPLOYMENT_TARGET=11.0 \
    CC_aarch64_apple_darwin=oa64-clang \
    CXX_aarch64_apple_darwin=oa64-clang++ \
    AR_aarch64_apple_darwin=aarch64-apple-darwin-ar \
    RANLIB_aarch64_apple_darwin=aarch64-apple-darwin-ranlib \
    CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER=/opt/osxcross/target/bin/oa64-clang
# Enable optional x86_64 macOS cross-compilation as well
ENV CC_x86_64_apple_darwin=o64-clang \
    CXX_x86_64_apple_darwin=o64-clang++ \
    AR_x86_64_apple_darwin=x86_64-apple-darwin-ar \
    RANLIB_x86_64_apple_darwin=x86_64-apple-darwin-ranlib \
    CARGO_TARGET_X86_64_APPLE_DARWIN_LINKER=/opt/osxcross/target/bin/o64-clang
# Install Rust target inside the image (best-effort)
RUN /usr/local/cargo/bin/rustup target add aarch64-apple-darwin x86_64-apple-darwin || true

# Preinstall nextest to speed up CI test startup (best effort; keep image lean)
RUN /usr/local/cargo/bin/cargo install cargo-nextest --locked || true; \
    rm -rf /usr/local/cargo/registry /usr/local/cargo/git

# --- Base layer: Node image + common OS tools used by all agents ---
FROM ${REGISTRY_PREFIX}node:22-bookworm-slim AS base
ENV DEBIAN_FRONTEND=noninteractive
RUN --mount=type=secret,id=migros_root_ca,target=/run/secrets/migros_root_ca,required=false sh -lc 'set -e; if [ -f /run/secrets/migros_root_ca ]; then install -m 0644 /run/secrets/migros_root_ca /usr/local/share/ca-certificates/migros-root-ca.crt || true; command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; fi; apt-get update && apt-get -o APT::Keep-Downloaded-Packages=false install -y --no-install-recommends git gnupg pinentry-curses ca-certificates curl ripgrep dumb-init procps emacs-nox vim nano mg nvi libnss-wrapper file; rm -rf /var/lib/apt/lists/*; if [ -f /usr/local/share/ca-certificates/migros-root-ca.crt ]; then rm -f /usr/local/share/ca-certificates/migros-root-ca.crt; command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; fi'
WORKDIR /workspace

# embed compiled Rust PATH shim into agent images, but do not yet add to PATH
RUN install -d -m 0755 /opt/aifo/bin
# Install compiled Rust aifo-shim and shell wrappers for sh/bash/dash
COPY --from=rust-builder /workspace/out/aifo-shim /opt/aifo/bin/aifo-shim
# hadolint ignore=SC2016,SC2026
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
  '# When invoked as sh -c/-lc "cmd", append ; exit so the shell terminates after running the command.' \
  'if [ "$#" -ge 2 ] && { [ "$1" = "-c" ] || [ "$1" = "-lc" ]; }; then' \
  '  flag="$1"' \
  '  cmd="$2"' \
  '  shift 2' \
  '  exec /bin/sh "$flag" "$cmd; exit" "$@"' \
  'fi' \
  '' \
  'exec /bin/sh "$@"' \
  > /opt/aifo/bin/sh && chmod 0755 /opt/aifo/bin/sh && \
  sed 's#/bin/sh#/bin/bash#g' /opt/aifo/bin/sh > /opt/aifo/bin/bash && chmod 0755 /opt/aifo/bin/bash && \
  sed 's#/bin/sh#/bin/dash#g' /opt/aifo/bin/sh > /opt/aifo/bin/dash && chmod 0755 /opt/aifo/bin/dash && \
  for t in cargo rustc node npm npx yarn pnpm deno tsc ts-node python pip pip3 gcc g++ cc c++ clang clang++ make cmake ninja pkg-config go gofmt say; do ln -sf aifo-shim "/opt/aifo/bin/$t"; done
# will get added by the top layer
#ENV PATH="/opt/aifo/bin:${PATH}"

# Install a tiny entrypoint to prep GnuPG runtime and launch gpg-agent if available
# hadolint ignore=SC2016,SC2145
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
RUN --mount=type=secret,id=migros_root_ca,target=/run/secrets/migros_root_ca,required=false sh -lc 'set -e; CAF=/run/secrets/migros_root_ca; if [ -f "$CAF" ]; then install -m 0644 "$CAF" /usr/local/share/ca-certificates/migros-root-ca.crt || true; command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; export NODE_EXTRA_CA_CERTS="$CAF"; export NODE_OPTIONS="${NODE_OPTIONS:+$NODE_OPTIONS }--use-openssl-ca"; export SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt; fi; npm install -g --omit=dev --no-audit --no-fund --no-update-notifier --no-optional @openai/codex; if [ -f /usr/local/share/ca-certificates/migros-root-ca.crt ]; then rm -f /usr/local/share/ca-certificates/migros-root-ca.crt; command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; fi'
ENV PATH="/opt/aifo/bin:${PATH}"
ARG KEEP_APT=0
# Optionally drop apt/procps from final image to reduce footprint
# hadolint ignore=SC2026
RUN if [ "$KEEP_APT" = "0" ]; then \
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
    rm -f /usr/local/bin/npm /usr/local/bin/npx /usr/local/bin/yarn /usr/local/bin/yarnpkg; \
    rm -rf /usr/local/lib/node_modules/npm/bin/npm-cli.js /usr/local/lib/node_modules/npm/bin/npx-cli.js; \
    rm -rf /opt/yarn-v1.22.22; \
  fi

# --- Crush image (adds only Crush CLI on top of base) ---
FROM base AS crush
# Crush docs: npm i -g @charmland/crush
RUN --mount=type=secret,id=migros_root_ca,target=/run/secrets/migros_root_ca,required=false sh -lc 'set -e; CAF=/run/secrets/migros_root_ca; if [ -f "$CAF" ]; then install -m 0644 "$CAF" /usr/local/share/ca-certificates/migros-root-ca.crt || true; command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; export NODE_EXTRA_CA_CERTS="$CAF"; export NODE_OPTIONS="${NODE_OPTIONS:+$NODE_OPTIONS }--use-openssl-ca"; export SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt; fi; npm install -g --omit=dev --no-audit --no-fund --no-update-notifier --no-optional @charmland/crush; if [ -f /usr/local/share/ca-certificates/migros-root-ca.crt ]; then rm -f /usr/local/share/ca-certificates/migros-root-ca.crt; command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; fi'
ENV PATH="/opt/aifo/bin:${PATH}"
ARG KEEP_APT=0
# Optionally drop apt/procps from final image to reduce footprint
# hadolint ignore=SC2026
RUN if [ "$KEEP_APT" = "0" ]; then \
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
    rm -f /usr/local/bin/npm /usr/local/bin/npx /usr/local/bin/yarn /usr/local/bin/yarnpkg; \
    rm -rf /usr/local/lib/node_modules/npm/bin/npm-cli.js /usr/local/lib/node_modules/npm/bin/npx-cli.js; \
    rm -rf /opt/yarn-v1.22.22; \
  fi

# --- Aider builder stage (with build tools, not shipped in final) ---
FROM base AS aider-builder
RUN --mount=type=secret,id=migros_root_ca,target=/run/secrets/migros_root_ca,required=false sh -lc 'set -e; if [ -f /run/secrets/migros_root_ca ]; then install -m 0644 /run/secrets/migros_root_ca /usr/local/share/ca-certificates/migros-root-ca.crt || true; command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; fi; apt-get update && apt-get -o APT::Keep-Downloaded-Packages=false install -y --no-install-recommends python3 python3-venv python3-pip build-essential pkg-config libssl-dev; rm -rf /var/lib/apt/lists/*; if [ -f /usr/local/share/ca-certificates/migros-root-ca.crt ]; then rm -f /usr/local/share/ca-certificates/migros-root-ca.crt; command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; fi'
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
        python3 -c "import urllib.request; open('/tmp/uv.sh','wb').write(urllib.request.urlopen('https://astral.sh/uv/install.sh').read())"; \
    fi; \
    sh /tmp/uv.sh; \
    mv /root/.local/bin/uv /usr/local/bin/uv; \
    uv venv /opt/venv; \
    uv pip install --native-tls --python /opt/venv/bin/python --upgrade pip; \
    uv pip install --native-tls --python /opt/venv/bin/python aider-chat; \
    if [ "$WITH_PLAYWRIGHT" = "1" ]; then \
        uv pip install --native-tls --python /opt/venv/bin/python --upgrade aider-chat[playwright]; \
    fi; \
    find /opt/venv -name '\''pycache'\'' -type d -exec rm -rf {} +; find /opt/venv -name '\''*.pyc'\'' -delete; \
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
        rm -f /usr/local/bin/node /usr/local/bin/nodejs /usr/local/bin/npm /usr/local/bin/npx /usr/local/bin/yarn /usr/local/bin/yarnpkg; \
        rm -rf /usr/local/lib/node_modules/npm/bin/npm-cli.js /usr/local/lib/node_modules/npm/bin/npx-cli.js; \
        rm -rf /opt/yarn-v1.22.22; \
    fi'

# --- Aider runtime stage (no compilers; only Python runtime + venv) ---
FROM base AS aider
RUN --mount=type=secret,id=migros_root_ca,target=/run/secrets/migros_root_ca,required=false sh -lc 'set -e; if [ -f /run/secrets/migros_root_ca ]; then install -m 0644 /run/secrets/migros_root_ca /usr/local/share/ca-certificates/migros-root-ca.crt || true; command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; fi; apt-get update && apt-get -o APT::Keep-Downloaded-Packages=false install -y --no-install-recommends python3; rm -rf /var/lib/apt/lists/*; if [ -f /usr/local/share/ca-certificates/migros-root-ca.crt ]; then rm -f /usr/local/share/ca-certificates/migros-root-ca.crt; command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; fi'
COPY --from=aider-builder /opt/venv /opt/venv
ENV PATH="/opt/venv/bin:${PATH}"
ENV PATH="/opt/aifo/bin:${PATH}"
ENV PLAYWRIGHT_BROWSERS_PATH="/ms-playwright"
ARG WITH_PLAYWRIGHT=1
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
    fi'
ARG KEEP_APT=0
# Optionally drop apt/procps from final image to reduce footprint
# hadolint ignore=SC2026
RUN if [ "$KEEP_APT" = "0" ]; then \
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
        rm -f /usr/local/bin/node /usr/local/bin/nodejs /usr/local/bin/npm /usr/local/bin/npx /usr/local/bin/yarn /usr/local/bin/yarnpkg; \
        rm -rf /usr/local/lib/node_modules/npm/bin/npm-cli.js /usr/local/lib/node_modules/npm/bin/npx-cli.js; \
        rm -rf /opt/yarn-v1.22.22; \
    fi

# --- OpenHands image (uv tool install; shims-first PATH) ---
FROM base AS openhands
ARG OPENHANDS_CONSTRAINT=""
# hadolint ignore=SC2016,SC2145
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
  printf '%s\n' '#!/bin/sh' 'exec /opt/venv-openhands/bin/openhands "$@"' > /usr/local/bin/openhands; \
  chmod 0755 /usr/local/bin/openhands; \
  if [ ! -x /opt/venv-openhands/bin/openhands ]; then ls -la /opt/venv-openhands/bin; echo "error: missing openhands console script"; exit 3; fi; \
  if [ ! -x /usr/local/bin/openhands ]; then ls -la /usr/local/bin; echo "error: missing openhands wrapper"; exit 2; fi; \
  # Ensure non-root can traverse uv-managed Python under /opt/uv-home (shebang interpreter resolution)
  find /opt/uv-home/.local/share/uv/python -type d -exec chmod 0755 {} + 2>/dev/null || true; \
  find /opt/uv-home/.local/share/uv/python -type f -name "python*" -exec chmod 0755 {} + 2>/dev/null || true; \
  rm -rf /root/.cache/uv /root/.cache/pip; \
  if [ -f /usr/local/share/ca-certificates/migros-root-ca.crt ]; then \
    rm -f /usr/local/share/ca-certificates/migros-root-ca.crt; \
    command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; \
  fi'
ENV PATH="/opt/aifo/bin:${PATH}"
ARG KEEP_APT=0
# hadolint ignore=SC2026
RUN if [ "$KEEP_APT" = "0" ]; then \
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
  fi

# --- OpenCode image (npm install; shims-first PATH) ---
FROM base AS opencode
ARG OPCODE_VERSION=latest
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
  npm install -g --omit=dev --no-audit --no-fund --no-update-notifier --no-optional "opencode-ai@${OPCODE_VERSION}"; \
  if [ -f /usr/local/share/ca-certificates/migros-root-ca.crt ]; then \
    rm -f /usr/local/share/ca-certificates/migros-root-ca.crt; \
    command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; \
  fi'
ENV PATH="/opt/aifo/bin:${PATH}"
ARG KEEP_APT=0
RUN if [ "$KEEP_APT" = "0" ]; then \
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
  fi

# --- Plandex builder (Go) ---
FROM ${REGISTRY_PREFIX}golang:1.23-bookworm AS plandex-builder
ARG TARGETOS
ARG TARGETARCH
ARG PLX_GIT_REF=main
WORKDIR /src
ENV DEBIAN_FRONTEND=noninteractive
ENV PATH="/usr/local/go/bin:${PATH}"
ENV GOTOOLCHAIN=auto
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
  export GOFLAGS="-trimpath -mod=readonly"; \
  V="$([ -f version.txt ] && cat version.txt || echo dev)"; \
  LDFLAGS="-s -w -X plandex/version.Version=$V"; \
  case "${TARGETOS:-}" in "") GOOS="$(/usr/local/go/bin/go env GOOS)";; *) GOOS="$TARGETOS";; esac; \
  case "${TARGETARCH:-}" in "") GOARCH="$(/usr/local/go/bin/go env GOARCH)";; *) GOARCH="$TARGETARCH";; esac; \
  GOOS="$GOOS" GOARCH="$GOARCH" /usr/local/go/bin/go build -ldflags "$LDFLAGS" -o /out/plandex .; \
  rm -rf /root/go/pkg /go/pkg/mod; \
  if [ -f /usr/local/share/ca-certificates/migros-root-ca.crt ]; then \
    rm -f /usr/local/share/ca-certificates/migros-root-ca.crt; \
    command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; \
  fi'
# --- Plandex runtime (copy binary; shims-first PATH) ---
FROM base AS plandex
COPY --from=plandex-builder /out/plandex /usr/local/bin/plandex
RUN chmod 0755 /usr/local/bin/plandex
ENV PATH="/opt/aifo/bin:${PATH}"
ARG KEEP_APT=0
RUN if [ "$KEEP_APT" = "0" ]; then \
    apt-get remove -y procps || true; \
    apt-get autoremove -y; \
    apt-get clean; \
    apt-get remove --purge -y --allow-remove-essential apt || true; \
    npm prune --omit=dev || true; \
    npm cache clean --force; \
    rm -rf /root/.npm /root/.cache; \
    rm -rf /usr/share/doc/* /usr/share/man/* /usr/share/info/*; \
    rm -rf /usr/share/locale/*; \
    rm -rf /var/lib/apt/lists/*; \
    rm -rf /var/cache/apt/apt-file/; \
  fi

# --- Slim base (minimal tools, no editors/ripgrep) ---
FROM ${REGISTRY_PREFIX}node:22-bookworm-slim AS base-slim
ENV DEBIAN_FRONTEND=noninteractive
RUN --mount=type=secret,id=migros_root_ca,target=/run/secrets/migros_root_ca,required=false sh -lc 'set -e; if [ -f /run/secrets/migros_root_ca ]; then install -m 0644 /run/secrets/migros_root_ca /usr/local/share/ca-certificates/migros-root-ca.crt || true; command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; fi; apt-get update && apt-get -o APT::Keep-Downloaded-Packages=false install -y --no-install-recommends git gnupg pinentry-curses ca-certificates curl dumb-init mg nvi libnss-wrapper file; rm -rf /var/lib/apt/lists/*; if [ -f /usr/local/share/ca-certificates/migros-root-ca.crt ]; then rm -f /usr/local/share/ca-certificates/migros-root-ca.crt; command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; fi'
WORKDIR /workspace

# embed compiled Rust PATH shim into slim images, but do not yet add to PATH
RUN install -d -m 0755 /opt/aifo/bin
# Install compiled Rust aifo-shim and shell wrappers for sh/bash/dash
COPY --from=rust-builder /workspace/out/aifo-shim /opt/aifo/bin/aifo-shim
# hadolint ignore=SC2016,SC2026
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
  '# When invoked as sh -c/-lc "cmd", append ; exit so the shell terminates after running the command.' \
  'if [ "$#" -ge 2 ] && { [ "$1" = "-c" ] || [ "$1" = "-lc" ]; }; then' \
  '  flag="$1"' \
  '  cmd="$2"' \
  '  shift 2' \
  '  exec /bin/sh "$flag" "$cmd; exit" "$@"' \
  'fi' \
  '' \
  'exec /bin/sh "$@"' \
  > /opt/aifo/bin/sh && chmod 0755 /opt/aifo/bin/sh && \
  sed 's#/bin/sh#/bin/bash#g' /opt/aifo/bin/sh > /opt/aifo/bin/bash && chmod 0755 /opt/aifo/bin/bash && \
  sed 's#/bin/sh#/bin/dash#g' /opt/aifo/bin/sh > /opt/aifo/bin/dash && chmod 0755 /opt/aifo/bin/dash && \
  for t in cargo rustc node npm npx yarn pnpm deno tsc ts-node python pip pip3 gcc g++ cc c++ clang clang++ make cmake ninja pkg-config go gofmt say; do ln -sf aifo-shim "/opt/aifo/bin/$t"; done
# will get added by the top layer
#ENV PATH="/opt/aifo/bin:${PATH}"

# Install a tiny entrypoint to prep GnuPG runtime and launch gpg-agent if available
# hadolint ignore=SC2016,SC2145
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
RUN --mount=type=secret,id=migros_root_ca,target=/run/secrets/migros_root_ca,required=false sh -lc 'set -e; CAF=/run/secrets/migros_root_ca; if [ -f "$CAF" ]; then install -m 0644 "$CAF" /usr/local/share/ca-certificates/migros-root-ca.crt || true; command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; export NODE_EXTRA_CA_CERTS="$CAF"; export NODE_OPTIONS="${NODE_OPTIONS:+$NODE_OPTIONS }--use-openssl-ca"; export SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt; fi; npm install -g --omit=dev --no-audit --no-fund --no-update-notifier --no-optional @openai/codex; if [ -f /usr/local/share/ca-certificates/migros-root-ca.crt ]; then rm -f /usr/local/share/ca-certificates/migros-root-ca.crt; command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; fi'
ENV PATH="/opt/aifo/bin:${PATH}"
ARG KEEP_APT=0
# Optionally drop apt/procps from final image to reduce footprint
RUN if [ "$KEEP_APT" = "0" ]; then \
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
    rm -f /usr/local/bin/node /usr/local/bin/nodejs /usr/local/bin/npm /usr/local/bin/npx /usr/local/bin/yarn /usr/local/bin/yarnpkg; \
    rm -rf /usr/local/lib/node_modules/npm/bin/npm-cli.js /usr/local/lib/node_modules/npm/bin/npx-cli.js; \
    rm -rf /opt/yarn-v1.22.22; \
  fi

# --- Crush slim image ---
FROM base-slim AS crush-slim
RUN --mount=type=secret,id=migros_root_ca,target=/run/secrets/migros_root_ca,required=false sh -lc 'set -e; CAF=/run/secrets/migros_root_ca; if [ -f "$CAF" ]; then install -m 0644 "$CAF" /usr/local/share/ca-certificates/migros-root-ca.crt || true; command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; export NODE_EXTRA_CA_CERTS="$CAF"; export NODE_OPTIONS="${NODE_OPTIONS:+$NODE_OPTIONS }--use-openssl-ca"; export SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt; fi; npm install -g --omit=dev --no-audit --no-fund --no-update-notifier --no-optional @charmland/crush; if [ -f /usr/local/share/ca-certificates/migros-root-ca.crt ]; then rm -f /usr/local/share/ca-certificates/migros-root-ca.crt; command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; fi'
ENV PATH="/opt/aifo/bin:${PATH}"
ARG KEEP_APT=0
# Optionally drop apt/procps from final image to reduce footprint
RUN if [ "$KEEP_APT" = "0" ]; then \
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
    rm -f /usr/local/bin/node /usr/local/bin/nodejs /usr/local/bin/npm /usr/local/bin/npx /usr/local/bin/yarn /usr/local/bin/yarnpkg; \
    rm -rf /usr/local/lib/node_modules/npm/bin/npm-cli.js /usr/local/lib/node_modules/npm/bin/npx-cli.js; \
    rm -rf /opt/yarn-v1.22.22; \
  fi

# --- Aider slim builder stage ---
FROM base-slim AS aider-builder-slim
RUN --mount=type=secret,id=migros_root_ca,target=/run/secrets/migros_root_ca,required=false sh -lc 'set -e; if [ -f /run/secrets/migros_root_ca ]; then install -m 0644 /run/secrets/migros_root_ca /usr/local/share/ca-certificates/migros-root-ca.crt || true; command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; fi; apt-get update && apt-get -o APT::Keep-Downloaded-Packages=false install -y --no-install-recommends python3 python3-venv python3-pip build-essential pkg-config libssl-dev; rm -rf /var/lib/apt/lists/*; if [ -f /usr/local/share/ca-certificates/migros-root-ca.crt ]; then rm -f /usr/local/share/ca-certificates/migros-root-ca.crt; command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; fi'
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
        python3 -c "import urllib.request; open('/tmp/uv.sh','wb').write(urllib.request.urlopen('https://astral.sh/uv/install.sh').read())"; \
    fi; \
    sh /tmp/uv.sh; \
    mv /root/.local/bin/uv /usr/local/bin/uv; \
    uv venv /opt/venv; \
    uv pip install --native-tls --python /opt/venv/bin/python --upgrade pip; \
    uv pip install --native-tls --python /opt/venv/bin/python aider-chat; \
    if [ "$WITH_PLAYWRIGHT" = "1" ]; then \
        uv pip install --native-tls --python /opt/venv/bin/python --upgrade aider-chat[playwright]; \
    fi; \
    find /opt/venv -name '\''pycache'\'' -type d -exec rm -rf {} +; find /opt/venv -name '\''*.pyc'\'' -delete; \
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
        rm -f /usr/local/bin/node /usr/local/bin/nodejs /usr/local/bin/npm /usr/local/bin/npx /usr/local/bin/yarn /usr/local/bin/yarnpkg; \
        rm -rf /usr/local/lib/node_modules/npm/bin/npm-cli.js /usr/local/lib/node_modules/npm/bin/npx-cli.js; \
        rm -rf /opt/yarn-v1.22.22; \
    fi'

# --- Aider slim runtime stage ---
FROM base-slim AS aider-slim
RUN --mount=type=secret,id=migros_root_ca,target=/run/secrets/migros_root_ca,required=false sh -lc 'set -e; if [ -f /run/secrets/migros_root_ca ]; then install -m 0644 /run/secrets/migros_root_ca /usr/local/share/ca-certificates/migros-root-ca.crt || true; command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; fi; apt-get update && apt-get -o APT::Keep-Downloaded-Packages=false install -y --no-install-recommends python3; rm -rf /var/lib/apt/lists/*; if [ -f /usr/local/share/ca-certificates/migros-root-ca.crt ]; then rm -f /usr/local/share/ca-certificates/migros-root-ca.crt; command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; fi'
COPY --from=aider-builder-slim /opt/venv /opt/venv
ENV PATH="/opt/venv/bin:${PATH}"
ENV PATH="/opt/aifo/bin:${PATH}"
ENV PLAYWRIGHT_BROWSERS_PATH="/ms-playwright"
ARG WITH_PLAYWRIGHT=1
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
        fi'
ARG KEEP_APT=0
# Optionally drop apt/procps from final image to reduce footprint
RUN if [ "$KEEP_APT" = "0" ]; then \
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
        rm -f /usr/local/bin/node /usr/local/bin/nodejs /usr/local/bin/npm /usr/local/bin/npx /usr/local/bin/yarn /usr/local/bin/yarnpkg; \
        rm -rf /usr/local/lib/node_modules/npm/bin/npm-cli.js /usr/local/lib/node_modules/npm/bin/npx-cli.js; \
        rm -rf /opt/yarn-v1.22.22; \
    fi

# --- OpenHands slim image (uv tool install; shims-first PATH) ---
FROM base-slim AS openhands-slim
ARG OPENHANDS_CONSTRAINT=""
# hadolint ignore=SC2016,SC2145
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
  printf '%s\n' '#!/bin/sh' 'exec /opt/venv-openhands/bin/openhands "$@"' > /usr/local/bin/openhands; \
  chmod 0755 /usr/local/bin/openhands; \
  if [ ! -x /opt/venv-openhands/bin/openhands ]; then ls -la /opt/venv-openhands/bin; echo "error: missing openhands console script"; exit 3; fi; \
  if [ ! -x /usr/local/bin/openhands ]; then ls -la /usr/local/bin; echo "error: missing openhands wrapper"; exit 2; fi; \
  # Ensure non-root can traverse uv-managed Python under /opt/uv-home (shebang interpreter resolution)
  find /opt/uv-home/.local/share/uv/python -type d -exec chmod 0755 {} + 2>/dev/null || true; \
  find /opt/uv-home/.local/share/uv/python -type f -name "python*" -exec chmod 0755 {} + 2>/dev/null || true; \
  rm -rf /root/.cache/uv /root/.cache/pip; \
  if [ -f /usr/local/share/ca-certificates/migros-root-ca.crt ]; then \
    rm -f /usr/local/share/ca-certificates/migros-root-ca.crt; \
    command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; \
  fi'
ENV PATH="/opt/aifo/bin:${PATH}"
ARG KEEP_APT=0
RUN if [ "$KEEP_APT" = "0" ]; then \
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
  fi

# --- OpenCode slim image (npm install; shims-first PATH) ---
FROM base-slim AS opencode-slim
ARG OPCODE_VERSION=latest
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
  npm install -g --omit=dev --no-audit --no-fund --no-update-notifier --no-optional "opencode-ai@${OPCODE_VERSION}"; \
  if [ -f /usr/local/share/ca-certificates/migros-root-ca.crt ]; then \
    rm -f /usr/local/share/ca-certificates/migros-root-ca.crt; \
    command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; \
  fi'
ENV PATH="/opt/aifo/bin:${PATH}"
ARG KEEP_APT=0
RUN if [ "$KEEP_APT" = "0" ]; then \
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
    rm -f /usr/local/bin/node /usr/local/bin/nodejs /usr/local/bin/npm /usr/local/bin/npx /usr/local/bin/yarn /usr/local/bin/yarnpkg; \
    rm -rf /usr/local/lib/node_modules/npm/bin/npm-cli.js /usr/local/lib/node_modules/npm/bin/npx-cli.js; \
    rm -rf /opt/yarn-v1.22.22; \
  fi

# --- Plandex slim image (copy binary; shims-first PATH) ---
FROM base-slim AS plandex-slim
COPY --from=plandex-builder /out/plandex /usr/local/bin/plandex
RUN chmod 0755 /usr/local/bin/plandex
ENV PATH="/opt/aifo/bin:${PATH}"
ARG KEEP_APT=0
RUN if [ "$KEEP_APT" = "0" ]; then \
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
  fi
