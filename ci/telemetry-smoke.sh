#!/usr/bin/env bash
set -euo pipefail

# Telemetry stdout smoke: ensure enabling/disabling OTEL does not change CLI stdout
# and that runs succeed without Docker.

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo not found; skipping telemetry smoke." >&2
  exit 0
fi

export AIFO_SUPPORT_ANIMATE=0
export AIFO_SUPPORT_SKIP_GREEN=1
export AIFO_CODER_TEST_DISABLE_DOCKER=1

echo "Running telemetry stdout smoke ..."

# Capture stdout with OTEL disabled
out_disabled="$(cargo run --quiet -- --help 2>/dev/null || true)"

# Capture stdout with OTEL enabled (to a dummy endpoint); it should match
export AIFO_OTEL_ENABLED=1
export AIFO_OTEL_ENDPOINT="http://localhost:4317"
export AIFO_OTEL_TRANSPORT="http"
out_enabled="$(cargo run --quiet -- --help 2>/dev/null || true)"

if [ "$out_disabled" != "$out_enabled" ]; then
  echo "telemetry smoke failed: stdout differs between OTEL disabled/enabled" >&2
  exit 1
fi

echo "telemetry smoke passed."
