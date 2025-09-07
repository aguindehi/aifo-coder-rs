# syntax=docker/dockerfile:1

# AIFO Rust Toolchain image (Phase 0)
# - Preinstalls: clippy, rustfmt, rust-src, llvm-tools-preview
# - Cargo tools: cargo-nextest (installed with --locked)
# - System packages commonly needed by crates
# - Ensures CARGO_HOME=/home/coder/.cargo and PATH prepends $CARGO_HOME/bin, keeps /usr/local/cargo/bin

ARG RUST_TAG=1.80-bookworm
FROM rust:${RUST_TAG}

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
      ca-certificates \
      curl \
      tzdata \
      locales; \
    rm -rf /var/lib/apt/lists/*

# Locale
ENV LANG=C.UTF-8 \
    LC_ALL=C.UTF-8

# Home and Cargo configuration
ENV HOME=/home/coder \
    CARGO_HOME=/home/coder/.cargo

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
    rustup component add clippy rustfmt rust-src llvm-tools-preview; \
    cargo install cargo-nextest --locked

# Default workdir mirrors sidecar runtime
WORKDIR /workspace
