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
if [ "${AIFO_TOOLCHAIN_VERBOSE:-}" = "1" ]; then
  echo "aifo-shim: tool=$tool cwd=$cwd" >&2
  echo "aifo-shim: preparing request to ${AIFO_TOOLEEXEC_URL} (proto=2)" >&2
fi
tmp="${TMPDIR:-/tmp}/aifo-shim.$$"
mkdir -p "$tmp"
# Build curl form payload (urlencode all key=value pairs)
cmd=(curl -sS --no-buffer -D "$tmp/h" -X POST -H "Authorization: Bearer $AIFO_TOOLEEXEC_TOKEN" -H "X-Aifo-Proto: 2" -H "TE: trailers" -H "Content-Type: application/x-www-form-urlencoded")
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
