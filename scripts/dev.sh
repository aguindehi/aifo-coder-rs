#!/usr/bin/env sh
# Developer helper: run cargo or generate SBOM inside a Docker container.
# Works even if cargo or make are not installed on the host.
set -e

IMAGE="${AIFO_DEV_IMAGE:-rust:1-bookworm}"
WORKDIR="/workspace"

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
    run_in_container cargo test --all-targets "$@"
    ;;
  cargo)
    # Pass-through arbitrary cargo commands
    if [ $# -eq 0 ]; then
      echo "Usage: $0 cargo <args...>" >&2
      exit 1
    fi
    run_in_container cargo "$@"
    ;;
  sbom)
    # Generate CycloneDX SBOM at dist/SBOM.cdx.json
    mkdir -p dist
    # Install cargo-cyclonedx if missing, then generate SBOM
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
