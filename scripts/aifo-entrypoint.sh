#!/bin/sh
set -e
umask 077

log_prefix="aifo-entrypoint"
log_verbose="${AIFO_TOOLCHAIN_VERBOSE:-0}"
runtime_user="${AIFO_RUNTIME_USER:-coder}"

if [ "$(id -u)" = "0" ] && [ "${AIFO_ENTRYPOINT_REEXEC:-0}" != "1" ]; then
    if command -v gosu >/dev/null 2>&1; then
        exec env AIFO_ENTRYPOINT_REEXEC=1 gosu "$runtime_user" "$0" "$@"
    fi
    printf '%s: warning: gosu missing; continuing as root\n' "$log_prefix" >&2
fi

log_debug() {
    if [ "$log_verbose" = "1" ]; then
        printf '%s: %s\n' "$log_prefix" "$1" >&2
    fi
}

# Ensure HOME is sane and writable
if [ -z "${HOME:-}" ] || [ "$HOME" = "/" ] || [ ! -d "$HOME" ] || [ ! -w "$HOME" ]; then
    export HOME="/home/coder"
fi
mkdir -p "$HOME"
chmod 0750 "$HOME" 2>/dev/null || true

if [ -z "${GNUPGHOME:-}" ]; then
    export GNUPGHOME="$HOME/.gnupg"
fi
mkdir -p "$GNUPGHOME"
chmod 0700 "$GNUPGHOME" 2>/dev/null || true

if [ -d "$HOME/.gnupg-host" ]; then
    for f in pubring.kbx trustdb.gpg gpg.conf gpg-agent.conf; do
        if [ -f "$HOME/.gnupg-host/$f" ] && [ ! -f "$GNUPGHOME/$f" ]; then
            cp -a "$HOME/.gnupg-host/$f" "$GNUPGHOME/$f"
        fi
    done
    for d in private-keys-v1.d openpgp-revocs.d; do
        if [ -d "$HOME/.gnupg-host/$d" ] && [ ! -e "$GNUPGHOME/$d" ]; then
            cp -a "$HOME/.gnupg-host/$d" "$GNUPGHOME/$d"
        fi
    done
fi

if [ -z "${XDG_RUNTIME_DIR:-}" ]; then
    export XDG_RUNTIME_DIR="/tmp/runtime-$(id -u)"
fi
mkdir -p "$XDG_RUNTIME_DIR" "$XDG_RUNTIME_DIR/gnupg"
chmod 0700 "$XDG_RUNTIME_DIR" "$XDG_RUNTIME_DIR/gnupg" 2>/dev/null || true

ensure_local_tree() {
    for d in "$HOME/.local" "$HOME/.local/share" "$HOME/.local/state" \
             "$HOME/.local/share/uv" "$HOME/.local/share/pnpm" "$HOME/.cache"; do
        mkdir -p "$d"
        chmod 0755 "$d" 2>/dev/null || true
    done
}

ensure_local_tree

# Bootstrap gnupg configs
if [ ! -f "$GNUPGHOME/gpg-agent.conf" ] && command -v pinentry-curses >/dev/null 2>&1; then
    printf '%s\n' "pinentry-program /usr/bin/pinentry-curses" > "$GNUPGHOME/gpg-agent.conf"
fi
( grep -q '^allow-loopback-pinentry' "$GNUPGHOME/gpg-agent.conf" 2>/dev/null || \
    printf '%s\n' "allow-loopback-pinentry" >> "$GNUPGHOME/gpg-agent.conf" ) || true
( grep -q '^default-cache-ttl ' "$GNUPGHOME/gpg-agent.conf" 2>/dev/null || \
    printf '%s\n' "default-cache-ttl 7200" >> "$GNUPGHOME/gpg-agent.conf" ) || true
( grep -q '^max-cache-ttl ' "$GNUPGHOME/gpg-agent.conf" 2>/dev/null || \
    printf '%s\n' "max-cache-ttl 86400" >> "$GNUPGHOME/gpg-agent.conf" ) || true
if [ -t 0 ] || [ -t 1 ]; then
    export GPG_TTY="${GPG_TTY:-/dev/tty}"
