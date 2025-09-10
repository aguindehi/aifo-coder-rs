# c-cpp toolchain sidecar image for aifo-coder toolchains
# Default tag: aifo-cpp-toolchain:latest
FROM debian:bookworm-slim

ENV DEBIAN_FRONTEND=noninteractive

# Base developer toolchain for C/C++
RUN apt-get update \
 && apt-get -y upgrade \
 && apt-get install -y --no-install-recommends \
    build-essential \
    clang \
    cmake \
    ninja-build \
    pkg-config \
    ccache \
    ca-certificates \
    git \
 && rm -rf /var/lib/apt/lists/*
# Trust enterprise root CA when provided via BuildKit secret (best-effort)
RUN --mount=type=secret,id=migros_root_ca,target=/run/secrets/migros_root_ca,required=false sh -lc 'set -e; if [ -f /run/secrets/migros_root_ca ]; then install -m 0644 /run/secrets/migros_root_ca /usr/local/share/ca-certificates/migros-root-ca.crt || true; command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; fi'
ENV CURL_CA_BUNDLE=/etc/ssl/certs/ca-certificates.crt

# Ensure traditional compiler entrypoints exist (hardlink cc/c++ to gcc/g++)
# Some build scripts expect 'cc'/'c++' explicitly.
RUN ln -f /usr/bin/gcc /usr/bin/cc && ln -f /usr/bin/g++ /usr/bin/c++

# Prepare a world-writable HOME for arbitrary uid:gid mappings
RUN mkdir -p /home/coder/.cache/ccache \
 && chmod -R 0777 /home/coder

ENV CCACHE_DIR=/home/coder/.cache/ccache

# Sleep by default; the toolexec proxy will docker exec into this container
CMD ["sleep", "infinity"]
