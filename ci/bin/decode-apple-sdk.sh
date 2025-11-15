#!/bin/sh
# Decode the Apple SDK from the masked APPLE_SDK_BASE64 CI variable into ci/osx/${OSX_SDK_FILENAME}.
# Usage (in CI before building the macos-cross-rust-builder stage):
#   ci/bin/decode-apple-sdk.sh
# Expected env:
#   APPLE_SDK_BASE64 (required, masked, protected)
#   OSX_SDK_FILENAME (optional; default: MacOSX13.3.sdk.tar.xz)
set -eu

: "${OSX_SDK_FILENAME:=MacOSX13.3.sdk.tar.xz}"

if [ -z "${APPLE_SDK_BASE64:-}" ]; then
  echo "ERROR: APPLE_SDK_BASE64 is not set. Aborting." >&2
  exit 1
fi

mkdir -p ci/osx
out="ci/osx/${OSX_SDK_FILENAME}"

# Decode without line wrapping assumptions.
# GNU base64 uses -d; BSD base64 uses -D. Try both.
if base64 --help 2>/dev/null | grep -q ' -d'; then
  printf "%s" "$APPLE_SDK_BASE64" | base64 -d > "$out"
else
  printf "%s" "$APPLE_SDK_BASE64" | base64 -D > "$out"
fi

# Show only metadata; never print file contents.
ls -lh "$out"

# Verify checksum when provided via APPLE_SDK_SHA256; otherwise warn.
if [ -n "${APPLE_SDK_SHA256:-}" ]; then
  echo "${APPLE_SDK_SHA256}  $out" | sha256sum -c -
else
  echo "Warning: APPLE_SDK_SHA256 not set; skipping checksum verification." >&2
fi

# Best-effort integrity check if xz is available.
if command -v xz >/dev/null 2>&1; then
  if ! xz -t "$out" >/dev/null 2>&1; then
    echo "WARNING: xz test failed for $out. The SDK may be corrupt." >&2
  fi
fi

echo "Decoded Apple SDK to $out"
