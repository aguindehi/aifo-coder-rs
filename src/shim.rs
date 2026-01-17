#![allow(clippy::module_name_repetitions)]

use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Component, Path, PathBuf};

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

fn lexical_normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in p.components() {
        match comp {
            Component::ParentDir => {
                let _ = out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

pub fn best_effort_canonicalize(p: &Path) -> PathBuf {
    fs::canonicalize(p).unwrap_or_else(|_| lexical_normalize(p))
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PythonKind {
    Script,
    Module,
    Command,
    Stdin,
    Repl,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PythonInvocation {
    pub kind: PythonKind,
    pub target: Option<PathBuf>,
    pub module: Option<String>,
}

impl PythonInvocation {
    pub fn target_in_workspace(&self) -> bool {
        self.target
            .as_ref()
            .map(|p| is_under_workspace(p))
            .unwrap_or(false)
    }

    pub fn module_is_pip_like(&self) -> bool {
        self.module
            .as_ref()
            .map(|m| matches!(m.as_str(), "pip" | "pip3" | "uv" | "uvx") || m.starts_with("pip3."))
            .unwrap_or(false)
    }
}

pub fn python_invocation(argv: &[OsString], cwd: Option<&Path>) -> PythonInvocation {
    let cwd_fallback = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let base_cwd = cwd.unwrap_or(&cwd_fallback);

    let mut i = 1usize;

    while i < argv.len() {
        let a = argv[i].to_string_lossy().to_string();

        if a == "--" {
            if i + 1 < argv.len() {
                let script = argv[i + 1].to_string_lossy().to_string();
                let resolved = resolve_program_path_with_cwd(&script, base_cwd);
                let target = best_effort_canonicalize(&resolved);
                return PythonInvocation {
                    kind: PythonKind::Script,
                    target: Some(target),
                    module: None,
                };
            }
            break;
        }

        if a == "-m" {
            let module = argv.get(i + 1).map(|v| v.to_string_lossy().to_string());
            return PythonInvocation {
                kind: PythonKind::Module,
                target: Some(best_effort_canonicalize(base_cwd)),
                module,
            };
        }
        if a.starts_with("-m") && a.len() > 2 {
            let module = Some(a[2..].to_string());
            return PythonInvocation {
                kind: PythonKind::Module,
                target: Some(best_effort_canonicalize(base_cwd)),
                module,
            };
        }

        if a == "-c" {
            return PythonInvocation {
                kind: PythonKind::Command,
                target: Some(best_effort_canonicalize(base_cwd)),
                module: None,
            };
        }

        if a == "-" {
            return PythonInvocation {
                kind: PythonKind::Stdin,
                target: Some(best_effort_canonicalize(base_cwd)),
                module: None,
            };
        }

        if a == "-W" || a == "-X" {
            i += 2;
            continue;
        }

        if a.starts_with('-') {
            i += 1;
            continue;
        }

        let resolved = resolve_program_path_with_cwd(&a, base_cwd);
        let target = best_effort_canonicalize(&resolved);
        return PythonInvocation {
            kind: PythonKind::Script,
            target: Some(target),
            module: None,
        };
    }

    PythonInvocation {
        kind: PythonKind::Repl,
        target: Some(best_effort_canonicalize(base_cwd)),
        module: None,
    }
}

pub fn uvx_has_from_flag(argv: &[OsString]) -> bool {
    let mut i = 1usize;
    while i < argv.len() {
        let a = argv[i].to_string_lossy();
        if a == "--" {
            break;
        }
        if a == "--from" {
            return true;
        }
        if a.starts_with("--from=") && a.len() > "--from=".len() {
            return true;
        }
        i += 1;
    }
    false
}

pub fn tool_is_always_proxy(tool: &str) -> bool {
    let t = tool.to_ascii_lowercase();
    if matches!(t.as_str(), "uv" | "pip" | "pip3") {
        return true;
    }
    if t.starts_with("pip3.") {
        return true;
    }
    false
}

pub fn tool_is_python_name(tool: &str) -> bool {
    let t = tool.to_ascii_lowercase();
    if t == "python" || t == "python3" {
        return true;
    }
    if let Some(rest) = t.strip_prefix("python3.") {
        return rest.chars().all(|c| c.is_ascii_digit());
    }
    false
}
