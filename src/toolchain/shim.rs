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
    "notifications-cmd",
];

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
trap 'sigint_count=$((sigint_count+1)); if [ $sigint_count -eq 1 ]; then send_signal INT; cleanup; if [ "${AIFO_SHIM_EXIT_ZERO_ON_SIGINT:-1}" = "1" ]; then exit 0; else exit 130; fi; elif [ $sigint_count -eq 2 ]; then send_signal TERM; cleanup; if [ "${AIFO_SHIM_EXIT_ZERO_ON_SIGINT:-1}" = "1" ]; then exit 0; else exit 143; fi; else send_signal KILL; cleanup; if [ "${AIFO_SHIM_EXIT_ZERO_ON_SIGINT:-1}" = "1" ]; then exit 0; else exit 137; fi; fi' INT
trap 'send_signal TERM; cleanup; if [ "${AIFO_SHIM_EXIT_ZERO_ON_SIGINT:-1}" = "1" ]; then exit 0; else exit 143; fi' TERM
trap 'send_signal HUP; cleanup; if [ "${AIFO_SHIM_EXIT_ZERO_ON_SIGINT:-1}" = "1" ]; then exit 0; else exit 129; fi' HUP
trap 'cleanup' EXIT

if [ "${AIFO_TOOLCHAIN_VERBOSE:-}" = "1" ]; then
  echo "aifo-shim: tool=$tool cwd=$cwd exec_id=$exec_id" >&2
  echo "aifo-shim: preparing request to ${AIFO_TOOLEEXEC_URL} (proto=2)" >&2
fi

tmp="${TMPDIR:-/tmp}/aifo-shim.$$"
mkdir -p "$tmp"

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
"${cmd[@]}" || true

ec="$(awk '/^X-Exit-Code:/{print $2}' "$tmp/h" | tr -d '\r' | tail -n1)"
: # body streamed directly by curl
rm -rf "$tmp"
# Fallback to 1 if header missing
case "$ec" in '' ) ec=1 ;; esac
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
    Ok(())
}
