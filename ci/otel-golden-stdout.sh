#!/bin/sh
# CI helper: build with OpenTelemetry, run a smoke test, and enforce golden stdout.
# Usage (from repo root):
#   ci/otel-golden-stdout.sh
set -eu

# Build library and binary with otel feature enabled (no tests).
cargo build --features otel

# Golden stdout test: --help output must be identical with and without telemetry enabled.
TMP_DIR="${TMPDIR:-/tmp}/aifo-otel-golden.$$"
mkdir -p "$TMP_DIR"
BASE="$TMP_DIR/base.txt"
OTEL="$TMP_DIR/otel.txt"

# Baseline (--help, no telemetry)
cargo run --quiet -- --help >"$BASE"

# Telemetry-enabled (--help, otel feature + AIFO_CODER_OTEL=1)
AIFO_CODER_OTEL=1 \
cargo run --quiet --features otel -- --help >"$OTEL"

if ! cmp -s "$BASE" "$OTEL"; then
  echo "ERROR: stdout differs when telemetry is enabled (AIFO_CODER_OTEL=1)." >&2
  echo "Diff (baseline vs otel):" >&2
  diff -u "$BASE" "$OTEL" || true
  rm -rf "$TMP_DIR"
  exit 1
fi

# Smoke run with metrics enabled; do not install fmt layer by default to avoid extra stderr logs.
AIFO_CODER_OTEL=1 \
AIFO_CODER_OTEL_METRICS=1 \
AIFO_CODER_TRACING_FMT="${AIFO_CODER_TRACING_FMT:-0}" \
cargo run --quiet --features otel -- --help >/dev/null

rm -rf "$TMP_DIR"

echo "OK: otel build, golden stdout, and smoke run all passed."
