#!/usr/bin/env bash
set -euo pipefail

# Isolation guard: run all checks in a temporary HOME/GNUPGHOME to avoid touching the caller's config.
orig_home="${HOME:-}"
tmp_root="$(mktemp -d /tmp/aifo-gpg-tests.XXXXXX)"
cleanup() {
  rm -rf "$tmp_root"
}
trap cleanup EXIT INT TERM

export HOME="$tmp_root/home"
export GNUPGHOME="$HOME/.gnupg"
export XDG_RUNTIME_DIR="$tmp_root/run"
export XDG_CONFIG_HOME="$HOME/.config"
export XDG_DATA_HOME="$HOME/.local/share"
export XDG_CACHE_HOME="$HOME/.cache"

mkdir -p "$HOME" "$GNUPGHOME" "$XDG_RUNTIME_DIR" "$XDG_CONFIG_HOME" "$XDG_DATA_HOME" "$XDG_CACHE_HOME"
chmod 700 "$HOME" "$GNUPGHOME" "$XDG_RUNTIME_DIR"

echo "test-gpg-signing: isolated HOME=${HOME} (original HOME=${orig_home})"
echo "test-gpg-signing: GNUPGHOME=${GNUPGHOME}"

# Placeholder for upcoming GPG/Git signing tests. When implemented, keep all writes within $HOME/$GNUPGHOME.
echo "test-gpg-signing: no-op (isolation harness ready)."
