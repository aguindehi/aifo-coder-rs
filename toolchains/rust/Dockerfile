# AIFO Rust Toolchain image (Phase 0)
# - Preinstalls: clippy, rustfmt, rust-src, llvm-tools-preview
# - Cargo tools: cargo-nextest (installed with --locked)
# - System packages commonly needed by crates
# - Ensures CARGO_HOME=/home/coder/.cargo and PATH prepends $CARGO_HOME/bin, keeps /usr/local/cargo/bin

ARG RUST_TAG=1-bookworm
ARG REGISTRY_PREFIX
FROM ${REGISTRY_PREFIX}rust:${RUST_TAG}

ENV DEBIAN_FRONTEND=noninteractive

# Base system dependencies (Debian/Bookworm)
RUN set -eux; \
    apt-get update; \
    apt-get install -y --no-install-recommends \
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
      docker.io \
      ca-certificates \
      curl \
      tzdata \
      locales; \
    rm -rf /var/lib/apt/lists/*

# Configure a default git identity and enable LFS filters system-wide for tests
RUN set -eux; \
    git config --system user.email "aifo-toolchain@test.invalid"; \
    git config --system user.name "AIFO Toolchain"; \
    git config --system init.defaultBranch main; \
    git lfs install --system || true

# Optionally trust corporate CA via BuildKit secret and update system trust store
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
    CARGO_HOME=/home/coder/.cargo \
    RUSTUP_HOME=/usr/local/rustup

# Prepare writable home/cargo directories (best-effort for arbitrary uid:gid at runtime)
RUN set -eux; \
    mkdir -p /home/coder/.cargo/bin; \
    chmod -R 0777 /home/coder /home/coder/.cargo || true

# Ensure PATH exposes cargo-installed tools first, retain /usr/local/cargo/bin as fallback
ENV PATH="${CARGO_HOME}/bin:/usr/local/cargo/bin:${PATH}"

# Preinstall rust components and cargo-nextest (idempotent on rebuild)
RUN set -eux; \
    rustup --version; \
    rustc --version; \
    cargo --version; \
    rustup toolchain install stable --profile minimal; \
    rustup default stable; \
    rustup component add --toolchain stable rustfmt clippy rust-src llvm-tools-preview; \
    cargo install cargo-nextest --locked

# Default workdir mirrors sidecar runtime
WORKDIR /workspace
