#!/bin/sh
set -e
umask 077

log_prefix="aifo-entrypoint"
log_verbose="${AIFO_TOOLCHAIN_VERBOSE:-0}"
runtime_user="${AIFO_RUNTIME_USER:-coder}"
IS_ROOT=0
if [ "$(id -u)" = "0" ]; then
    IS_ROOT=1
fi

log_debug() {
    if [ "$log_verbose" = "1" ]; then
        printf '%s: %s\n' "$log_prefix" "$1" >&2
    fi
}

is_fullscreen_agent() {
    case "${AIFO_AGENT_NAME:-}" in
        opencode) return 0 ;;
        *) return 1 ;;
    esac
}

resolve_home() {
    home_path="$(getent passwd "$1" 2>/dev/null | cut -d: -f6)"
    if [ -z "$home_path" ]; then
        home_path="/home/$1"
    fi
    printf '%s' "$home_path"
}

safe_install_dir() {
    dir="$1"
    mode="${2:-0700}"
    if [ "$IS_ROOT" = "1" ]; then
        install -d -m "$mode" "$dir" 2>/dev/null || true
    else
        mkdir -p "$dir" 2>/dev/null || true
    fi
}

maybe_chmod() {
    if [ "$IS_ROOT" = "1" ]; then
        chmod "$@" 2>/dev/null || true
    fi
}

ensure_gpg_tty() {
    if [ -t 0 ] || [ -t 1 ]; then
        tty_path="$(tty 2>/dev/null || printf '')"
        case "$tty_path" in
            ""|"not a tty") ;;
            *)
                export GPG_TTY="$tty_path"
                return 0
                ;;
        esac
    fi
    if [ -n "${GPG_TTY:-}" ] && [ -c "$GPG_TTY" ]; then
        return 0
    fi
    return 1
}

refresh_gpg_agent_tty() {
    if ! command -v gpg-connect-agent >/dev/null 2>&1; then
        return
    fi
    gpg-connect-agent updatestartuptty /bye >/dev/null 2>&1 || true
    gpg-connect-agent reloadagent /bye >/dev/null 2>&1 || true
}

copy_with_mode() {
    mode="$1"
    src="$2"
    dest="$3"
    if [ "$IS_ROOT" = "1" ]; then
        install -m "$mode" "$src" "$dest" 2>/dev/null || cp "$src" "$dest" 2>/dev/null || true
    else
        cp "$src" "$dest" 2>/dev/null || true
    fi
}

ensure_conf_line() {
    file="$1"
    prefix="$2"
    newline="$3"
    tmp="$(mktemp "$file.tmp.XXXXXX" 2>/dev/null || mktemp)"
    touch "$file"
    grep -v "^$prefix" "$file" 2>/dev/null > "$tmp" || true
    printf '%s
' "$newline" >> "$tmp"
    mv "$tmp" "$file"
    maybe_chmod 0600 "$file"
}

sync_host_gpg() {
    host_dir="$HOME/.gnupg-host"
    [ -d "$host_dir" ] || return
    safe_install_dir "$GNUPGHOME" 0700
    find "$GNUPGHOME" -mindepth 1 -maxdepth 1 -exec rm -rf {} + 2>/dev/null || true
    if command -v rsync >/dev/null 2>&1; then
        rsync -a --delete "$host_dir"/ "$GNUPGHOME"/ >/dev/null 2>&1 || \
            cp -a "$host_dir"/. "$GNUPGHOME"/ 2>/dev/null || true
    else
        cp -a "$host_dir"/. "$GNUPGHOME"/ 2>/dev/null || true
    fi
    chmod 700 "$GNUPGHOME" 2>/dev/null || true
}

detect_signing_key() {
    key="$(git config --global user.signingkey 2>/dev/null || printf '')"
    if [ -z "$key" ]; then
        key="$(git config --system user.signingkey 2>/dev/null || printf '')"
    fi
    if [ -z "$key" ]; then
        key="$(gpg --list-secret-keys --with-colons 2>/dev/null | awk -F: '/^sec/ { print $5; exit }')"
    fi
    printf '%s' "$key"
}

gpg_keygrip_for_signing() {
    key_filter="$1"
    if [ -n "$key_filter" ]; then
        listing="$(gpg --list-secret-keys --with-colons --with-keygrip "$key_filter" 2>/dev/null || true)"
    else
        listing="$(gpg --list-secret-keys --with-colons --with-keygrip 2>/dev/null || true)"
    fi
    printf '%s\n' "$listing" | awk -F: '/^grp/ { print $10; exit }'
}

