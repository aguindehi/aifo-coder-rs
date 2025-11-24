# AIFO Rust Toolchain image (Phase 0)
# - Preinstalls: clippy, rustfmt, rust-src, llvm-tools-preview
# - Cargo tools: cargo-nextest (installed with --locked)
# - System packages commonly needed by crates
# - Ensures CARGO_HOME=/home/coder/.cargo and PATH prepends $CARGO_HOME/bin, keeps /usr/local/cargo/bin
# - Note: CMake verification and system git configuration consolidated into base deps RUN to reduce layers

ARG RUST_TAG=1-slim-bookworm
ARG REGISTRY_PREFIX
# CI builds use Kaniko --use-new-run; retain RUN --mount (secrets/cache). Avoid COPY --link/--chmod.
FROM ${REGISTRY_PREFIX}rust:${RUST_TAG}

ARG KEEP_APT=0
ENV KEEP_APT=${KEEP_APT}

ENV DEBIAN_FRONTEND=noninteractive

# Base system dependencies (Debian/Bookworm)
RUN set -eux; \
    apt-get update; \
    apt-get -o APT::Keep-Downloaded-Packages=false install -y --no-install-recommends \
      build-essential \
      pkg-config \
      cmake \
      ninja-build \
      clang \
      libclang-dev \
      python3 \
      libssl-dev \
      zlib1g-dev \
      libsqlite3-dev \
      libcurl4-openssl-dev \
      git \
      git-lfs \
      ca-certificates \
      curl \
      procps \
      tzdata; \
    apt-mark manual build-essential pkg-config cmake cmake-data ninja-build clang libclang-dev python3 libssl-dev zlib1g-dev libsqlite3-dev libcurl4-openssl-dev git git-lfs ca-certificates curl tzdata >/dev/null 2>&1 || true; \
    apt-mark hold cmake cmake-data >/dev/null 2>&1 || true; \
    rm -rf /var/lib/apt/lists/* /usr/share/doc/* /usr/share/man/* /usr/share/info/* /usr/share/locale/*; \
    # Consolidation: verify CMake and configure system git in the same layer \
    /usr/bin/cmake --version >/dev/null; \
    git config --system user.email "aifo-toolchain@test.invalid"; \
    git config --system user.name "AIFO Toolchain"; \
    git config --system init.defaultBranch main; \
    git lfs install --system || true

# Optionally trust corporate CA via BuildKit secret and update system trust store
# Guardrail: This uses BuildKit RUN --mount=type=secret; CI relies on Kaniko --use-new-run to support this.
# Note: CMake verification and system git config consolidated into base deps RUN above
# Pass during build: --secret id=migros_root_ca,src=$HOME/.certificates/MigrosRootCA2.crt
RUN --mount=type=secret,id=migros_root_ca,required=false,target=/run/secrets/MigrosRootCA2.crt \
    if [ -s /run/secrets/MigrosRootCA2.crt ]; then \
      cp /run/secrets/MigrosRootCA2.crt /usr/local/share/ca-certificates/MigrosRootCA2.crt; \
      update-ca-certificates; \
    fi

# Locale
ENV LANG=C.UTF-8 \
    LC_ALL=C.UTF-8

# Home and Cargo configuration
ENV HOME=/home/coder \
    CARGO_HOME=/usr/local/cargo \
    RUSTUP_HOME=/usr/local/rustup

# Prepare writable home/cargo directories (best-effort for arbitrary uid:gid at runtime)
RUN set -eux; \
    mkdir -p /home/coder/.cargo/bin; \
    chmod -R 0777 /home/coder /home/coder/.cargo || true

# Ensure PATH exposes user and system cargo bins
ENV PATH="/home/coder/.cargo/bin:/usr/local/cargo/bin:${PATH}"
ARG NEXTEST_VERSION=0.9.114
ARG NEXTEST_FALLBACK_COMPILE=0
ARG BUILDPLATFORM
ARG TARGETPLATFORM
ARG TARGETARCH
ARG GRCOV_VERSION=0.10.5

# Preinstall rust components and cargo-nextest + grcov using prebuilt binaries; compile fallback only on native builds
RUN set -eux; \
    rustup --version; \
    rustc --version; \
    cargo --version; \
    rustup toolchain install stable --profile minimal; \
    rustup default stable; \
    rustup component add --toolchain stable rustfmt clippy rust-src llvm-tools-preview; \
    CROSS=0; \
    if [ -n "${BUILDPLATFORM:-}" ] && [ -n "${TARGETPLATFORM:-}" ] && [ "${BUILDPLATFORM}" != "${TARGETPLATFORM}" ]; then \
      CROSS=1; \
    fi; \
    # Determine prebuilt targets for nextest (gnu; musl fallback on amd64) and grcov (gnu; musl fallback) \
    if [ -n "${TARGETARCH:-}" ]; then \
      case "$TARGETARCH" in \
        amd64) tgt_nextest="x86_64-unknown-linux-gnu"; alt_nextest="x86_64-unknown-linux-musl"; tgt_grcov="x86_64-unknown-linux-gnu"; alt_grcov="x86_64-unknown-linux-musl" ;; \
        arm64) tgt_nextest="aarch64-unknown-linux-gnu"; alt_nextest=""; tgt_grcov="aarch64-unknown-linux-gnu"; alt_grcov="aarch64-unknown-linux-musl" ;; \
        *)     tgt_nextest=""; alt_nextest=""; tgt_grcov=""; alt_grcov="" ;; \
      esac; \
    else \
      arch="$(uname -m)"; \
      case "$arch" in \
        x86_64|amd64) tgt_nextest="x86_64-unknown-linux-gnu"; alt_nextest="x86_64-unknown-linux-musl"; tgt_grcov="x86_64-unknown-linux-gnu"; alt_grcov="x86_64-unknown-linux-musl" ;; \
        aarch64|arm64) tgt_nextest="aarch64-unknown-linux-gnu"; alt_nextest=""; tgt_grcov="aarch64-unknown-linux-gnu"; alt_grcov="aarch64-unknown-linux-musl" ;; \
        *)             tgt_nextest=""; alt_nextest=""; tgt_grcov=""; alt_grcov="" ;; \
      esac; \
    fi; \
    ok_nextest=0; \
    if [ "${NEXTEST_VERSION:-}" = "latest" ]; then \
      if curl -fsSL --retry 3 --connect-timeout 5 https://get.nexte.st/latest/linux -o /tmp/nextest.tgz; then \
        mkdir -p /tmp/nextest && tar -C /tmp/nextest -xzf /tmp/nextest.tgz; \
        bin="$(find /tmp/nextest -type f -name cargo-nextest -print -quit)"; \
        if [ -n "$bin" ]; then install -m 0755 "$bin" /usr/local/cargo/bin/cargo-nextest; fi; \
        rm -rf /tmp/nextest /tmp/nextest.tgz; \
      fi; \
    else \
      if [ -n "$tgt_nextest" ]; then \
        url="https://github.com/nextest-rs/nextest/releases/download/cargo-nextest-${NEXTEST_VERSION}/cargo-nextest-${NEXTEST_VERSION}-${tgt_nextest}.tar.gz"; \
        if curl -fsSL --retry 3 --connect-timeout 5 "$url" -o /tmp/nextest.tgz; then \
          mkdir -p /tmp/nextest && tar -C /tmp/nextest -xzf /tmp/nextest.tgz; \
          bin="$(find /tmp/nextest -type f -name cargo-nextest -print -quit)"; \
          [ -n "$bin" ] && install -m 0755 "$bin" /usr/local/cargo/bin/cargo-nextest; \
          rm -rf /tmp/nextest /tmp/nextest.tgz; \
        fi; \
        if ! /usr/local/cargo/bin/cargo-nextest -V >/dev/null 2>&1 && [ -n "$alt_nextest" ]; then \
          alt="https://github.com/nextest-rs/nextest/releases/download/cargo-nextest-${NEXTEST_VERSION}/cargo-nextest-${NEXTEST_VERSION}-${alt_nextest}.tar.gz"; \
          if curl -fsSL --retry 3 --connect-timeout 5 "$alt" -o /tmp/nextest.tgz; then \
            mkdir -p /tmp/nextest && tar -C /tmp/nextest -xzf /tmp/nextest.tgz; \
            bin="$(find /tmp/nextest -type f -name cargo-nextest -print -quit)"; \
            [ -n "$bin" ] && install -m 0755 "$bin" /usr/local/cargo/bin/cargo-nextest; \
            rm -rf /tmp/nextest /tmp/nextest.tgz; \
          fi; \
        fi; \
      fi; \
    fi; \
    /usr/local/cargo/bin/cargo-nextest -V >/dev/null 2>&1 && ok_nextest=1 || ok_nextest=0; \
    if [ "$ok_nextest" -ne 1 ]; then \
      echo "error: cargo-nextest not installed (prebuilt failed; compile fallback disabled or not possible)"; \
      exit 12; \
    fi; \
    ok_grcov=0; \
    if [ -n "$tgt_grcov" ]; then \
      g_url="https://github.com/mozilla/grcov/releases/download/v${GRCOV_VERSION}/grcov-${tgt_grcov}.tar.bz2"; \
      if curl -fsSL --retry 3 --connect-timeout 5 "$g_url" -o /tmp/grcov.tar.bz2; then \
        mkdir -p /tmp/grcov && tar -C /tmp/grcov -xjf /tmp/grcov.tar.bz2; \
        gbin="$(find /tmp/grcov -maxdepth 2 -type f -name grcov -print -quit)"; \
        [ -n "$gbin" ] && install -m 0755 "$gbin" /usr/local/cargo/bin/grcov; \
        rm -rf /tmp/grcov /tmp/grcov.tar.bz2; \
      fi; \
      /usr/local/cargo/bin/grcov --version >/dev/null 2>&1 || { \
        if [ -n "${alt_grcov:-}" ]; then \
          g_alt="https://github.com/mozilla/grcov/releases/download/v${GRCOV_VERSION}/grcov-${alt_grcov}.tar.bz2"; \
          if curl -fsSL --retry 3 --connect-timeout 5 "$g_alt" -o /tmp/grcov.tar.bz2; then \
            mkdir -p /tmp/grcov && tar -C /tmp/grcov -xjf /tmp/grcov.tar.bz2; \
            gbin="$(find /tmp/grcov -maxdepth 2 -type f -name grcov -print -quit)"; \
            [ -n "$gbin" ] && install -m 0755 "$gbin" /usr/local/cargo/bin/grcov; \
            rm -rf /tmp/grcov /tmp/grcov.tar.bz2; \
          fi; \
        fi; \
      }; \
    fi; \
    /usr/local/cargo/bin/grcov --version >/dev/null 2>&1 && ok_grcov=1 || ok_grcov=0; \
    if [ "$ok_grcov" -ne 1 ] && [ "$CROSS" -eq 0 ]; then \
      cargo install grcov --locked && ok_grcov=1 || ok_grcov=0; \
    fi; \
    if [ "$ok_grcov" -ne 1 ]; then \
      echo "error: grcov not installed (prebuilt failed; compile fallback disabled or not possible)"; \
      exit 13; \
    fi; \
    strip /usr/local/cargo/bin/cargo-nextest /usr/local/cargo/bin/grcov 2>/dev/null || true; \
    rm -rf /usr/local/cargo/registry /usr/local/cargo/git /usr/local/rustup/downloads /usr/local/rustup/tmp

# Optionally drop procps/apt and fold final CMake verification into same layer
RUN set -eux; \
  if [ "${KEEP_APT:-0}" = "0" ] ; then \
    # Ensure CMake present before removing apt; reinstall if missing (best-effort) \
    if [ ! -x /usr/bin/cmake ] ; then \
      apt-get update; \
      apt-get -o APT::Keep-Downloaded-Packages=false install -y --no-install-recommends cmake cmake-data; \
      apt-mark manual cmake cmake-data >/dev/null 2>&1 || true; \
      apt-mark hold cmake cmake-data   >/dev/null 2>&1 || true; \
    fi; \
    /usr/bin/cmake --version >/dev/null; \
    apt-get clean; \
    rm -rf /var/lib/apt/lists/* /usr/share/doc/* /usr/share/man/* /usr/share/info/* /usr/share/locale/*; \
    rm /usr/bin/apt /usr/bin/apt-cache /usr/bin/apt-cdrom /usr/bin/apt-config /usr/bin/apt-get; \
    rm /usr/bin/apt-key /usr/bin/apt-mark /bin/kill /bin/ps /sbin/sysctl /usr/bin/free /usr/bin/pgrep; \
    rm /usr/bin/pidwait /usr/bin/pmap /usr/bin/pwdx /usr/bin/skill; \
    rm /usr/bin/slabtop /usr/bin/tload /usr/bin/top /usr/bin/uptime /usr/bin/vmstat /usr/bin/w /usr/bin/watch; \
    rm /usr/bin/pkill /usr/bin/snice; \
  else \
    # Even when KEEP_APT=1, verify cmake remains available \
    /usr/bin/cmake --version >/dev/null; \
  fi

# Final CMake verification folded into the KEEP_APT cleanup RUN above

# Default workdir mirrors sidecar runtime
WORKDIR /workspace
