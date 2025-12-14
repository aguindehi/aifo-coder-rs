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

use crate::ShellFile;

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
    "uv",
    "uvx",
];

/// Expose shim tool list for tests and image checks.
pub fn shim_tool_names() -> &'static [&'static str] {
    SHIM_TOOLS
}

fn build_posix_shim_script() -> io::Result<String> {
    // Keep semantics identical to the previous raw multi-line literal.
    ShellFile::new()
        .extend([
            "#!/bin/sh".to_string(),
            "set -e".to_string(),
            "".to_string(),
            r#"if [ -z "$AIFO_TOOLEEXEC_URL" ] || [ -z "$AIFO_TOOLEEXEC_TOKEN" ]; then"#.to_string(),
            r#"  echo "aifo-shim: proxy not configured. Please launch agent with --toolchain." >&2"#.to_string(),
            "  exit 86".to_string(),
            "fi".to_string(),
            "".to_string(),
            r#"tool="$(basename "$0")""#.to_string(),
            r#"cwd="$(pwd)""#.to_string(),
            "".to_string(),
            "# ExecId generation (prefer existing AIFO_EXEC_ID, else uuidgen, else time-pid)".to_string(),
            r#"exec_id="${AIFO_EXEC_ID:-}""#.to_string(),
            r#"if [ -z "$exec_id" ]; then"#.to_string(),
            r#"  if command -v uuidgen >/dev/null 2>&1; then"#.to_string(),
            r#"    exec_id="$(uuidgen | tr 'A-Z' 'a-z' | tr -d '{}')""#.to_string(),
            "  else".to_string(),
            r#"    exec_id="$(date +%s%N).$$""#.to_string(),
            "  fi".to_string(),
            "fi".to_string(),
            "".to_string(),
            "# Notification tools: early /notify path (POSIX curl)".to_string(),
            r#"# Allow overriding the list via AIFO_NOTIFY_TOOLS; default to "say""#.to_string(),
            r#"NOTIFY_TOOLS="${AIFO_NOTIFY_TOOLS:-say}""#.to_string(),
            "is_notify=0".to_string(),
            "for nt in $NOTIFY_TOOLS; do".to_string(),
            r#"  if [ "$tool" = "$nt" ]; then"#.to_string(),
            "    is_notify=1".to_string(),
            "    break".to_string(),
            "  fi".to_string(),
            "done".to_string(),
            r#"if [ "$is_notify" -eq 1 ]; then"#.to_string(),
            r#"  if [ "${AIFO_TOOLCHAIN_VERBOSE:-}" = "1" ] || [ "${AIFO_SHIM_NOTIFY_ASYNC:-1}" = "0" ]; then"#.to_string(),
            r#"    if [ "${AIFO_TOOLCHAIN_VERBOSE:-}" = "1" ]; then"#.to_string(),
            r#"      if [ "${AIFO_SHIM_LOG_VARIANT:-0}" = "1" ]; then"#.to_string(),
            r#"        echo "aifo-shim: variant=posix transport=curl""#.to_string(),
            "      fi".to_string(),
            r#"      printf "aifo-shim: notify cmd=%s argv=%s client=posix-shim-curl\n" "$tool" "$*""#.to_string(),
            r#"      echo "aifo-shim: preparing request to /notify (proto=2) client=posix-shim-curl""#.to_string(),
            "    fi".to_string(),
            r#"    tmp="${TMPDIR:-/tmp}/aifo-shim.$$""#.to_string(),
            r#"    mkdir -p "$tmp""#.to_string(),
            r#"    cmd=(curl -sS -D "$tmp/h" -X POST -H "Authorization: Bearer $AIFO_TOOLEEXEC_TOKEN" -H "X-Aifo-Proto: 2" -H "X-Aifo-Client: posix-shim-curl" -H "Content-Type: application/x-www-form-urlencoded")"#.to_string(),
            r#"    if [ -n "${TRACEPARENT:-}" ]; then"#.to_string(),
            r#"      cmd+=(-H "traceparent: $TRACEPARENT")"#.to_string(),
            "    fi".to_string(),
            r#"    if printf %s "$AIFO_TOOLEEXEC_URL" | grep -q '^unix://'; then"#.to_string(),
            r#"      SOCKET="${AIFO_TOOLEEXEC_URL#unix://}""#.to_string(),
            r#"      cmd+=(--unix-socket "$SOCKET")"#.to_string(),
            r#"      URL="http://localhost/notify""#.to_string(),
            "    else".to_string(),
            r#"      base="$AIFO_TOOLEEXEC_URL""#.to_string(),
            r#"      base="${base%/exec}""#.to_string(),
            r#"      URL="${base}/notify""#.to_string(),
            "    fi".to_string(),
            r#"    cmd+=(--data-urlencode "cmd=$tool")"#.to_string(),
            r#"    for a in "$@"; do"#.to_string(),
            r#"      cmd+=(--data-urlencode "arg=$a")"#.to_string(),
            "    done".to_string(),
            r#"    if ! "${cmd[@]}"; then"#.to_string(),
            r#"      : # body printed by curl on error as well"#.to_string(),
            "    fi".to_string(),
            r#"    ec="$(awk '/^X-Exit-Code:/{print $2}' "$tmp/h" | tr -d '\r' | tail -n1)""#.to_string(),
            r#"    rm -rf "$tmp""#.to_string(),
            r#"    [ -n "$ec" ] || ec=1"#.to_string(),
            r#"    # In verbose mode, add a tiny delay to let proxy logs flush before returning"#.to_string(),
            r#"    if [ "${AIFO_TOOLCHAIN_VERBOSE:-}" = "1" ]; then"#.to_string(),
            r#"      delay="${AIFO_NOTIFY_EXIT_DELAY_SECS:-0.5}""#.to_string(),
            r#"      awk "BEGIN { s=$delay+0; if (s>0) system(\"sleep \" s) }" >/dev/null 2>&1 || sleep 0.5"#.to_string(),
            "    fi".to_string(),
            r#"    exit "$ec""#.to_string(),
            "  else".to_string(),
            r#"    # Non-verbose async: fire-and-forget notify request"#.to_string(),
            r#"    cmd=(curl -sS -X POST -H "Authorization: Bearer $AIFO_TOOLEEXEC_TOKEN" -H "X-Aifo-Proto: 2" -H "X-Aifo-Client: posix-shim-curl" -H "Content-Type: application/x-www-form-urlencoded")"#.to_string(),
            r#"    if [ -n "${TRACEPARENT:-}" ]; then"#.to_string(),
            r#"      cmd+=(-H "traceparent: $TRACEPARENT")"#.to_string(),
            "    fi".to_string(),
            r#"    if printf %s "$AIFO_TOOLEEXEC_URL" | grep -q '^unix://'; then"#.to_string(),
            r#"      SOCKET="${AIFO_TOOLEEXEC_URL#unix://}""#.to_string(),
            r#"      cmd+=(--unix-socket "$SOCKET")"#.to_string(),
            r#"      URL="http://localhost/notify""#.to_string(),
            "    else".to_string(),
            r#"      base="$AIFO_TOOLEEXEC_URL""#.to_string(),
            r#"      base="${base%/exec}""#.to_string(),
            r#"      URL="${base}/notify""#.to_string(),
            "    fi".to_string(),
            r#"    cmd+=(--data-urlencode "cmd=$tool")"#.to_string(),
            r#"    for a in "$@"; do"#.to_string(),
            r#"      cmd+=(--data-urlencode "arg=$a")"#.to_string(),
            "    done".to_string(),
            r#"    ( "${cmd[@]}" >/dev/null 2>&1 ) &"#.to_string(),
            r#"    disown 2>/dev/null || true"#.to_string(),
            "    exit 0".to_string(),
            "  fi".to_string(),
            "fi".to_string(),
            "".to_string(),
            "# Signal forwarding helpers and traps".to_string(),
            "sigint_count=0".to_string(),
            "send_signal() {".to_string(),
            r#"  sig="$1""#.to_string(),
            r#"  [ -z "$exec_id" ] && return 0"#.to_string(),
            r#"  scmd=(curl -sS -X POST -H "Authorization: Bearer $AIFO_TOOLEEXEC_TOKEN" -H "X-Aifo-Proto: 2" -H "Content-Type: application/x-www-form-urlencoded")"#.to_string(),
            r#"  if [ -n "${TRACEPARENT:-}" ]; then"#.to_string(),
            r#"    scmd+=(-H "traceparent: $TRACEPARENT")"#.to_string(),
            "  fi".to_string(),
            r#"  if printf %s "$AIFO_TOOLEEXEC_URL" | grep -q '^unix://'; then"#.to_string(),
            r#"    SOCKET="${AIFO_TOOLEEXEC_URL#unix://}""#.to_string(),
            r#"    scmd+=(--unix-socket "$SOCKET")"#.to_string(),
            r#"    SURL="http://localhost/signal""#.to_string(),
            "  else".to_string(),
            r#"    base="$AIFO_TOOLEEXEC_URL""#.to_string(),
            r#"    base="${base%/exec}""#.to_string(),
            r#"    SURL="${base}/signal""#.to_string(),
            "  fi".to_string(),
            r#"  scmd+=(--data-urlencode "exec_id=$exec_id" --data-urlencode "signal=$sig" "$SURL")"#.to_string(),
            r#"  "${scmd[@]}" >/dev/null 2>&1 || true"#.to_string(),
            "}".to_string(),
            r#"# Best-effort temp cleanup; safe if $tmp is empty/unset"#.to_string(),
            r#"cleanup() { [ -n "$tmp" ] && rm -rf "$tmp"; }"#.to_string(),
            "kill_parent_shell_if_interactive() {".to_string(),
            r#"  if [ "${AIFO_SHIM_KILL_PARENT_SHELL_ON_SIGINT:-1}" = "1" ] && { [ -t 0 ] || [ -t 1 ]; }; then"#.to_string(),
            r#"    p="$PPID""#.to_string(),
            r#"    # Detect parent command name without ps if possible"#.to_string(),
            r#"    comm=""#.to_string(),
            r#"    if [ -r "/proc/$p/comm" ]; then"#.to_string(),
            r#"      comm="$(tr -d '\r\n' < "/proc/$p/comm" 2>/dev/null || printf '')""#.to_string(),
            r#"    elif command -v ps >/dev/null 2>&1; then"#.to_string(),
            r#"      comm="$(ps -o comm= -p "$p" 2>/dev/null | tr -d '\r\n' || printf '')""#.to_string(),
            "    fi".to_string(),
            "    is_shell=0".to_string(),
            "    case \"$comm\" in".to_string(),
            r#"      sh|bash|dash|zsh|ksh|ash|busybox|busybox-sh) is_shell=1 ;;"#.to_string(),
            "    esac".to_string(),
            r#"    if [ "$is_shell" -eq 1 ]; then"#.to_string(),
            r#"      # Try graceful -> forceful sequence on parent shell; avoid wide PGID kills by default."#.to_string(),
            r#"      # If parent is a group leader, also try signaling its PGID."#.to_string(),
            r#"      pgid=""#.to_string(),
            r#"      if [ -r "/proc/$p/stat" ]; then"#.to_string(),
            r#"        pgid="$(awk '{print $5}' "/proc/$p/stat" 2>/dev/null | tr -d ' \r\n')""#.to_string(),
            r#"      elif command -v ps >/dev/null 2>&1; then"#.to_string(),
            r#"        pgid="$(ps -o pgid= -p "$p" 2>/dev/null | tr -d ' \r\n')""#.to_string(),
            "      fi".to_string(),
            r#"      kill -HUP "$p" >/dev/null 2>&1 || true"#.to_string(),
            "      sleep 0.05".to_string(),
            r#"      kill -TERM "$p" >/dev/null 2>&1 || true"#.to_string(),
            "      sleep 0.05".to_string(),
            r#"      if [ -n "$pgid" ] && [ "$pgid" = "$p" ]; then"#.to_string(),
            r#"        kill -HUP -"$pgid" >/dev/null 2>&1 || true"#.to_string(),
            "        sleep 0.05".to_string(),
            r#"        kill -TERM -"$pgid" >/dev/null 2>&1 || true"#.to_string(),
            "        sleep 0.05".to_string(),
            "      fi".to_string(),
            r#"      kill -KILL "$p" >/dev/null 2>&1 || true"#.to_string(),
            "    fi".to_string(),
            "  fi".to_string(),
            "}".to_string(),
            "trap - INT".to_string(),
            r#"trap 'sigint_count=$((sigint_count+1)); if [ $sigint_count -eq 1 ]; then send_signal INT; cleanup; kill_parent_shell_if_interactive; if [ "${AIFO_SHIM_EXIT_ZERO_ON_SIGINT:-1}" = "1" ]; then exit 0; else exit 130; fi; elif [ $sigint_count -eq 2 ]; then send_signal TERM; cleanup; kill_parent_shell_if_interactive; if [ "${AIFO_SHIM_EXIT_ZERO_ON_SIGINT:-1}" = "1" ]; then exit 0; else exit 143; fi; else send_signal KILL; cleanup; kill_parent_shell_if_interactive; if [ "${AIFO_SHIM_EXIT_ZERO_ON_SIGINT:-1}" = "1" ]; then exit 0; else exit 137; fi; fi' INT"#.to_string(),
            r#"trap 'send_signal TERM; cleanup; kill_parent_shell_if_interactive; if [ "${AIFO_SHIM_EXIT_ZERO_ON_SIGINT:-1}" = "1" ]; then exit 0; else exit 143; fi' TERM"#.to_string(),
            r#"trap 'send_signal HUP; cleanup; kill_parent_shell_if_interactive; if [ "${AIFO_SHIM_EXIT_ZERO_ON_SIGINT:-1}" = "1" ]; then exit 0; else exit 129; fi' HUP"#.to_string(),
            r#"trap 'cleanup' EXIT"#.to_string(),
            "".to_string(),
            r#"if [ "${AIFO_TOOLCHAIN_VERBOSE:-}" = "1" ]; then"#.to_string(),
            r#"  echo "aifo-shim: variant=posix transport=curl" >&2"#.to_string(),
            r#"  echo "aifo-shim: tool=$tool cwd=$cwd exec_id=$exec_id" >&2"#.to_string(),
            r#"  echo "aifo-shim: preparing request to ${AIFO_TOOLEEXEC_URL} (proto=2)" >&2"#.to_string(),
            "fi".to_string(),
            "".to_string(),
            r#"tmp="${TMPDIR:-/tmp}/aifo-shim.$$""#.to_string(),
            r#"mkdir -p "$tmp""#.to_string(),
            "".to_string(),
            r#"# Record agent container terminal foreground PGID for this exec to allow proxy to close the /run shell on disconnect."#.to_string(),
            r#"d="$HOME/.aifo-exec/$exec_id""#.to_string(),
            r#"mkdir -p "$d" 2>/dev/null || true"#.to_string(),
            r#"tpgid=""#.to_string(),
            r#"if [ -r "/proc/$$/stat" ]; then"#.to_string(),
            r#"  tpgid="$(awk '{print $8}' "/proc/$$/stat" 2>/dev/null | tr -d ' \r\n')""#.to_string(),
            r#"elif command -v ps >/dev/null 2>&1; then"#.to_string(),
            r#"  tpgid="$(ps -o tpgid= -p "$$" 2>/dev/null | tr -d ' \r\n')""#.to_string(),
            "fi".to_string(),
            r#"if [ -n "$tpgid" ]; then printf "%s" "$tpgid" > "$d/agent_tpgid" 2>/dev/null || true; fi"#.to_string(),
            r#"printf "%s" "$PPID" > "$d/agent_ppid" 2>/dev/null || true"#.to_string(),
            r#"# Record controlling TTY path to help terminate the /run shell on disconnect (best-effort)"#.to_string(),
            r#"tty_link=""#.to_string(),
            r#"if [ -t 0 ]; then"#.to_string(),
            r#"  tty_link="$(readlink -f "/proc/$$/fd/0" 2>/dev/null || true)""#.to_string(),
            r#"elif [ -t 1 ]; then"#.to_string(),
            r#"  tty_link="$(readlink -f "/proc/$$/fd/1" 2>/dev/null || true)""#.to_string(),
            "fi".to_string(),
            r#"if [ -n "$tty_link" ]; then printf "%s" "$tty_link" > "$d/tty" 2>/dev/null || true; fi"#.to_string(),
            r#"# Mark this TTY as protected from interactive fallback; wrapper will auto-exit."#.to_string(),
            r#"touch "$d/no_shell_on_tty" 2>/dev/null || true"#.to_string(),
            "".to_string(),
            r#"# Build curl form payload (urlencode all key=value pairs)"#.to_string(),
            r#"cmd=(curl -sS --no-buffer -D "$tmp/h" -X POST -H "Authorization: Bearer $AIFO_TOOLEEXEC_TOKEN" -H "X-Aifo-Proto: 2" -H "TE: trailers" -H "Content-Type: application/x-www-form-urlencoded" -H "X-Aifo-Exec-Id: $exec_id")"#.to_string(),
            r#"if [ -n "${TRACEPARENT:-}" ]; then"#.to_string(),
            r#"  cmd+=(-H "traceparent: $TRACEPARENT")"#.to_string(),
            "fi".to_string(),
            r#"cmd+=(--data-urlencode "tool=$tool" --data-urlencode "cwd=$cwd")"#.to_string(),
            r#"# Append args preserving order"#.to_string(),
            r#"for a in "$@"; do"#.to_string(),
            r#"  cmd+=(--data-urlencode "arg=$a")"#.to_string(),
            "done".to_string(),
            "".to_string(),
            r#"# Detect optional unix socket URL (Linux unix transport)"#.to_string(),
            r#"if printf %s "$AIFO_TOOLEEXEC_URL" | grep -q '^unix://'; then"#.to_string(),
            r#"  SOCKET="${AIFO_TOOLEEXEC_URL#unix://}""#.to_string(),
            r#"  cmd+=(--unix-socket "$SOCKET")"#.to_string(),
            r#"  URL="http://localhost/exec""#.to_string(),
            "else".to_string(),
            r#"  URL="$AIFO_TOOLEEXEC_URL""#.to_string(),
            "fi".to_string(),
            "".to_string(),
            r#"cmd+=("$URL")"#.to_string(),
            "disconnected=0".to_string(),
            r#"if ! "${cmd[@]}"; then"#.to_string(),
            "  disconnected=1".to_string(),
            "fi".to_string(),
            "".to_string(),
            r#"ec="$(awk '/^X-Exit-Code:/{print $2}' "$tmp/h" | tr -d '\r' | tail -n1)""#.to_string(),
            r#": # body streamed directly by curl"#.to_string(),
            r#"# If the HTTP stream disconnected (e.g., Ctrl-C) or header is missing, give proxy logs a moment"#.to_string(),
            r#"# to flush (verbose mode), then terminate the transient parent shell to avoid lingering prompts."#.to_string(),
            r#"if [ "$disconnected" -ne 0 ] || [ -z "$ec" ]; then"#.to_string(),
            r#"  if [ "${AIFO_TOOLCHAIN_VERBOSE:-}" = "1" ]; then"#.to_string(),
            r#"    echo "aifo-coder: disconnect, waiting for process termination..." >&2"#.to_string(),
            r#"    wait_secs="${AIFO_SHIM_DISCONNECT_WAIT_SECS:-1}""#.to_string(),
            "    case \"$wait_secs\" in".to_string(),
            r#"      '' ) wait_secs=1 ;;"#.to_string(),
            r#"      *[!0-9]* ) wait_secs=1 ;;"#.to_string(),
            "    esac".to_string(),
            r#"    if [ "$wait_secs" -gt 0 ]; then"#.to_string(),
            r#"      sleep "$wait_secs""#.to_string(),
            "    fi".to_string(),
            r#"    echo "aifo-coder: terminating now" >&2"#.to_string(),
            r#"    # Ensure the agent prompt appears on a fresh, clean line"#.to_string(),
            r#"    echo >&2"#.to_string(),
            "  fi".to_string(),
            "  kill_parent_shell_if_interactive".to_string(),
            "fi".to_string(),
            r#"# Resolve exit code: prefer header; on disconnect, default to 0 unless opted out."#.to_string(),
            r#"if [ -z "$ec" ]; then"#.to_string(),
            r#"  if [ "$disconnected" -ne 0 ] && [ "${AIFO_SHIM_EXIT_ZERO_ON_DISCONNECT:-1}" = "1" ]; then"#.to_string(),
            "    ec=0".to_string(),
            "  else".to_string(),
            "    ec=1".to_string(),
            "  fi".to_string(),
            "fi".to_string(),
            r#"if [ "$disconnected" -eq 0 ] && [ -n "$ec" ]; then rm -rf "$d" 2>/dev/null || true; fi"#.to_string(),
            r#"rm -rf "$tmp""#.to_string(),
            r#"exit "$ec""#.to_string(),
        ])
        .build()
}