maybe_preset_gpg_passphrase() {
    signing_key="$1"
    passphrase=""
    if [ -n "${AIFO_GPG_PASSPHRASE_FILE:-}" ] && [ -r "$AIFO_GPG_PASSPHRASE_FILE" ]; then
        passphrase="$(head -n1 "$AIFO_GPG_PASSPHRASE_FILE" 2>/dev/null | tr -d '\r\n')"
    elif [ -n "${AIFO_GPG_PASSPHRASE:-}" ]; then
        passphrase="$AIFO_GPG_PASSPHRASE"
    else
        return 1
    fi
    if [ -z "$passphrase" ]; then
        return 1
    fi
    if ! command -v gpg-preset-passphrase >/dev/null 2>&1; then
        return 1
    fi
    keygrip="$(gpg_keygrip_for_signing "$signing_key")"
    if [ -z "$keygrip" ]; then
        return 1
    fi
    if printf '%s' "$passphrase" | gpg-preset-passphrase --preset "$keygrip" >/dev/null 2>&1; then
        printf '%s: gpg: passphrase cached via gpg-preset-passphrase.\n' "$log_prefix" >&2
        unset passphrase
        export AIFO_GPG_PRIMED=1
        return 0
    fi
    status=$?
    unset passphrase
    printf '%s: warning: gpg-preset-passphrase failed (exit %s); falling back to pinentry.\n' "$log_prefix" "$status" >&2
    return 1
}

prime_gpg_agent_if_requested() {
    if [ "${AIFO_GPG_REQUIRE_PRIME:-0}" != "1" ]; then
        return
    fi
    if [ "${AIFO_GPG_PRIMED:-0}" = "1" ]; then
        log_debug "gpg priming already completed; skipping"
        return
    fi
    if ! command -v gpg >/dev/null 2>&1; then
        printf '%s: error: gpg priming requested but gpg is unavailable in the container.\n' "$log_prefix" >&2
        exit 1
    fi
    if ! gpg --list-secret-keys --with-colons 2>/dev/null | grep -q '^sec'; then
        printf '%s: error: commit signing enabled but no secret key was found inside the container. Mount ~/.gnupg and retry.\n' "$log_prefix" >&2
        exit 1
    fi

    signing_key="$(detect_signing_key)"

    if is_fullscreen_agent; then
        # Fullscreen agents (e.g., opencode) must never rely on pinentry during runtime.
        # Obtain the passphrase from env/file or via a single interactive prompt here,
        # then supply it via AIFO_GPG_PASSPHRASE so the aifo-gpg-wrapper can always
        # use loopback mode without invoking pinentry.
        if [ -z "${AIFO_GPG_PASSPHRASE:-}" ] && [ -n "${AIFO_GPG_PASSPHRASE_FILE:-}" ] && [ -r "$AIFO_GPG_PASSPHRASE_FILE" ]; then
            AIFO_GPG_PASSPHRASE="$(head -n1 "$AIFO_GPG_PASSPHRASE_FILE" 2>/dev/null | tr -d '\r\n')"
            export AIFO_GPG_PASSPHRASE
        fi

        if [ -z "${AIFO_GPG_PASSPHRASE:-}" ]; then
            if [ ! -t 0 ] && [ ! -t 1 ]; then
                printf '%s: error: fullscreen agent requires GPG passphrase via AIFO_GPG_PASSPHRASE or AIFO_GPG_PASSPHRASE_FILE; no TTY available for interactive prompt.\n' "$log_prefix" >&2
                exit 1
            fi
            if ! ensure_gpg_tty; then
                printf '%s: error: fullscreen agent requires GPG passphrase via env/file; unable to determine controlling terminal for interactive prompt.\n' "$log_prefix" >&2
                exit 1
            fi
            # Read passphrase once interactively (no echo) and cache in env for wrapper use.
            printf '%s: enter GPG passphrase for fullscreen agent (input will not be echoed): ' "$log_prefix" >&2
            # shellcheck disable=SC2162
            stty -echo 2>/dev/null || true
            read pass 2>/dev/null || pass=""
            stty echo 2>/dev/null || true
            printf '\n' >&2
            if [ -z "$pass" ]; then
                printf '%s: error: empty GPG passphrase entered; aborting.\n' "$log_prefix" >&2
                exit 1
            fi
            AIFO_GPG_PASSPHRASE="$pass"
            unset pass
            export AIFO_GPG_PASSPHRASE
        fi

        # Optional: preset into gpg-agent so loopback can reuse it without pinentry.
        maybe_preset_gpg_passphrase "$signing_key" || true
        export AIFO_GPG_PRIMED=1
        return
    fi

    # Non-fullscreen agents: prefer preset via env/file, fall back to one interactive pinentry step.
    if maybe_preset_gpg_passphrase "$signing_key"; then
        return
    fi
    if [ ! -t 0 ] && [ ! -t 1 ]; then
        printf '%s: warning: gpg priming skipped (no interactive terminal). Signed commits may prompt later.\n' "$log_prefix" >&2
        return
    fi
    if ! ensure_gpg_tty; then
        printf '%s: warning: gpg priming skipped (unable to determine controlling terminal). Signed commits may prompt later.\n' "$log_prefix" >&2
        return
    fi
    refresh_gpg_agent_tty
    prime_cmd="gpg --armor --sign --detach-sig --output /dev/null"
    set -- gpg --armor --sign --detach-sig --yes --output /dev/null
    if [ -n "$signing_key" ]; then
        set -- "$@" --local-user "$signing_key" --default-key "$signing_key"
        prime_cmd="$prime_cmd --local-user \"$signing_key\" --default-key \"$signing_key\""
    fi
    set -- "$@" /dev/null
    prime_cmd="$prime_cmd /dev/null"
    printf '%s: gpg: requesting passphrase via pinentry-curses before launching the agent...\n' "$log_prefix" >&2
    if "$@"; then
        printf '%s: gpg: passphrase cached for this session.\n' "$log_prefix" >&2
        export AIFO_GPG_PRIMED=1
        return
    else
        status=$?
    fi
    if [ "$status" -eq 0 ]; then
        status=1
    fi
    printf '%s: error: gpg priming failed (exit %s).\n' "$log_prefix" "$status" >&2
    printf '%s: hint: rerun `%s` inside the container to inspect the failure.\n' "$log_prefix" "$prime_cmd" >&2
    exit "$status"
}

