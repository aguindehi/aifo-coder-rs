/*!
Shim writer module: emits aifo-shim (curl-based v2 client) and tool symlinks.

The generated shim sends:
- Authorization: Bearer <token>
- X-Aifo-Proto: 2
- TE: trailers

It uses --data-urlencode for correct form encoding and supports Linux unix sockets.
*/
use std::fs;
use std::io;
use std::path::Path;

/// Full set of shim tools to write into the shim dir.
const SHIM_TOOLS: &[&str] = &[
    "cargo",
    "rustc",
    "node",
    "npm",
    "npx",
    "yarn",
    "pnpm",
    "deno",
    "tsc",
    "ts-node",
    "python",
    "pip",
    "pip3",
    "gcc",
    "g++",
    "cc",
    "c++",
    "clang",
    "clang++",
    "make",
    "cmake",
    "ninja",
    "pkg-config",
    "go",
    "gofmt",
    "say",
];

/// Expose shim tool list for tests and image checks.
pub fn shim_tool_names() -> &'static [&'static str] {
    SHIM_TOOLS
}

/// Write aifo-shim and tool wrappers into the given directory.
pub fn toolchain_write_shims(dir: &Path) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let shim_path = dir.join("aifo-shim");
    let shim = r#"#!/bin/sh
set -e

if [ -z "$AIFO_TOOLEEXEC_URL" ] || [ -z "$AIFO_TOOLEEXEC_TOKEN" ]; then
  echo "aifo-shim: proxy not configured. Please launch agent with --toolchain." >&2
  exit 86
fi

tool="$(basename "$0")"
cwd="$(pwd)"

# ExecId generation (prefer existing AIFO_EXEC_ID, else uuidgen, else time-pid)
exec_id="${AIFO_EXEC_ID:-}"
if [ -z "$exec_id" ]; then
  if command -v uuidgen >/dev/null 2>&1; then
    exec_id="$(uuidgen | tr 'A-Z' 'a-z' | tr -d '{}')"
  else
    exec_id="$(date +%s%N).$$"
  fi
fi

# Notification tools: early /notify path (POSIX curl)
# Allow overriding the list via AIFO_NOTIFY_TOOLS; default to "say"
NOTIFY_TOOLS="${AIFO_NOTIFY_TOOLS:-say}"
is_notify=0
for nt in $NOTIFY_TOOLS; do
  if [ "$tool" = "$nt" ]; then
    is_notify=1
    break
  fi
done
if [ "$is_notify" -eq 1 ]; then
  if [ "${AIFO_TOOLCHAIN_VERBOSE:-}" = "1" ] || [ "${AIFO_SHIM_NOTIFY_ASYNC:-1}" = "0" ]; then
    if [ "${AIFO_TOOLCHAIN_VERBOSE:-}" = "1" ]; then
      if [ "${AIFO_SHIM_LOG_VARIANT:-0}" = "1" ]; then
        echo "aifo-shim: variant=posix transport=curl"
      fi
      printf "aifo-shim: notify cmd=%s argv=%s client=posix-shim-curl\n" "$tool" "$*"
      echo "aifo-shim: preparing request to /notify (proto=2) client=posix-shim-curl"
    fi
    tmp="${TMPDIR:-/tmp}/aifo-shim.$$"
    mkdir -p "$tmp"
    cmd=(curl -sS -D "$tmp/h" -X POST -H "Authorization: Bearer $AIFO_TOOLEEXEC_TOKEN" -H "X-Aifo-Proto: 2" -H "X-Aifo-Client: posix-shim-curl" -H "Content-Type: application/x-www-form-urlencoded")
    if printf %s "$AIFO_TOOLEEXEC_URL" | grep -q '^unix://'; then
      SOCKET="${AIFO_TOOLEEXEC_URL#unix://}"
      cmd+=(--unix-socket "$SOCKET")
      URL="http://localhost/notify"
    else
      base="$AIFO_TOOLEEXEC_URL"
      base="${base%/exec}"
      URL="${base}/notify"
    fi
    cmd+=(--data-urlencode "cmd=$tool")
    for a in "$@"; do
      cmd+=(--data-urlencode "arg=$a")
    done
    if ! "${cmd[@]}"; then
      : # body printed by curl on error as well
    fi
    ec="$(awk '/^X-Exit-Code:/{print $2}' "$tmp/h" | tr -d '\r' | tail -n1)"
    rm -rf "$tmp"
    [ -n "$ec" ] || ec=1
    # In verbose mode, add a tiny delay to let proxy logs flush before returning
    if [ "${AIFO_TOOLCHAIN_VERBOSE:-}" = "1" ]; then
      delay="${AIFO_NOTIFY_EXIT_DELAY_SECS:-0.5}"
      awk "BEGIN { s=$delay+0; if (s>0) system(\"sleep \" s) }" >/dev/null 2>&1 || sleep 0.5
    fi
    exit "$ec"
  else
    # Non-verbose async: fire-and-forget notify request
    cmd=(curl -sS -X POST -H "Authorization: Bearer $AIFO_TOOLEEXEC_TOKEN" -H "X-Aifo-Proto: 2" -H "X-Aifo-Client: posix-shim-curl" -H "Content-Type: application/x-www-form-urlencoded")
    if printf %s "$AIFO_TOOLEEXEC_URL" | grep -q '^unix://'; then
      SOCKET="${AIFO_TOOLEEXEC_URL#unix://}"
      cmd+=(--unix-socket "$SOCKET")
      URL="http://localhost/notify"
    else
      base="$AIFO_TOOLEEXEC_URL"
      base="${base%/exec}"
      URL="${base}/notify"
    fi
    cmd+=(--data-urlencode "cmd=$tool")
    for a in "$@"; do
      cmd+=(--data-urlencode "arg=$a")
    done
    ( "${cmd[@]}" >/dev/null 2>&1 ) &
    disown 2>/dev/null || true
    exit 0
  fi
