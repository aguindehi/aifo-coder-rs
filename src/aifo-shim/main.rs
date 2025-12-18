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

fn pick_local_python_path() -> Option<&'static str> {
    // Prefer distro python first (common in Debian/Ubuntu), fallback to /usr/local.
    for p in ["/usr/bin/python3", "/usr/local/bin/python3"] {
        if Path::new(p).is_file() {
            return Some(p);
        }
    }
    None
}

fn proxied_exec_url() -> Option<String> {
    env::var("AIFO_TOOLEEXEC_URL")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn proxied_exec_token() -> Option<String> {
    env::var("AIFO_TOOLEEXEC_TOKEN")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
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
            || a == "--inspect-port"
            || a == "--title"
        {
            i += 2;
            continue;
        }

        // Flags that are `--flag=value` forms that consume their value inline.
        if a.starts_with("--require=")
            || a.starts_with("--loader=")
            || a.starts_with("--import=")
            || a.starts_with("--inspect-port=")
            || a.starts_with("--title=")
        {
            i += 1;
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

fn python_script_arg(argv: &[OsString]) -> Option<String> {
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

        // -m module: treat as local in v1 when smart python enabled
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

        // Generic option, keep scanning.
        if a.starts_with('-') {
            i += 1;
            continue;
        }

        // First non-flag token treated as script path.
        return Some(a);
    }
    None
}

fn python_is_module_mode(argv: &[OsString]) -> bool {
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

fn should_smart_local_python(tool: &str, argv: &[OsString]) -> Option<Option<PathBuf>> {
    if tool != "python" && tool != "python3" {
        return None;
    }
    if !env_is_truthy("AIFO_SHIM_SMART") || !env_is_truthy("AIFO_SHIM_SMART_PYTHON") {
        return None;
    }

    // `python -m module` => local (v1 conservative default)
    if python_is_module_mode(argv) {
        return Some(None);
    }

    // `python /path/to/script.py` => local if script outside /workspace
    if let Some(script) = python_script_arg(argv) {
        let p = resolve_program_path(&script);
        if !is_under_workspace(&p) {
            return Some(Some(p));
        }
    }

    None
}

fn exec_local(local_bin: &str, argv: &[OsString]) -> ExitCode {
    let mut cmd = Command::new(local_bin);
    if argv.len() > 1 {
        cmd.args(&argv[1..]);
    }
    let status = match cmd.status() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("aifo-shim: failed to exec local runtime: {e}");
            return ExitCode::from(1);
        }
    };
    ExitCode::from(status.code().unwrap_or(1) as u8)
}

fn exec_proxy(tool: &str, argv: &[OsString]) -> ExitCode {
    // Phase 2 requires that proxied execution remains available in the same shim.
    // The real proxy sidecar HTTP protocol is implemented by the existing aifo toolchain.
    //
    // In this codebase, we keep it simple and invoke `aifo-toolexec` if present, falling back
    // to a clear error if proxy config is missing.
    //
    // NOTE: This intentionally does NOT consult PATH for the runtime tool (to avoid recursion);
    // it delegates to a dedicated proxy client binary.
    let url = match proxied_exec_url() {
        Some(u) => u,
        None => {
            eprintln!("aifo-shim: proxy disabled: missing AIFO_TOOLEEXEC_URL");
            return ExitCode::from(86);
        }
    };
    let token = proxied_exec_token().unwrap_or_default();

    // Prefer the in-image client.
    let client = if Path::new("/usr/local/bin/aifo-toolexec").is_file() {
        "/usr/local/bin/aifo-toolexec"
    } else if Path::new("/opt/aifo/bin/aifo-toolexec").is_file() {
        "/opt/aifo/bin/aifo-toolexec"
    } else {
        eprintln!("aifo-shim: proxy client not found (aifo-toolexec)");
        return ExitCode::from(86);
    };

    let mut cmd = Command::new(client);
    cmd.arg("--url").arg(url);
    if !token.is_empty() {
        cmd.arg("--token").arg(token);
    }
    cmd.arg("--tool").arg(tool);
    if argv.len() > 1 {
        for a in &argv[1..] {
            cmd.arg(a);
        }
    }

    let status = match cmd.status() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("aifo-shim: failed to exec proxy client: {e}");
            return ExitCode::from(1);
        }
    };
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
            if verbose_enabled() {
                eprintln!("aifo-shim: smart: tool=node wanted local but no local node found");
            }
            return exec_proxy(&tool, &argv);
        };

        log_smart_line("node", "outside-workspace", Some(&program), Some(local));
        return exec_local(local, &argv);
    }

    if let Some(program_opt) = should_smart_local_python(&tool, &argv) {
        let Some(local) = pick_local_python_path() else {
            if verbose_enabled() {
                eprintln!("aifo-shim: smart: tool=python wanted local but no local python found");
            }
            return exec_proxy(&tool, &argv);
        };

        match program_opt {
            Some(script) => {
                log_smart_line("python", "outside-workspace", Some(&script), Some(local));
            }
            None => {
                log_smart_line("python", "module-mode", None, Some(local));
            }
        }

        return exec_local(local, &argv);
    }

    exec_proxy(&tool, &argv)
}
