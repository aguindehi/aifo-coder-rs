# macOS cross Rust builder (osxcross; standalone Dockerfile)
# This image is built separately to avoid coupling agent builds to the Apple SDK artifact.
ARG REGISTRY_PREFIX
FROM ${REGISTRY_PREFIX}rust:1-bookworm AS macos-cross-rust-builder
ENV DEBIAN_FRONTEND=noninteractive
# Minimal packages required to build osxcross and perform smoke checks
# hadolint ignore=DL3008
RUN apt-get update && apt-get install -y --no-install-recommends \
      clang llvm lld make cmake patch xz-utils unzip curl git python3 file ca-certificates sccache \
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
# hadolint ignore=DL4006
RUN set -e; \
    git clone --depth=1 https://github.com/tpoechtrager/osxcross.git osxcross; \
    if [ -n "${OSXCROSS_REF}" ]; then \
      git -C osxcross fetch --depth=1 origin "${OSXCROSS_REF}" && git -C osxcross checkout FETCH_HEAD; \
    fi; \
    SDK_TMP="/tmp/MacOSX.sdk.tar.xz"; \
    SDK_NAME="${OSXCROSS_SDK_TARBALL}"; \
    if [ -z "$SDK_NAME" ]; then \
      # Try to derive version from top-level directory inside the tarball: MacOSX<ver>.sdk/
      TOP="$( (tar -tf "$SDK_TMP" 2>/dev/null || xz -dc "$SDK_TMP" 2>/dev/null | tar -tf - 2>/dev/null) | head -n1 || true)"; \
      VER="$(printf '%s\n' "$TOP" | sed -n -E "s#^(\./)?MacOSX([0-9][0-9.]*)\.sdk(/.*)?$#\2#p" | tr -d " \t\r\n")"; \
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
    SDK_DIR="$(find /opt/osxcross/target/SDK -mindepth 1 -maxdepth 1 -type d -name 'MacOSX*.sdk' -print -quit 2>/dev/null)"; \
    [ -n "$SDK_DIR" ] && printf '%s\n' "$SDK_DIR" > /opt/osxcross/SDK/SDK_DIR.txt || true
# Create stable tool aliases to avoid depending on Darwin minor suffixes
WORKDIR /opt/osxcross/target/bin
# hadolint ignore=SC2016,SC2026
RUN set -e; \
    for t in ar ranlib strip; do \
      set -- aarch64-apple-darwin*-"$t"; [ -e "$1" ] && ln -sf "$1" aarch64-apple-darwin-"$t" || true; \
      set -- x86_64-apple-darwin*-"$t";  [ -e "$1" ] && ln -sf "$1" x86_64-apple-darwin-"$t"  || true; \
    done; \
    printf '%s\n' \
    '#!/bin/sh' \
    'SDK="$(cat /opt/osxcross/SDK/SDK_DIR.txt 2>/dev/null || find /opt/osxcross/target/SDK -mindepth 1 -maxdepth 1 -type d -name MacOSX\*.sdk -print -quit 2>/dev/null)"' \
    'exec clang -target aarch64-apple-darwin --sysroot="$SDK" -B/opt/osxcross/target/bin "$@"' \
    > oa64-clang; \
    chmod 0755 oa64-clang; \
    printf '%s\n' \
    '#!/bin/sh' \
    'SDK="$(cat /opt/osxcross/SDK/SDK_DIR.txt 2>/dev/null || find /opt/osxcross/target/SDK -mindepth 1 -maxdepth 1 -type d -name MacOSX\*.sdk -print -quit 2>/dev/null)"' \
    'exec clang -target x86_64-apple-darwin --sysroot="$SDK" -B/opt/osxcross/target/bin "$@"' \
    > o64-clang; \
    chmod 0755 o64-clang; \
    printf '%s\n' \
    '#!/bin/sh' \
    'set -e' \
    'SDK_DIR="$(cat /opt/osxcross/SDK/SDK_DIR.txt 2>/dev/null || true)"' \
    'HAVE_PV=0; OS_MIN="";' \
    'for a in "$@"; do' \
    '  case "$a" in' \
    '    -platform_version) HAVE_PV=1 ;;' \
    '    -mmacosx-version-min=*) OS_MIN="${a#-mmacosx-version-min=}" ;;' \
    '  esac' \
    'done' \
    'if [ "$HAVE_PV" -eq 0 ] then' \
    '  [ -n "$OS_MIN" ] || OS_MIN="${MACOSX_DEPLOYMENT_TARGET:-11.0}"' \
    '  case "$OS_MIN" in *.*.*) : ;; *.*) OS_MIN="$OS_MIN.0" ;; *) OS_MIN="$OS_MIN.0.0" ;; esac' \
    '  SDK_VER=""' \
    '  if [ -n "$SDK_DIR" ]; then' \
    '    base="${SDK_DIR%/}"; base="${base##*/}"' \
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
WORKDIR /
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
# Install Rust targets and preinstall nextest (with corp CA; caches) in one layer
RUN --mount=type=secret,id=migros_root_ca,target=/run/secrets/migros_root_ca,required=false \
    --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    sh -lc 'set -e; \
    CAF=/run/secrets/migros_root_ca; \
    if [ -f "$CAF" ]; then \
      install -m 0644 "$CAF" /usr/local/share/ca-certificates/migros-root-ca.crt || true; \
      command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; \
      export SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt; \
      export SSL_CERT_DIR=/etc/ssl/certs; \
      export CARGO_HTTP_CAINFO=/etc/ssl/certs/ca-certificates.crt; \
      export CURL_CA_BUNDLE=/etc/ssl/certs/ca-certificates.crt; \
      export RUSTUP_USE_CURL=1; \
    fi; \
    /usr/local/cargo/bin/rustup target add aarch64-apple-darwin x86_64-apple-darwin || true; \
    /usr/local/cargo/bin/cargo install cargo-nextest --locked; \
    strip /usr/local/cargo/bin/cargo-nextest 2>/dev/null || true; \
    find /usr/local/cargo/registry -mindepth 1 -maxdepth 1 -exec rm -rf {} + 2>/dev/null || true; \
    find /usr/local/cargo/git -mindepth 1 -maxdepth 1 -exec rm -rf {} + 2>/dev/null || true; \
    if [ -f /usr/local/share/ca-certificates/migros-root-ca.crt ]; then \
      rm -f /usr/local/share/ca-certificates/migros-root-ca.crt; \
      command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; \
    fi'
# Configure sccache wrapper for rustc and prepare cache directory
ENV RUSTC="/usr/local/cargo/bin/rustc" \
    RUSTC_WRAPPER="/usr/bin/sccache" \
    SCCACHE_DIR="/opt/sccache" \
    SCCACHE_CACHE_SIZE="2G"
RUN install -d -m 0755 /opt/sccache