fi

# Signal forwarding helpers and traps
sigint_count=0
send_signal() {
  sig="$1"
  [ -z "$exec_id" ] && return 0
  scmd=(curl -sS -X POST -H "Authorization: Bearer $AIFO_TOOLEEXEC_TOKEN" -H "X-Aifo-Proto: 2" -H "Content-Type: application/x-www-form-urlencoded")
  if printf %s "$AIFO_TOOLEEXEC_URL" | grep -q '^unix://'; then
    SOCKET="${AIFO_TOOLEEXEC_URL#unix://}"
    scmd+=(--unix-socket "$SOCKET")
    SURL="http://localhost/signal"
  else
    base="$AIFO_TOOLEEXEC_URL"
    base="${base%/exec}"
    SURL="${base}/signal"
  fi
  scmd+=(--data-urlencode "exec_id=$exec_id" --data-urlencode "signal=$sig" "$SURL")
  "${scmd[@]}" >/dev/null 2>&1 || true
}
# Best-effort temp cleanup; safe if $tmp is empty/unset
cleanup() { [ -n "$tmp" ] && rm -rf "$tmp"; }
kill_parent_shell_if_interactive() {
  if [ "${AIFO_SHIM_KILL_PARENT_SHELL_ON_SIGINT:-1}" = "1" ] && { [ -t 0 ] || [ -t 1 ]; }; then
    p="$PPID"
    # Detect parent command name without ps if possible
    comm=""
    if [ -r "/proc/$p/comm" ]; then
      comm="$(tr -d '\r\n' < "/proc/$p/comm" 2>/dev/null || printf '')"
    elif command -v ps >/dev/null 2>&1; then
      comm="$(ps -o comm= -p "$p" 2>/dev/null | tr -d '\r\n' || printf '')"
    fi
    is_shell=0
    case "$comm" in
      sh|bash|dash|zsh|ksh|ash|busybox|busybox-sh) is_shell=1 ;;
    esac
    if [ "$is_shell" -eq 1 ]; then
      # Try graceful -> forceful sequence on parent shell; avoid wide PGID kills by default.
      # If parent is a group leader, also try signaling its PGID.
      pgid=""
      if [ -r "/proc/$p/stat" ]; then
        pgid="$(awk '{print $5}' "/proc/$p/stat" 2>/dev/null | tr -d ' \r\n')"
      elif command -v ps >/dev/null 2>&1; then
        pgid="$(ps -o pgid= -p "$p" 2>/dev/null | tr -d ' \r\n')"
      fi
      kill -HUP "$p" >/dev/null 2>&1 || true
      sleep 0.05
      kill -TERM "$p" >/dev/null 2>&1 || true
      sleep 0.05
      if [ -n "$pgid" ] && [ "$pgid" = "$p" ]; then
        kill -HUP -"$pgid" >/dev/null 2>&1 || true
        sleep 0.05
        kill -TERM -"$pgid" >/dev/null 2>&1 || true
        sleep 0.05
      fi
      kill -KILL "$p" >/dev/null 2>&1 || true
    fi
  fi
}
trap - INT
trap 'sigint_count=$((sigint_count+1)); if [ $sigint_count -eq 1 ]; then send_signal INT; cleanup; kill_parent_shell_if_interactive; if [ "${AIFO_SHIM_EXIT_ZERO_ON_SIGINT:-1}" = "1" ]; then exit 0; else exit 130; fi; elif [ $sigint_count -eq 2 ]; then send_signal TERM; cleanup; kill_parent_shell_if_interactive; if [ "${AIFO_SHIM_EXIT_ZERO_ON_SIGINT:-1}" = "1" ]; then exit 0; else exit 143; fi; else send_signal KILL; cleanup; kill_parent_shell_if_interactive; if [ "${AIFO_SHIM_EXIT_ZERO_ON_SIGINT:-1}" = "1" ]; then exit 0; else exit 137; fi; fi' INT
trap 'send_signal TERM; cleanup; kill_parent_shell_if_interactive; if [ "${AIFO_SHIM_EXIT_ZERO_ON_SIGINT:-1}" = "1" ]; then exit 0; else exit 143; fi' TERM
trap 'send_signal HUP; cleanup; kill_parent_shell_if_interactive; if [ "${AIFO_SHIM_EXIT_ZERO_ON_SIGINT:-1}" = "1" ]; then exit 0; else exit 129; fi' HUP
trap 'cleanup' EXIT

