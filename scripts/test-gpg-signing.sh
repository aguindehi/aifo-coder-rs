#!/usr/bin/env bash
set -euo pipefail

# Isolation guard: run everything under a temp tree to avoid touching the caller's config/home.
orig_home="${HOME:-}"
tmp_root="$(mktemp -d /tmp/aifo-gpg-tests.XXXXXX)"
cleanup() {
  rm -rf "$tmp_root"
}
trap cleanup EXIT INT TERM

# Preserve docker connectivity without touching the caller's config by copying DOCKER_CONFIG.
source_docker_config="${DOCKER_CONFIG:-${orig_home}/.docker}"
if [ -d "$source_docker_config" ]; then
  mkdir -p "$tmp_root/docker-config"
  cp -a "$source_docker_config"/. "$tmp_root/docker-config"/ 2>/dev/null || true
  export DOCKER_CONFIG="$tmp_root/docker-config"
fi

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
if [ -n "${DOCKER_CONFIG:-}" ]; then
  echo "test-gpg-signing: DOCKER_CONFIG (copied)=${DOCKER_CONFIG}"
fi

if ! command -v docker >/dev/null 2>&1; then
  echo "test-gpg-signing: docker not available; skipping container signing checks." >&2
  exit 0
fi

AGENTS="${AIFO_TEST_AGENTS:-opencode codex crush aider openhands plandex}"
PASSPHRASE="${AIFO_TEST_GPG_PASSPHRASE:-test-passphrase}"

is_fullscreen() {
  case "$1" in
    opencode|opencode-slim|codex|codex-slim|crush|crush-slim) return 0 ;;
    *) return 1 ;;
  esac
}

run_agent() {
  agent="$1"
  image="${AIFO_TEST_IMAGE_PREFIX:-aifo-coder}-${agent}:${AIFO_TEST_IMAGE_TAG:-latest}"
  if ! err="$(docker image inspect "$image" 2>&1 >/dev/null)"; then
    if printf '%s' "$err" | grep -qi "No such image"; then
      echo "test-gpg-signing: skip ${agent} (image not found: ${image})"
      return 0
    fi
    echo "test-gpg-signing: docker inspect failed for ${image}: ${err}" >&2
    return 1
  fi

  echo "test-gpg-signing: testing agent=${agent} image=${image}"
  docker run --rm -i \
    -e HOME=/tmp/home \
    -e GNUPGHOME=/tmp/home/.gnupg \
    -e XDG_RUNTIME_DIR=/tmp/run \
    -e XDG_CONFIG_HOME=/tmp/home/.config \
    -e XDG_DATA_HOME=/tmp/home/.local/share \
    -e XDG_CACHE_HOME=/tmp/home/.cache \
    -e AIFO_AGENT_NAME="$agent" \
    -e TEST_PASSPHRASE="$PASSPHRASE" \
    "$image" /bin/sh -euo pipefail <<'EOS'
home="$HOME"
gnupg="$GNUPGHOME"
pass="$TEST_PASSPHRASE"
mkdir -p "$home" "$gnupg" "$XDG_RUNTIME_DIR" "$XDG_CONFIG_HOME" "$XDG_DATA_HOME" "$XDG_CACHE_HOME"
chmod 700 "$home" "$gnupg" "$XDG_RUNTIME_DIR"

require_bin() {
  b="$1"
  if ! command -v "$b" >/dev/null 2>&1; then
    echo "missing binary: $b" >&2
    exit 1
  fi
}

for b in gpg gpg-agent gpg-preset-passphrase pinentry-curses git; do
  require_bin "$b"
done

cat >"$gnupg/gpg-agent.conf" <<EOF
allow-loopback-pinentry
allow-preset-passphrase
pinentry-program /usr/bin/pinentry-curses
default-cache-ttl 21600
max-cache-ttl 86400
log-file /tmp/gpg-agent.log
EOF
cat >"$gnupg/gpg.conf" <<EOF
pinentry-mode loopback
EOF
chmod 600 "$gnupg/gpg-agent.conf" "$gnupg/gpg.conf" || true

gpgconf --kill gpg-agent >/dev/null 2>&1 || true
gpgconf --launch gpg-agent >/dev/null 2>&1 || true

uid="Test User <test@mgb.ch>"
gpg --batch --yes --pinentry-mode loopback --passphrase "$pass" --quick-generate-key "$uid" ed25519 sign 0

fpr="$(gpg --batch --with-colons --list-secret-keys "$uid" | awk -F: '$1=="fpr"{print $10; exit}')"
grip="$(gpg --batch --with-colons --with-keygrip --list-secret-keys "$uid" | awk -F: '$1=="grp"{print $10; exit}')"
if [ -z "$fpr" ] || [ -z "$grip" ]; then
  echo "failed to obtain key fingerprint/keygrip" >&2
  exit 1
fi

if printf '%s\n' "$pass" | gpg-preset-passphrase --homedir "$gnupg" --preset "$grip" >/dev/null 2>&1; then
  :
else
  echo "gpg-preset-passphrase failed" >&2
  exit 1
fi

cached="$(gpg-connect-agent "keyinfo $grip" /bye | awk '/^S KEYINFO/{print $0}')"
if printf '%s' "$cached" | grep -q ' 1 P'; then
  :
else
  echo "passphrase not cached (keyinfo: $cached)" >&2
  exit 1
fi

echo test | gpg --batch --yes --pinentry-mode loopback --local-user "$fpr" --clearsign >/tmp/sig.txt

export GIT_CONFIG_GLOBAL=/tmp/gitconfig
export GIT_CONFIG_NOSYSTEM=1
repo=/tmp/repo
mkdir -p "$repo"
cd "$repo"
git init -q
echo hello > file.txt
git add file.txt
git -c user.name="Test User" -c user.email="test@mgb.ch" -c user.signingkey="$fpr" -c commit.gpgsign=true -c gpg.program=gpg commit -qm "signed commit"
EOS
}

overall=0
for agent in $AGENTS; do
  if ! run_agent "$agent"; then
    echo "test-gpg-signing: FAILED for agent=${agent}" >&2
    overall=1
  else
    echo "test-gpg-signing: OK for agent=${agent}"
  fi
done

exit "$overall"
