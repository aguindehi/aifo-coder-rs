#!/bin/sh
# CI helper: build with OpenTelemetry, run a smoke test, and enforce golden stdout.
# Usage (from repo root):
#   ci/otel-golden-stdout.sh
set -eu

# Build library and binary with otel feature enabled (no tests).
cargo build --features otel

# Golden stdout test: --help output must be identical regardless of telemetry env settings.
TMP_DIR="${TMPDIR:-/tmp}/aifo-otel-golden.$$"
mkdir -p "$TMP_DIR"
BASE="$TMP_DIR/base.txt"
OTEL="$TMP_DIR/otel.txt"

# Baseline (--help, telemetry default ON with otel feature)
cargo run --quiet --features otel -- --help >"$BASE"

# Telemetry explicitly disabled via env
AIFO_CODER_OTEL=0 \
cargo run --quiet --features otel -- --help >"$OTEL"

if ! cmp -s "$BASE" "$OTEL"; then
  echo "ERROR: stdout differs when telemetry env settings change (default vs AIFO_CODER_OTEL=0)." >&2
  echo "Diff (baseline vs disabled):" >&2
  diff -u "$BASE" "$OTEL" || true
  rm -rf "$TMP_DIR"
  exit 1
fi

# Smoke run with metrics enabled; do not install fmt layer by default to avoid extra stderr logs.
AIFO_CODER_OTEL_METRICS=1 \
AIFO_CODER_TRACING_FMT="${AIFO_CODER_TRACING_FMT:-0}" \
cargo run --quiet --features otel -- --help >/dev/null

# OTLP exporter smoke (feature: otel-otlp). Should not panic even if a collector is absent.
cargo build --features otel-otlp
OTEL_EXPORTER_OTLP_ENDPOINT="${OTEL_EXPORTER_OTLP_ENDPOINT:-https://localhost:4318}" \
AIFO_CODER_TRACING_FMT="${AIFO_CODER_TRACING_FMT:-0}" \
AIFO_CODER_OTEL_METRICS=1 \
cargo run --quiet --features otel-otlp -- --help >/dev/null

rm -rf "$TMP_DIR"

echo "OK: otel build, golden stdout, and smoke run all passed."