fn build_sh_wrapper_script() -> io::Result<String> {
    ShellFile::new()
        .extend([
            "#!/bin/sh".to_string(),
            "# aifo-coder sh wrapper: auto-exit after -c/-lc commands and avoid lingering shells on Ctrl-C.".to_string(),
            "# Opt-out: AIFO_SH_WRAP_DISABLE=1".to_string(),
            r#"if [ "${AIFO_SH_WRAP_DISABLE:-0}" = "1" ]; then"#.to_string(),
            r#"  exec /bin/sh "$@""#.to_string(),
            "fi".to_string(),
            "".to_string(),
            "# If interactive and this TTY was used for a recent tool exec, exit immediately.".to_string(),
            r#"if { [ -t 0 ] || [ -t 1 ] || [ -t 2 ]; }; then"#.to_string(),
            r#"  TTY_PATH="$(readlink -f "/proc/$$/fd/0" 2>/dev/null || readlink -f "/proc/$$/fd/1" 2>/dev/null || readlink -f "/proc/$$/fd/2" 2>/dev/null || true)""#.to_string(),
            r#"  NOW="$(date +%s)""#.to_string(),
            r#"  RECENT="${AIFO_SH_RECENT_SECS:-10}""#.to_string(),
            r#"  if [ -n "$TTY_PATH" ] && [ -d "$HOME/.aifo-exec" ]; then"#.to_string(),
            r#"    for d in "$HOME"/.aifo-exec/*; do"#.to_string(),
            r#"      [ -d "$d" ] || continue"#.to_string(),
            r#"      if [ -f "$d/no_shell_on_tty" ] && [ -f "$d/tty" ] && [ "$(cat "$d/tty" 2>/dev/null)" = "$TTY_PATH" ]; then"#.to_string(),
            r#"        MTIME="$(stat -c %Y "$d" 2>/dev/null || stat -f %m "$d" 2>/dev/null || echo 0)""#.to_string(),
            r#"        AGE="$((NOW - MTIME))""#.to_string(),
            r#"        if [ "$AGE" -le "$RECENT" ] 2>/dev/null; then exit 0; fi"#.to_string(),
            "      fi".to_string(),
            "    done".to_string(),
            "  fi".to_string(),
            "fi".to_string(),
            "".to_string(),
            "# Normalize '-lc' to '-c' for POSIX shells; do not append '; exit'".to_string(),
            r#"if [ "$#" -ge 2 ] && { [ "$1" = "-c" ] || [ "$1" = "-lc" ]; }; then"#.to_string(),
            r#"  flag="$1""#.to_string(),
            r#"  cmd="$2""#.to_string(),
            "  shift 2".to_string(),
            r#"  [ "$flag" = "-lc" ] && flag="-c""#.to_string(),
            r#"  exec /bin/sh "$flag" "$cmd" "$@""#.to_string(),
            "fi".to_string(),
            "".to_string(),
            "# Fallback: just chain to real /bin/sh".to_string(),
            r#"exec /bin/sh "$@""#.to_string(),
        ])
        .build()
}

