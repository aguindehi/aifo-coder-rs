#![allow(clippy::module_name_repetitions)]

use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const WORKSPACE_PREFIX: &str = "/workspace";

fn env_is_truthy(key: &str) -> bool {
    match env::var(key).ok().as_deref() {
        Some("1" | "true" | "TRUE" | "yes" | "YES" | "on" | "ON") => true,
        _ => false,
    }
}

fn verbose_enabled() -> bool {
    env::var("AIFO_TOOLCHAIN_VERBOSE").ok().as_deref() == Some("1")
}

fn tool_name_from_argv0() -> Option<String> {
    env::args_os()
        .next()
        .and_then(|p| Path::new(&p).file_name().map(|s| s.to_string_lossy().to_string()))
}

/// Best-effort absolute-ish path resolution:
/// - if arg begins with '/', keep as-is
/// - otherwise join with current working directory
fn resolve_program_path(program: &str) -> PathBuf {
    if program.starts_with('/') {
        PathBuf::from(program)
    } else {
        env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(program)
    }
}

fn is_under_workspace(p: &Path) -> bool {
    let s = p.to_string_lossy();
    s == WORKSPACE_PREFIX || s.starts_with(&format!("{WORKSPACE_PREFIX}/"))
}

fn pick_local_node_path() -> Option<&'static str> {
    for p in ["/usr/local/bin/node", "/usr/bin/node"] {
        if Path::new(p).is_file() {
            return Some(p);
        }
    }
    None
}

/// Return the "main program" arg for `node` invocations, following the v1 rules:
/// - honor `--` separator
/// - skip known flags that consume an argument
/// - otherwise first non-flag token is treated as program path
fn node_main_program_arg(argv: &[OsString]) -> Option<String> {
    // argv includes argv0 at index 0
    let mut i = 1usize;
    while i < argv.len() {
        let a = argv[i].to_string_lossy().to_string();

        if a == "--" {
            if i + 1 < argv.len() {
                return Some(argv[i + 1].to_string_lossy().to_string());
            }
            return None;
        }

        // Flags that mean "no program path" (REPL/eval). Keep proxied in v1.
        if a == "-e"
            || a == "--eval"
            || a == "-p"
            || a == "--print"
            || a == "-h"
            || a == "--help"
            || a == "-v"
            || a == "--version"
        {
            return None;
        }

        // Flags that consume the next value
        if a == "-r"
            || a == "--require"
            || a == "--loader"
            || a == "--import"
            || a == "--eval-file"
        {
            i += 2;
            continue;
        }

        // Generic flag (does not identify program)
        if a.starts_with('-') {
            i += 1;
            continue;
        }

        // First non-flag token is treated as program
        return Some(a);
    }

    None
}

fn log_smart_line(tool: &str, reason: &str, program: Option<&Path>, local_bin: Option<&str>) {
    if !verbose_enabled() {
        return;
    }
    let mut msg = format!("aifo-shim: smart: tool={tool} mode=local reason={reason}");
    if let Some(p) = program {
        msg.push_str(&format!(" program={}", p.display()));
    }
    if let Some(b) = local_bin {
        msg.push_str(&format!(" local={b}"));
    }
    eprintln!("{msg}");
}

fn should_smart_local_node(tool: &str, argv: &[OsString]) -> Option<PathBuf> {
    if tool != "node" {
        return None;
    }
    if !env_is_truthy("AIFO_SHIM_SMART") || !env_is_truthy("AIFO_SHIM_SMART_NODE") {
        return None;
    }

    let program = node_main_program_arg(argv)?;
    let p = resolve_program_path(&program);

    if !is_under_workspace(&p) {
        return Some(p);
    }

    None
}

fn exec_local(local_bin: &str, argv: &[OsString]) -> ExitCode {
    // argv[0] is the shim name; replace it with the real runtime path.
    let mut cmd = Command::new(local_bin);
    if argv.len() > 1 {
        cmd.args(&argv[1..]);
    }
    let status = cmd.status().unwrap_or_else(|_| std::process::ExitStatus::from_raw(1));
    ExitCode::from(status.code().unwrap_or(1) as u8)
}

fn main() -> ExitCode {
    let argv: Vec<OsString> = env::args_os().collect();
    let Some(tool) = tool_name_from_argv0() else {
        eprintln!("aifo-shim: could not determine tool name");
        return ExitCode::from(1);
    };

    if let Some(program) = should_smart_local_node(&tool, &argv) {
        let Some(local) = pick_local_node_path() else {
            // If smart is enabled but we cannot find local node, fall back to proxy
            // by exiting with a distinct error (proxy wrapper will handle it).
            if verbose_enabled() {
                eprintln!("aifo-shim: smart: tool=node wanted local but no local node found");
            }
            // Proxy behavior is implemented elsewhere; returning 86 keeps parity with
            // "proxy not configured" style errors.
            return ExitCode::from(86);
        };

        log_smart_line("node", "outside-workspace", Some(&program), Some(local));
        // Best-effort direct local exec; do not proxy signals.
        let mut cmd = Command::new(local);
        if argv.len() > 1 {
            cmd.args(&argv[1..]);
        }
        let status = match cmd.status() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("aifo-shim: failed to exec local node: {e}");
                return ExitCode::from(1);
            }
        };
        return ExitCode::from(status.code().unwrap_or(1) as u8);
    }

    // Phase 2 only: if we didn't take local path, keep existing behavior.
    // This file is a placeholder shim main; the actual proxy behavior must already exist
    // in the environment's /opt/aifo/bin/aifo-shim script/binary.
    //
    // If this binary is invoked directly without proxy wiring, fail clearly.
    eprintln!("aifo-shim: proxy behavior not implemented in this build");
    ExitCode::from(86)
}