fi
unset GPG_AGENT_INFO || true
if command -v gpgconf >/dev/null 2>&1; then
    gpgconf --kill gpg-agent >/dev/null 2>&1 || true
    gpgconf --launch gpg-agent >/dev/null 2>&1 || true
else
    gpg-agent --daemon >/dev/null 2>&1 || true
fi

sanitize_name() {
    case "$1" in
        *[!A-Za-z0-9._-]*|"") return 1 ;;
        *) return 0 ;;
    esac
}

lower() {
    printf '%s' "$1" | tr 'A-Z' 'a-z'
}

is_secret_name() {
    name_lc=$(lower "$1")
    hints="$(lower "${CFG_HINTS}")"
    IFS=,; for hint in $hints; do
        case "$name_lc" in
            *"$hint"*) unset IFS; return 0 ;;
        esac
    done
    unset IFS
    return 1
}

copy_safe_file() {
    src="$1"
    dest_dir="$2"
    base="$(basename "$src")"
    if ! sanitize_name "$base"; then
        log_debug "config: skip invalid name $base"
        return
    fi
    if [ -h "$src" ]; then
        log_debug "config: skip symlink $base"
        return
    fi
    if [ ! -f "$src" ]; then
        log_debug "config: skip non-regular $base"
        return
    fi
    size=$(wc -c < "$src" 2>/dev/null || printf '0')
    if [ "$size" -gt "$CFG_MAX" ]; then
        log_debug "config: skip oversized $base"
        return
    fi
    ext="${base##*.}"
    ext_lc=$(lower "$ext")
    allowed=0
    IFS=,; for e in $CFG_EXT; do
        if [ "$ext_lc" = "$(lower "$e")" ]; then
            allowed=1
            break
        fi
    done
    unset IFS
    if [ "$allowed" -ne 1 ]; then
        log_debug "config: skip disallowed extension $base"
        return
    fi
    mode=0644
    case "$ext_lc" in
        pem|key|token) mode=0600 ;;
    esac
    if is_secret_name "$base"; then
        mode=0600
    fi
    install -d -m 0700 "$dest_dir"
    install -m "$mode" "$src" "$dest_dir/$base" 2>/dev/null || cp "$src" "$dest_dir/$base"
    chmod "$mode" "$dest_dir/$base" 2>/dev/null || true
}

