# c-cpp toolchain sidecar image for aifo-coder toolchains
# Default tag: aifo-coder-toolchain-cpp:latest
ARG REGISTRY_PREFIX
# CI builds use Kaniko --use-new-run; retain RUN --mount (secrets/cache). Avoid COPY --link/--chmod.
FROM ${REGISTRY_PREFIX}debian:bookworm-slim

ARG KEEP_APT=0
ARG HADOLINT_VERSION=2.12.0
ARG TARGETARCH
ENV KEEP_APT=${KEEP_APT}

ENV DEBIAN_FRONTEND=noninteractive

# Base developer toolchain for C/C++
# Guardrail: Uses BuildKit RUN --mount for secret/caches; CI relies on Kaniko --use-new-run to support these mounts.
# Best-effort CA handling: inject inside RUN and remove in the same RUN to avoid persisting CA in layers.
RUN --mount=type=secret,id=migros_root_ca,target=/run/secrets/migros_root_ca,required=false sh -lc 'set -e; \
    if [ -f /run/secrets/migros_root_ca ]; then \
      install -d -m 0755 /usr/local/share/ca-certificates || true; \
      install -m 0644 /run/secrets/migros_root_ca /usr/local/share/ca-certificates/migros-root-ca.crt || true; \
      command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; \
    fi; \
    # Ensure apt cache directories exist to avoid rename races \
    mkdir -p /var/cache/apt/archives /var/cache/apt/archives/partial; \
    rm -f /var/lib/apt/lists/lock /var/cache/apt/archives/lock /var/lib/dpkg/lock-frontend || true; \
    apt-get update -o Acquire::Retries=3; \
    apt-get -o Acquire::Retries=3 -o Dpkg::Use-Pty=0 -o APT::Keep-Downloaded-Packages=false install -y --no-install-recommends build-essential clang cmake ninja-build pkg-config ccache procps ca-certificates curl git; \
    apt-mark manual cmake cmake-data ninja-build build-essential clang pkg-config ccache git ca-certificates curl >/dev/null 2>&1 || true; \
    apt-mark hold cmake cmake-data >/dev/null 2>&1 || true; \
    rm -rf /var/lib/apt/lists/* /usr/share/doc/* /usr/share/man/* /usr/share/info/* /usr/share/locale/*; \
    if [ -f /usr/local/share/ca-certificates/migros-root-ca.crt ]; then \
      rm -f /usr/local/share/ca-certificates/migros-root-ca.crt; \
      command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; \
    fi; \
    /usr/bin/cmake --version >/dev/null'
# Enterprise CA is injected via BuildKit secret only during RUN steps; not persisted in the final image

# Install hadolint (multi-arch binary)
RUN set -eux; \
    arch="${TARGETARCH:-$(dpkg --print-architecture 2>/dev/null || uname -m)}"; \
    case "$arch" in \
      amd64 | x86_64) hl_arch="x86_64" ;; \
      arm64 | aarch64) hl_arch="arm64" ;; \
      *) echo "unsupported architecture for hadolint: $arch" >&2; exit 1 ;; \
    esac; \
    url="https://github.com/hadolint/hadolint/releases/download/v${HADOLINT_VERSION}/hadolint-Linux-${hl_arch}"; \
    curl -fsSL --retry 3 --connect-timeout 5 "$url" -o /usr/local/bin/hadolint; \
    chmod 0755 /usr/local/bin/hadolint; \
    strip /usr/local/bin/hadolint 2>/dev/null || true; \
    /usr/local/bin/hadolint --version >/dev/null

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
RUN sh -lc 'set -eux; \
  if [ "${KEEP_APT:-0}" = "0" ]; then \
    if [ ! -x /usr/bin/cmake ]; then \
      echo "warning: /usr/bin/cmake missing; re-installing cmake ..." >&2; \
      mkdir -p /var/cache/apt/archives /var/cache/apt/archives/partial; \
      rm -f /var/lib/apt/lists/lock /var/cache/apt/archives/lock /var/lib/dpkg/lock-frontend || true; \
      apt-get update -o Acquire::Retries=3; \
      apt-get -o Acquire::Retries=3 -o Dpkg::Use-Pty=0 -o APT::Keep-Downloaded-Packages=false install -y --no-install-recommends cmake cmake-data; \
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
