#!/usr/bin/env sh
# Developer helper: run cargo or generate SBOM inside a Docker container.
# Works even if cargo or make are not installed on the host.
set -e

IMAGE="${AIFO_DEV_IMAGE:-rust:1-bookworm}"
WORKDIR="/workspace"

# Detect available tooling on host
if command -v docker >/dev/null 2>&1; then HAVE_DOCKER=1; else HAVE_DOCKER=0; fi
if command -v cargo >/dev/null 2>&1; then HAVE_CARGO=1; else HAVE_CARGO=0; fi

run_in_container() {
  docker run --rm \
    -u "$(id -u):$(id -g)" \
    -e CARGO_HOME=/root/.cargo \
    -v "$PWD:${WORKDIR}" \
    -v "$HOME/.cargo:/root/.cargo" \
    -v "$PWD/target:${WORKDIR}/target" \
    -w "${WORKDIR}" \
    "${IMAGE}" "$@"
}

cmd="$1"; shift || true

case "$cmd" in
  test)
    # Run cargo tests
    if [ "$HAVE_DOCKER" -eq 1 ]; then
      run_in_container cargo test --all-targets "$@"
    elif [ "$HAVE_CARGO" -eq 1 ]; then
      cargo test --all-targets "$@"
    else
      echo "Neither Docker nor cargo are installed." >&2
      echo "Install Rust: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y" >&2
      echo "Or install Docker: https://docs.docker.com/engine/install/" >&2
      exit 127
    fi
    ;;
  cargo)
    # Pass-through arbitrary cargo commands
    if [ $# -eq 0 ]; then
      echo "Usage: $0 cargo <args...>" >&2
      exit 1
    fi
    if [ "$HAVE_DOCKER" -eq 1 ]; then
      run_in_container cargo "$@"
    elif [ "$HAVE_CARGO" -eq 1 ]; then
      cargo "$@"
    else
      echo "Neither Docker nor cargo are installed." >&2
      echo "Install Rust: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y" >&2
      echo "Or install Docker: https://docs.docker.com/engine/install/" >&2
      exit 127
    fi
    ;;
  sbom)
    # Generate CycloneDX SBOM at dist/SBOM.cdx.json
    mkdir -p dist
    if [ "$HAVE_DOCKER" -eq 1 ]; then
      # Install cargo-cyclonedx if missing, then generate SBOM inside container
      run_in_container /bin/sh -lc '
        set -e
        if ! command -v cargo >/dev/null 2>&1; then
          echo "cargo not found in container" >&2
          exit 127
        fi
        if ! cargo cyclonedx -h >/dev/null 2>&1; then
          echo "Installing cargo-cyclonedx ..." >&2
          cargo install cargo-cyclonedx
        fi
        cargo cyclonedx -o dist/SBOM.cdx.json
      '
      echo "Wrote dist/SBOM.cdx.json"
    elif [ "$HAVE_CARGO" -eq 1 ]; then
      if cargo cyclonedx -h >/dev/null 2>&1; then
        cargo cyclonedx -o dist/SBOM.cdx.json
        chmod 0644 dist/SBOM.cdx.json 2>/dev/null || true
        echo "Wrote dist/SBOM.cdx.json"
      else
        echo "cargo-cyclonedx not installed. Install with:" >&2
        echo "  cargo install cargo-cyclonedx" >&2
        exit 1
      fi
    else
      echo "Neither Docker nor cargo are installed." >&2
      echo "Install Rust: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y" >&2
      echo "Or install Docker: https://docs.docker.com/engine/install/" >&2
      exit 127
    fi
    ;;
  *)
    echo "Usage: $0 {test|cargo|sbom} [args...]" >&2
    echo "Examples:" >&2
    echo "  $0 test" >&2
    echo "  $0 cargo build --release" >&2
    echo "  AIFO_DEV_IMAGE=rust:1-alpine $0 sbom" >&2
    exit 2
    ;;
esac