copy_tree_contents() {
    src_dir="$1"
    dst_dir="$2"
    for item in "$src_dir"/.* "$src_dir"/*; do
        [ -e "$item" ] || continue
        base="$(basename "$item")"
        if [ "$base" = "." ] || [ "$base" = ".." ]; then
            continue
        fi
        if [ -d "$item" ] && [ ! -h "$item" ]; then
            sub="$dst_dir/$base"
            install -d -m 0700 "$sub"
            copy_tree_contents "$item" "$sub"
            continue
        fi
        copy_safe_file "$item" "$dst_dir"
    done
}

copy_agent_configs() {
    if [ -d "$CFG_DST/aider" ]; then
        for f in ".aider.conf.yml" ".aider.model.settings.yml" ".aider.model.metadata.json"; do
            [ -f "$CFG_DST/aider/$f" ] || continue
            install -m 0644 "$CFG_DST/aider/$f" "$HOME/$f" 2>/dev/null || cp "$CFG_DST/aider/$f" "$HOME/$f"
        done
    fi
    if [ -d "$CFG_DST/crush" ]; then
        install -d -m 0700 "$HOME/.crush"
        copy_tree_contents "$CFG_DST/crush" "$HOME/.crush"
    fi
    if [ -d "$CFG_DST/openhands" ]; then
        install -d -m 0700 "$HOME/.openhands"
        copy_tree_contents "$CFG_DST/openhands" "$HOME/.openhands"
    fi
    if [ -d "$CFG_DST/opencode" ]; then
        install -d -m 0700 "$HOME/.config"
        install -d -m 0700 "$HOME/.config/opencode"
        copy_tree_contents "$CFG_DST/opencode" "$HOME/.config/opencode"
    fi
    if [ -d "$CFG_DST/plandex" ]; then
        install -d -m 0700 "$HOME/.plandex-home"
        copy_tree_contents "$CFG_DST/plandex" "$HOME/.plandex-home"
    fi
}

maybe_copy_configs() {
    CFG_HOST="${AIFO_CONFIG_HOST_DIR:-${AIFO_CODER_CONFIG_HOST_DIR:-$HOME/.aifo-config-host}}"
    CFG_DST="${AIFO_CONFIG_DST_DIR:-$HOME/.aifo-config}"
    CFG_ENABLE="${AIFO_CONFIG_ENABLE:-1}"
    CFG_MAX="${AIFO_CONFIG_MAX_SIZE:-262144}"
    CFG_EXT="${AIFO_CONFIG_ALLOW_EXT:-json,toml,yaml,yml,ini,conf,crt,pem,key,token}"
    CFG_HINTS="${AIFO_CONFIG_SECRET_HINTS:-token,secret,key,pem}"
    CFG_COPY_ALWAYS="${AIFO_CONFIG_COPY_ALWAYS:-0}"

    export AIFO_CODER_CONFIG_DIR="$CFG_DST"

    [ "$CFG_ENABLE" = "1" ] || return
    [ -d "$CFG_HOST" ] || return

    install -d -m 0700 "$CFG_DST"

    stamp="$CFG_DST/.copied"
    should_copy=1
    if [ "$CFG_COPY_ALWAYS" != "1" ] && [ -f "$stamp" ]; then
        src_mtime=$(find "$CFG_HOST" -type f -printf '%T@\n' 2>/dev/null | sort -nr | head -n1)
        dst_mtime=$(stat -c %Y "$stamp" 2>/dev/null || stat -f %m "$stamp" 2>/dev/null || printf '0')
        src_mtime=${src_mtime%%.*}
        [ -z "$src_mtime" ] && src_mtime=0
        if [ "$src_mtime" -le "$dst_mtime" ]; then
            should_copy=0
        fi
    fi

    if [ "$should_copy" = "1" ]; then
        log_debug "config: copying from $CFG_HOST"
        find "$CFG_DST" -mindepth 1 -maxdepth 1 ! -name '.copied' -exec rm -rf {} + 2>/dev/null || true
        if [ -d "$CFG_HOST/global" ]; then
            copy_tree_contents "$CFG_HOST/global" "$CFG_DST/global"
        fi
        for d in "$CFG_HOST"/*; do
            [ -d "$d" ] || continue
            base="$(basename "$d")"
            [ "$base" = "global" ] && continue
            dest="$CFG_DST/$base"
            copy_tree_contents "$d" "$dest"
        done
        copy_agent_configs
        install -m 0600 /dev/null "$stamp" 2>/dev/null || :
        touch "$stamp"
    else
        log_debug "config: skip copy (up-to-date)"
    fi
}

maybe_copy_configs

# Azure/OpenAI normalization for OpenHands
if [ "${OPENAI_API_TYPE:-}" = "azure" ]; then
    settings="$HOME/.openhands/agent_settings.json"
    if [ -f "$settings" ]; then
        if [ -n "${AIFO_API_VERSION:-}" ]; then
            sed -i -E "s|\"api_version\"[[:space:]]*:[[:space:]]*\"[^\"]*\"|\"api_version\": \"${AIFO_API_VERSION}\"|g" "$settings"
        fi
        if [ -n "${AIFO_API_BASE:-}" ]; then
            sed -i -E "s|\"base_url\"[[:space:]]*:[[:space:]]*\"[^\"]*\"|\"base_url\": \"${AIFO_API_BASE}\"|g" "$settings"
        fi
        if [ -n "${AIFO_API_KEY:-}" ]; then
            sed -i -E "s|\"api_key\"[[:space:]]*:[[:space:]]*\"[^\"]*\"|\"api_key\": \"${AIFO_API_KEY}\"|g" "$settings"
        fi
    fi
fi

# Claude config symlink exposure
if [ -n "${AIFO_CODER_CONFIG_DIR:-}" ]; then
    real_conf="$HOME/.config/claude/claude_desktop_config.json"
    link_dir="$AIFO_CODER_CONFIG_DIR/claude"
    link_path="$link_dir/claude_desktop_config.json"
    if [ -f "$real_conf" ] || [ -L "$real_conf" ]; then
        if [ ! -e "$link_path" ]; then
            install -d -m 0700 "$link_dir"
            ln -s "$real_conf" "$link_path" 2>/dev/null || true
        fi
    fi
fi

exec "$@"