runtime_home="$(resolve_home "$runtime_user")"
[ -n "$runtime_home" ] || runtime_home="/home/$runtime_user"

# For fullscreen agents (e.g., opencode), keep loopback pinentry enabled so that
# gpg operations from non-interactive environments (no TTY) can still use the
# cached passphrase via gpg-agent and the aifo-gpg-wrapper.

if [ "$IS_ROOT" = "1" ] && [ "${AIFO_ENTRYPOINT_REEXEC:-0}" != "1" ]; then
    safe_install_dir "$runtime_home" 0750
    chown -R "$runtime_user:$runtime_user" "$runtime_home" 2>/dev/null || true
    export HOME="$runtime_home"
    if command -v gosu >/dev/null 2>&1; then
        exec env AIFO_ENTRYPOINT_REEXEC=1 gosu "$runtime_user" "$0" "$@"
    fi
    printf '%s: warning: gosu missing; continuing as root\n' "$log_prefix" >&2
fi

# Ensure HOME is sane and writable

if [ -z "${HOME:-}" ] || [ "$HOME" = "/" ] || [ ! -d "$HOME" ] || [ ! -w "$HOME" ]; then
    export HOME="$runtime_home"
fi
if [ "$IS_ROOT" = "1" ]; then
    safe_install_dir "$HOME" 0750
    maybe_chmod 1777 "$HOME"
else
    mkdir -p "$HOME" 2>/dev/null || true
fi

if [ -z "${GNUPGHOME:-}" ]; then
    export GNUPGHOME="$HOME/.gnupg"
fi
safe_install_dir "$GNUPGHOME" 0700
maybe_chmod 0700 "$GNUPGHOME"

sync_host_gpg

# If a host gitconfig is mounted at ~/.gitconfig-host (read-only), clone it into
# a writable ~/.gitconfig inside the container so git config --global can update
# gpg.program without touching the host file.
if [ -f "$HOME/.gitconfig-host" ]; then
    if [ ! -f "$HOME/.gitconfig" ]; then
        cp "$HOME/.gitconfig-host" "$HOME/.gitconfig" 2>/dev/null || true
    fi
    chmod 600 "$HOME/.gitconfig" 2>/dev/null || true
fi

