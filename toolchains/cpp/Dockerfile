# c-cpp toolchain sidecar image for aifo-coder toolchains
# Default tag: aifo-coder-toolchain-cpp:latest
ARG REGISTRY_PREFIX
FROM ${REGISTRY_PREFIX}debian:bookworm-slim

ARG KEEP_APT

ENV DEBIAN_FRONTEND=noninteractive

# Base developer toolchain for C/C++
RUN --mount=type=secret,id=migros_root_ca,target=/run/secrets/migros_root_ca,required=false sh -lc 'set -e; if [ -f /run/secrets/migros_root_ca ]; then install -m 0644 /run/secrets/migros_root_ca /usr/local/share/ca-certificates/migros-root-ca.crt || true; command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; fi; apt-get update && apt-get -y upgrade && apt-get install -y --no-install-recommends build-essential clang cmake ninja-build pkg-config ccache procps ca-certificates git; rm -rf /var/lib/apt/lists/*; if [ -f /usr/local/share/ca-certificates/migros-root-ca.crt ]; then rm -f /usr/local/share/ca-certificates/migros-root-ca.crt; command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; fi'
# Enterprise CA is injected via BuildKit secret only during RUN steps; not persisted in the final image
# Verify cmake is installed for tests and fail fast otherwise
RUN cmake --version >/dev/null

# Ensure traditional compiler entrypoints exist (hardlink cc/c++ to gcc/g++)
# Some build scripts expect 'cc'/'c++' explicitly.
RUN ln -f /usr/bin/gcc /usr/bin/cc && ln -f /usr/bin/g++ /usr/bin/c++
# Ensure common build tools are on /usr/local/bin for PATH precedence in some setups
RUN ln -sf /usr/bin/cmake /usr/local/bin/cmake && \
    ln -sf /usr/bin/ninja /usr/local/bin/ninja && \
    ln -sf /usr/bin/pkg-config /usr/local/bin/pkg-config

# Prepare a world-writable HOME for arbitrary uid:gid mappings
RUN mkdir -p /home/coder/.cache/ccache \
 && chmod -R 0777 /home/coder

ENV CCACHE_DIR=/home/coder/.cache/ccache

# Optionally drop procps from final image to reduce footprint
RUN if [ "$KEEP_APT" = "0" ]; then \
  apt-get remove -y procps || true; \
  apt-get autoremove -y; \
  apt-get clean; \
  rm -rf /var/lib/apt/lists/*; \
fi

# Sleep by default; the toolexec proxy will docker exec into this container
CMD ["sleep", "infinity"]
