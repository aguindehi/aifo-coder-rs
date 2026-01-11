#!/bin/sh
set -eu

# Compatibility wrapper for legacy CI jobs.
script_dir="$(cd "$(dirname "$0")" && pwd)"
target="${script_dir}/telemetry-smoke.sh"

if [ ! -x "$target" ]; then
  echo "telemetry-smoke.sh not found or not executable at $target" >&2
  exit 1
fi

exec "$target" "$@"
