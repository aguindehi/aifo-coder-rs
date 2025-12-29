#!/bin/sh
# dash-compatible: avoid pipefail (not supported by /bin/sh here)
set -eu

# Wrapper to force loopback pinentry so Git signing works in non-interactive shells.
# Optional env vars:
#   AIFO_GPG_PASSPHRASE: inline passphrase (discouraged; use *_FILE instead)
#   AIFO_GPG_PASSPHRASE_FILE: path to file containing passphrase (single line)
# NOTE: gpg-agent must allow loopback (entrypoint enforces allow-loopback-pinentry).

args="--batch --pinentry-mode loopback"
if [ -n "${AIFO_GPG_PASSPHRASE_FILE:-}" ] && [ -f "$AIFO_GPG_PASSPHRASE_FILE" ]; then
    exec gpg $args --passphrase-file "$AIFO_GPG_PASSPHRASE_FILE" "$@"
fi
if [ -n "${AIFO_GPG_PASSPHRASE:-}" ]; then
    exec gpg $args --passphrase "$AIFO_GPG_PASSPHRASE" "$@"
fi
exec gpg $args "$@"