cache_ttl="${AIFO_GPG_CACHE_TTL_SECONDS:-7200}"
max_cache_ttl="${AIFO_GPG_CACHE_MAX_TTL_SECONDS:-86400}"
conf="$GNUPGHOME/gpg-agent.conf"
ensure_conf_line "$conf" "pinentry-program " "pinentry-program /usr/bin/pinentry-curses"
ensure_conf_line "$conf" "allow-loopback-pinentry" "allow-loopback-pinentry"
ensure_conf_line "$conf" "allow-preset-passphrase" "allow-preset-passphrase"
ensure_conf_line "$conf" "default-cache-ttl " "default-cache-ttl ${cache_ttl}"
ensure_conf_line "$conf" "max-cache-ttl " "max-cache-ttl ${max_cache_ttl}"

if [ -z "${XDG_RUNTIME_DIR:-}" ]; then
    export XDG_RUNTIME_DIR="/tmp/runtime-$(id -u)"
fi
mkdir -p "$XDG_RUNTIME_DIR" "$XDG_RUNTIME_DIR/gnupg"
chmod 0700 "$XDG_RUNTIME_DIR" "$XDG_RUNTIME_DIR/gnupg" 2>/dev/null || true

ensure_local_tree() {
    for d in "$HOME/.local" "$HOME/.local/share" "$HOME/.local/state" \
             "$HOME/.local/share/uv" "$HOME/.local/share/pnpm" "$HOME/.cache"; do
        safe_install_dir "$d" 0755
    done
}

ensure_local_tree

# Bootstrap gnupg configs
ensure_gpg_tty || unset GPG_TTY || true
unset GPG_AGENT_INFO || true
if [ "${AIFO_GPG_PRIMED:-0}" != "1" ]; then
    if command -v gpgconf >/dev/null 2>&1; then
        gpgconf --kill gpg-agent >/dev/null 2>&1 || true
        gpgconf --launch gpg-agent >/dev/null 2>&1 || true
    else
        gpg-agent --daemon >/dev/null 2>&1 || true
    fi
fi
refresh_gpg_agent_tty

configure_git_gpg_wrapper() {
    if ! command -v git >/dev/null 2>&1; then
        return
    fi
    current=$(git config --global --get gpg.program 2>/dev/null || true)
    if [ "${AIFO_DISABLE_GPG_LOOPBACK:-0}" = "1" ]; then
        if [ "$current" != "gpg" ]; then
            git config --global gpg.program gpg >/dev/null 2>&1 || true
        fi
        return
    fi
    if [ ! -x /usr/local/bin/aifo-gpg-wrapper ]; then
        return
    fi
    # For fullscreen agents (e.g., opencode), always force the loopback wrapper so that
    # later non-interactive git signing uses gpg --batch --pinentry-mode loopback and can
    # reuse the cached passphrase from gpg-agent without requiring a TTY.
    if is_fullscreen_agent; then
        git config --global gpg.program /usr/local/bin/aifo-gpg-wrapper >/dev/null 2>&1 || true
        return
    fi
    if [ "$current" != "/usr/local/bin/aifo-gpg-wrapper" ]; then
        git config --global gpg.program /usr/local/bin/aifo-gpg-wrapper >/dev/null 2>&1 || true
    fi
}

configure_git_gpg_wrapper
prime_gpg_agent_if_requested

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
    safe_install_dir "$dest_dir" 0700
    copy_with_mode "$mode" "$src" "$dest_dir/$base"
    maybe_chmod "$mode" "$dest_dir/$base"
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
            safe_install_dir "$sub" 0700
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
            copy_with_mode 0644 "$CFG_DST/aider/$f" "$HOME/$f"
        done
    fi
    if [ -d "$CFG_DST/crush" ]; then
        safe_install_dir "$HOME/.crush" 0700
        copy_tree_contents "$CFG_DST/crush" "$HOME/.crush"
    fi
    if [ -d "$CFG_DST/openhands" ]; then
        safe_install_dir "$HOME/.openhands" 0700
        copy_tree_contents "$CFG_DST/openhands" "$HOME/.openhands"
    fi
    if [ -d "$CFG_DST/opencode" ]; then
        safe_install_dir "$HOME/.config" 0700
        safe_install_dir "$HOME/.config/opencode" 0700
        copy_tree_contents "$CFG_DST/opencode" "$HOME/.config/opencode"
    fi
    if [ -d "$CFG_DST/plandex" ]; then
        safe_install_dir "$HOME/.plandex-home" 0700
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

    safe_install_dir "$CFG_DST" 0700

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
        if [ "$IS_ROOT" = "1" ]; then
            install -m 0600 /dev/null "$stamp" 2>/dev/null || :
        else
            :
        fi
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
            safe_install_dir "$link_dir" 0700
            ln -s "$real_conf" "$link_path" 2>/dev/null || true
        fi
    fi
fi

exec "$@"