if [ "${AIFO_TOOLCHAIN_VERBOSE:-}" = "1" ]; then
  echo "aifo-shim: variant=posix transport=curl" >&2
  echo "aifo-shim: tool=$tool cwd=$cwd exec_id=$exec_id" >&2
  echo "aifo-shim: preparing request to ${AIFO_TOOLEEXEC_URL} (proto=2)" >&2
fi

tmp="${TMPDIR:-/tmp}/aifo-shim.$$"
mkdir -p "$tmp"

# Record agent container terminal foreground PGID for this exec to allow proxy to close the /run shell on disconnect.
d="$HOME/.aifo-exec/$exec_id"
mkdir -p "$d" 2>/dev/null || true
tpgid=""
if [ -r "/proc/$$/stat" ]; then
  tpgid="$(awk '{print $8}' "/proc/$$/stat" 2>/dev/null | tr -d ' \r\n')"
elif command -v ps >/dev/null 2>&1; then
  tpgid="$(ps -o tpgid= -p "$$" 2>/dev/null | tr -d ' \r\n')"
fi
if [ -n "$tpgid" ]; then printf "%s" "$tpgid" > "$d/agent_tpgid" 2>/dev/null || true; fi
printf "%s" "$PPID" > "$d/agent_ppid" 2>/dev/null || true
# Record controlling TTY path to help terminate the /run shell on disconnect (best-effort)
tty_link=""
if [ -t 0 ]; then
  tty_link="$(readlink -f "/proc/$$/fd/0" 2>/dev/null || true)"
elif [ -t 1 ]; then
  tty_link="$(readlink -f "/proc/$$/fd/1" 2>/dev/null || true)"
fi
if [ -n "$tty_link" ]; then printf "%s" "$tty_link" > "$d/tty" 2>/dev/null || true; fi
# Mark this TTY as protected from interactive fallback; wrapper will auto-exit.
touch "$d/no_shell_on_tty" 2>/dev/null || true

# Build curl form payload (urlencode all key=value pairs)
cmd=(curl -sS --no-buffer -D "$tmp/h" -X POST -H "Authorization: Bearer $AIFO_TOOLEEXEC_TOKEN" -H "X-Aifo-Proto: 2" -H "TE: trailers" -H "Content-Type: application/x-www-form-urlencoded" -H "X-Aifo-Exec-Id: $exec_id")
cmd+=(--data-urlencode "tool=$tool" --data-urlencode "cwd=$cwd")
# Append args preserving order
for a in "$@"; do
  cmd+=(--data-urlencode "arg=$a")
done

# Detect optional unix socket URL (Linux unix transport)
if printf %s "$AIFO_TOOLEEXEC_URL" | grep -q '^unix://'; then
  SOCKET="${AIFO_TOOLEEXEC_URL#unix://}"
  cmd+=(--unix-socket "$SOCKET")
  URL="http://localhost/exec"
else
  URL="$AIFO_TOOLEEXEC_URL"
fi

cmd+=("$URL")
disconnected=0
if ! "${cmd[@]}"; then
  disconnected=1
fi