pub fn toolchain_write_shims(dir: &Path) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let shim_path = dir.join("aifo-shim");

    let shim = build_posix_shim_script()?;
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
    let sh_wrap = build_sh_wrapper_script()?;
    let sh_path = dir.join("sh");
    fs::write(&sh_path, sh_wrap)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&sh_path, fs::Permissions::from_mode(0o755))?;
    }
    // Provide bash and dash wrappers with the same auto-exit behavior
    let sh_wrap_text = build_sh_wrapper_script()?;
    let bash_wrap = sh_wrap_text.replace("/bin/sh", "/bin/bash");
    let dash_wrap = sh_wrap_text.replace("/bin/sh", "/bin/dash");
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ShellFile;

    #[test]
    fn test_shell_file_rejects_embedded_newlines() {
        let mut sf = ShellFile::new();
        sf.push("ok".to_string());
        sf.push("bad\nline".to_string());
        let err = sf.build().expect_err("expected invalid input");
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn test_build_posix_shim_script_smoke() {
        let s = build_posix_shim_script().expect("build shim");
        assert!(s.starts_with("#!/bin/sh\n"));
        assert!(s.contains("X-Aifo-Proto: 2"));
        assert!(s.contains("TE: trailers"));
        assert!(s.ends_with('\n'));
    }

    #[test]
    fn test_build_sh_wrapper_script_smoke() {
        let s = build_sh_wrapper_script().expect("build wrapper");
        assert!(s.starts_with("#!/bin/sh\n"));
        assert!(s.contains("AIFO_SH_WRAP_DISABLE"));
        assert!(s.ends_with('\n'));
    }
}
