#!/usr/bin/env sh
# Build aifo-coder images without requiring GNU make.
# Usage:
#   ./scripts/build-images.sh              # builds codex, crush, aider, openhands, opencode, plandex
#   ./scripts/build-images.sh codex aider  # builds only specified targets
#   ./scripts/build-images.sh codex-slim   # also supports -slim targets

set -eu

IMAGE_PREFIX="${IMAGE_PREFIX:-aifo-coder}"
TAG="${TAG:-latest}"

# Determine registry prefix best-effort (mirror Makefile behavior)
RP=""
echo "Checking reachability of https://repository.migros.net ..."
if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://repository.migros.net/v2/ >/dev/null 2>&1; then
  echo "repository.migros.net reachable via HTTPS; tagging image with registry prefix."
  RP="repository.migros.net/"
else
  echo "repository.migros.net not reachable via HTTPS; using Docker Hub (no prefix)."
  if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://registry-1.docker.io/v2/ >/dev/null 2>&1; then
    echo "Docker Hub reachable via HTTPS; proceeding without registry prefix."
  else
    echo "Error: Neither repository.migros.net nor Docker Hub is reachable via HTTPS; cannot build images."
    exit 1
  fi
fi

targets="$*"
if [ -z "${targets}" ]; then
  targets="codex crush aider openhands opencode plandex"
fi

for t in ${targets}; do
  case "${t}" in
    codex|crush|aider|openhands|opencode|plandex|codex-slim|crush-slim|aider-slim|openhands-slim|opencode-slim|plandex-slim) ;;
    *)
      echo "Unknown target: ${t}"
      echo "Valid targets: codex crush aider openhands opencode plandex codex-slim crush-slim aider-slim openhands-slim opencode-slim plandex-slim"
      exit 2
      ;;
  esac

  img="${IMAGE_PREFIX}-${t}:${TAG}"
  if [ -n "${RP}" ]; then
    echo "Building ${img} (also tagging ${RP}${img}) ..."
    docker build --build-arg REGISTRY_PREFIX="${RP}" --target "${t}" -t "${img}" -t "${RP}${img}" .
  else
    echo "Building ${img} ..."
    docker build --build-arg REGISTRY_PREFIX="${RP}" --target "${t}" -t "${img}" .
  fi
done

echo "Done."