ec="$(awk '/^X-Exit-Code:/{print $2}' "$tmp/h" | tr -d '\r' | tail -n1)"
: # body streamed directly by curl
# If the HTTP stream disconnected (e.g., Ctrl-C) or header is missing, give proxy logs a moment
# to flush (verbose mode), then terminate the transient parent shell to avoid lingering prompts.
if [ "$disconnected" -ne 0 ] || [ -z "$ec" ]; then
  if [ "${AIFO_TOOLCHAIN_VERBOSE:-}" = "1" ]; then
    echo "aifo-coder: disconnect, waiting for process termination..." >&2
    wait_secs="${AIFO_SHIM_DISCONNECT_WAIT_SECS:-1}"
    case "$wait_secs" in
      '' ) wait_secs=1 ;;
      *[!0-9]* ) wait_secs=1 ;;
    esac
    if [ "$wait_secs" -gt 0 ]; then
      sleep "$wait_secs"
    fi
    echo "aifo-coder: terminating now" >&2
    # Ensure the agent prompt appears on a fresh, clean line
    echo >&2
  fi
  kill_parent_shell_if_interactive
fi
# Resolve exit code: prefer header; on disconnect, default to 0 unless opted out.
if [ -z "$ec" ]; then
  if [ "$disconnected" -ne 0 ] && [ "${AIFO_SHIM_EXIT_ZERO_ON_DISCONNECT:-1}" = "1" ]; then
    ec=0
  else
    ec=1
  fi
fi
if [ "$disconnected" -eq 0 ] && [ -n "$ec" ]; then rm -rf "$d" 2>/dev/null || true; fi
rm -rf "$tmp"
exit "$ec"
"#;
    fs::write(&shim_path, shim)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&shim_path, fs::Permissions::from_mode(0o755))?;
    }
    for t in SHIM_TOOLS {
        let path = dir.join(t);
        fs::write(
            &path,
            "#!/bin/sh\nexec \"$(dirname \"$0\")/aifo-shim\" \"$@\"\n",
        )?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
        }
    }

    // Provide a 'sh' wrapper that ensures transient shells exit after running a command.
    // This prevents dropping into an interactive shell after '/run ...' completes or is interrupted.
    // Opt-out by setting AIFO_SH_WRAP_DISABLE=1 inside the agent container.
    let sh_wrap = r#"#!/bin/sh
# aifo-coder sh wrapper: auto-exit after -c/-lc commands and avoid lingering shells on Ctrl-C.
# Opt-out: AIFO_SH_WRAP_DISABLE=1
if [ "${AIFO_SH_WRAP_DISABLE:-0}" = "1" ]; then
  exec /bin/sh "$@"
fi

# If interactive and this TTY was used for a recent tool exec, exit immediately.
if { [ -t 0 ] || [ -t 1 ] || [ -t 2 ]; }; then
  TTY_PATH="$(readlink -f "/proc/$$/fd/0" 2>/dev/null || readlink -f "/proc/$$/fd/1" 2>/dev/null || readlink -f "/proc/$$/fd/2" 2>/dev/null || true)"
  NOW="$(date +%s)"
  RECENT="${AIFO_SH_RECENT_SECS:-10}"
  if [ -n "$TTY_PATH" ] && [ -d "$HOME/.aifo-exec" ]; then
    for d in "$HOME"/.aifo-exec/*; do
      [ -d "$d" ] || continue
      if [ -f "$d/no_shell_on_tty" ] && [ -f "$d/tty" ] && [ "$(cat "$d/tty" 2>/dev/null)" = "$TTY_PATH" ]; then
        MTIME="$(stat -c %Y "$d" 2>/dev/null || stat -f %m "$d" 2>/dev/null || echo 0)"
        AGE="$((NOW - MTIME))"
        if [ "$AGE" -le "$RECENT" ] 2>/dev/null; then exit 0; fi
      fi
    done
  fi
fi

# When invoked as sh -c "cmd" [...] or sh -lc "cmd" [...],
# append '; exit' so the shell terminates after the command finishes.
if [ "$#" -ge 2 ] && { [ "$1" = "-c" ] || [ "$1" = "-lc" ]; }; then
  flag="$1"
  cmd="$2"
  shift 2
  exec /bin/sh "$flag" "$cmd; exit" "$@"
fi

# Fallback: just chain to real /bin/sh
exec /bin/sh "$@"
"#;
    let sh_path = dir.join("sh");
    fs::write(&sh_path, sh_wrap)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&sh_path, fs::Permissions::from_mode(0o755))?;
    }
    // Provide bash and dash wrappers with the same auto-exit behavior
    let bash_wrap = sh_wrap.replace("/bin/sh", "/bin/bash");
    let dash_wrap = sh_wrap.replace("/bin/sh", "/bin/dash");
    let bash_path = dir.join("bash");
    fs::write(&bash_path, &bash_wrap)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&bash_path, fs::Permissions::from_mode(0o755))?;
    }
    let dash_path = dir.join("dash");
    fs::write(&dash_path, &dash_wrap)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&dash_path, fs::Permissions::from_mode(0o755))?;
    }

    Ok(())
}
