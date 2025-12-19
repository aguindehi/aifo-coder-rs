#![allow(clippy::module_name_repetitions)]

use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

const WORKSPACE_PREFIX: &str = "/workspace";

pub fn env_is_truthy(key: &str) -> bool {
    matches!(
        env::var(key).ok().as_deref(),
        Some("1" | "true" | "TRUE" | "yes" | "YES" | "on" | "ON")
    )
}

/// Best-effort absolute-ish path resolution:
/// - if arg begins with '/', keep as-is
/// - otherwise join with current working directory
pub fn resolve_program_path(program: &str) -> PathBuf {
    if program.starts_with('/') {
        PathBuf::from(program)
    } else {
        env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(program)
    }
}

/// Deterministic resolver for tests.
pub fn resolve_program_path_with_cwd(program: &str, cwd: &Path) -> PathBuf {
    if program.starts_with('/') {
        PathBuf::from(program)
    } else {
        cwd.join(program)
    }
}

pub fn is_under_workspace(p: &Path) -> bool {
    let s = p.to_string_lossy();
    s == WORKSPACE_PREFIX || s.starts_with(&format!("{WORKSPACE_PREFIX}/"))
}

/// Return the "main program" arg for `node` invocations, following the v1 rules:
/// - honor `--` separator
/// - skip known flags that consume an argument
/// - ignore eval/print/REPL (`-e/-p`) as “no program path” (proxy by default)
pub fn node_main_program_arg(argv: &[OsString]) -> Option<String> {
    let mut i = 1usize;
    while i < argv.len() {
        let a = argv[i].to_string_lossy().to_string();

        if a == "--" {
            if i + 1 < argv.len() {
                return Some(argv[i + 1].to_string_lossy().to_string());
            }
            return None;
        }

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

        if a == "-r"
            || a == "--require"
            || a == "--loader"
            || a == "--import"
            || a == "--eval-file"
            || a == "--inspect-port"
            || a == "--title"
        {
            i += 2;
            continue;
        }

        if a.starts_with("--require=")
            || a.starts_with("--loader=")
            || a.starts_with("--import=")
            || a.starts_with("--inspect-port=")
            || a.starts_with("--title=")
        {
            i += 1;
            continue;
        }

        if a.starts_with('-') {
            i += 1;
            continue;
        }

        return Some(a);
    }

    None
}

pub fn python_script_arg(argv: &[OsString]) -> Option<String> {
    let mut i = 1usize;
    while i < argv.len() {
        let a = argv[i].to_string_lossy().to_string();

        if a == "--" {
            if i + 1 < argv.len() {
                return Some(argv[i + 1].to_string_lossy().to_string());
            }
            return None;
        }

        // -m module: treat as local when smart python enabled
        if a == "-m" {
            return None;
        }
        if a.starts_with("-m") && a.len() > 2 {
            return None;
        }

        // Options that consume a following value; skip both.
        if a == "-c" || a == "-W" || a == "-X" {
            i += 2;
            continue;
        }

        if a.starts_with('-') {
            i += 1;
            continue;
        }

        return Some(a);
    }
    None
}

pub fn python_is_module_mode(argv: &[OsString]) -> bool {
    let mut i = 1usize;
    while i < argv.len() {
        let a = argv[i].to_string_lossy();
        if a == "--" {
            return false;
        }
        if a == "-m" {
            return true;
        }
        if a.starts_with("-m") && a.len() > 2 {
            return true;
        }
        i += 1;
    }
    false
}

pub fn tool_is_always_proxy(tool: &str) -> bool {
    matches!(tool, "pip" | "pip3" | "uv" | "uvx")
}
