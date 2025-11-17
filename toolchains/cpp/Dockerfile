# c-cpp toolchain sidecar image for aifo-coder toolchains
# Default tag: aifo-coder-toolchain-cpp:latest
ARG REGISTRY_PREFIX
# CI builds use Kaniko --use-new-run; retain RUN --mount (secrets/cache). Avoid COPY --link/--chmod.
FROM ${REGISTRY_PREFIX}debian:bookworm-slim

ARG KEEP_APT

ENV DEBIAN_FRONTEND=noninteractive

# Base developer toolchain for C/C++
RUN --mount=type=secret,id=migros_root_ca,target=/run/secrets/migros_root_ca,required=false --mount=type=cache,target=/var/cache/apt --mount=type=cache,target=/var/lib/apt/lists sh -lc 'set -e; if [ -f /run/secrets/migros_root_ca ]; then install -m 0644 /run/secrets/migros_root_ca /usr/local/share/ca-certificates/migros-root-ca.crt || true; command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; fi; apt-get update && apt-get -o APT::Keep-Downloaded-Packages=false install -y --no-install-recommends build-essential clang cmake ninja-build pkg-config ccache procps ca-certificates git; apt-mark manual cmake cmake-data ninja-build build-essential clang pkg-config ccache git ca-certificates >/dev/null 2>&1 || true; apt-mark hold cmake cmake-data >/dev/null 2>&1 || true; rm -rf /var/lib/apt/lists/* /usr/share/doc/* /usr/share/man/* /usr/share/info/* /usr/share/locale/*; if [ -f /usr/local/share/ca-certificates/migros-root-ca.crt ]; then rm -f /usr/local/share/ca-certificates/migros-root-ca.crt; command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; fi; /usr/bin/cmake --version >/dev/null'
# Enterprise CA is injected via BuildKit secret only during RUN steps; not persisted in the final image

# Consolidated: create cc/c++ hardlinks, add /usr/local/bin symlinks, and prepare HOME/ccache in one layer
RUN set -eux; \
    ln -f /usr/bin/gcc /usr/bin/cc; \
    ln -f /usr/bin/g++ /usr/bin/c++; \
    ln -sf /usr/bin/cmake /usr/local/bin/cmake; \
    ln -sf /usr/bin/ninja /usr/local/bin/ninja; \
    ln -sf /usr/bin/pkg-config /usr/local/bin/pkg-config; \
    mkdir -p /home/coder/.cache/ccache; \
    chmod -R 0777 /home/coder || true

ENV CCACHE_DIR=/home/coder/.cache/ccache

# Optionally drop procps and apt after verifying cmake; retain apt cache mounts for re-install if needed
RUN --mount=type=cache,target=/var/cache/apt --mount=type=cache,target=/var/lib/apt/lists sh -lc 'set -eux; \
  if [ "$KEEP_APT" = "0" ]; then \
    if [ ! -x /usr/bin/cmake ]; then \
      echo "warning: /usr/bin/cmake missing; re-installing cmake ..." >&2; \
      apt-get update; \
      apt-get -o APT::Keep-Downloaded-Packages=false install -y --no-install-recommends cmake cmake-data; \
      apt-mark manual cmake cmake-data >/dev/null 2>&1 || true; \
      apt-mark hold cmake cmake-data >/dev/null 2>&1 || true; \
      rm -rf /var/lib/apt/lists/*; \
    fi; \
    /usr/bin/cmake --version >/dev/null; \
    apt-get clean; \
    rm -rf /var/lib/apt/lists/* /usr/share/doc/* /usr/share/man/* /usr/share/info/* /usr/share/locale/*; \
    rm /usr/bin/apt /usr/bin/apt-cache /usr/bin/apt-cdrom /usr/bin/apt-config /usr/bin/apt-get; \
    rm /usr/bin/apt-key /usr/bin/apt-mark /bin/kill /bin/ps /sbin/sysctl /usr/bin/free /usr/bin/pgrep; \
    rm /usr/bin/pidwait /usr/bin/pmap /usr/bin/pwdx /usr/bin/skill; \
    rm /usr/bin/slabtop /usr/bin/tload /usr/bin/top /usr/bin/uptime /usr/bin/vmstat /usr/bin/w /usr/bin/watch; \
    rm /usr/bin/pkill /usr/bin/snice; \
  fi'

# Sleep by default; the toolexec proxy will docker exec into this container
CMD ["sleep", "infinity"]
