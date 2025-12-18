/*!
Shim writer module: emits tool shims/wrappers under /opt/aifo/bin.

Phase 2 (smart shims v2): the authoritative shim implementation is the Rust binary
`aifo-shim` (fused smart routing + proxy protocol). Therefore this module must NOT
generate a proxy-implementing POSIX `aifo-shim` script anymore.

This module only writes:
- tool entrypoint wrappers (node/python/pip/...) that exec the local `aifo-shim` binary
- optional shell wrappers (sh/bash/dash) for session UX

The actual proxy protocol semantics live in `src/bin/aifo-shim.rs` and must be kept
single-source-of-truth.
*/
use std::fs;
use std::io;
use std::path::Path;

use crate::{ShellFile, TextLines};

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

    // Phase 2 (smart shims v2): never install a proxy-implementing POSIX `aifo-shim`.
    // The shim directory is expected to contain the Rust `aifo-shim` binary, either
    // from the agent image (preferred) or from an explicit AIFO_SHIM_DIR mount.
    //
    // We only ensure tool wrappers exist and point at the (already-present) `aifo-shim`.
    if !shim_path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "aifo-shim binary not found at {} (POSIX shim generation is disabled in v2)",
                shim_path.display()
            ),
        ));
    }

    for t in SHIM_TOOLS {
        let path = dir.join(t);
        let wrapper = TextLines::new()
            .extend([
                "#!/bin/sh".to_string(),
                r#"exec "$(dirname "$0")/aifo-shim" "$@""#.to_string(),
            ])
            .build_lf()?;
        fs::write(&path, wrapper)?;
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
    use crate::{ShellFile, TextLines};

    #[test]
    fn test_shell_file_rejects_embedded_newlines() {
        let mut sf = ShellFile::new();
        sf.push("ok".to_string());
        sf.push("bad\nline".to_string());
        let err = sf.build().expect_err("expected invalid input");
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn test_text_lines_rejects_embedded_newlines() {
        let mut tl = TextLines::new();
        tl.push("ok".to_string());
        tl.push("bad\nline".to_string());
        let err = tl.build_lf().expect_err("expected invalid input");
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn test_build_sh_wrapper_script_smoke() {
        let s = build_sh_wrapper_script().expect("build wrapper");
        assert!(s.starts_with("#!/bin/sh\n"));
        assert!(s.contains("AIFO_SH_WRAP_DISABLE"));
        assert!(s.ends_with('\n'));
    }
}
