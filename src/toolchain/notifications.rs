/*!
Notifications command configuration parsing and execution.

This module replicates the existing logic for parsing ~/.aider.conf.yml and
executing a host notifications command (say) with a timeout, to be used by the
proxy notifications endpoint.
*/

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;
use std::io::Read;

use crate::{shell_like_split_args, strip_outer_quotes};

#[derive(Debug)]
struct NotifCfg {
    exec_abs: PathBuf,
    fixed_args: Vec<String>,
    has_trailing_args_placeholder: bool,
}

#[derive(Debug)]
pub(crate) enum NotifyError {
    Policy(String),
    ExecSpawn(String),
    Timeout,
}

fn parse_notif_cfg() -> Result<NotifCfg, String> {
    // Reuse legacy tokenizer to obtain tokens; enforce new invariants on top.
    let tokens = parse_notifications_command_config()?;
    if tokens.is_empty() {
        return Err("notifications-command is empty".to_string());
    }
    let exec = &tokens[0];
    // Enforce absolute executable path
    if !exec.starts_with('/') {
        return Err("notifications-command executable must be an absolute path".to_string());
    }
    // Detect optional trailing "{args}" placeholder; it must be strictly last if present.
    let mut has_placeholder = false;
    if let Some(last) = tokens.last() {
        if last == "{args}" {
            has_placeholder = true;
        }
    }
    if has_placeholder {
        // Disallow any other "{args}" occurrences
        for (i, t) in tokens.iter().enumerate().take(tokens.len().saturating_sub(1)) {
            if t == "{args}" {
                return Err("invalid notifications-command: '{args}' placeholder must be trailing".to_string());
            }
        }
    } else {
        // Disallow non-trailing "{args}" anywhere (defensive)
        if tokens.iter().any(|t| t == "{args}") {
            return Err("invalid notifications-command: '{args}' placeholder must be trailing".to_string());
        }
    }

    let fixed_args: Vec<String> = if has_placeholder && tokens.len() >= 2 {
        tokens[1..tokens.len() - 1].to_vec()
    } else if tokens.len() >= 2 {
        tokens[1..].to_vec()
    } else {
        Vec::new()
    };

    Ok(NotifCfg {
        exec_abs: PathBuf::from(exec),
        fixed_args,
        has_trailing_args_placeholder: has_placeholder,
    })
}

fn compute_allowlist_basenames() -> Vec<String> {
    // Default allowlist
    let mut out: Vec<String> = vec!["say".to_string()];
    if let Ok(extra) = std::env::var("AIFO_NOTIFICATIONS_ALLOWLIST") {
        let mut seen = std::collections::HashSet::<String>::new();
        // seed with defaults
        for d in &out {
            seen.insert(d.clone());
        }
        for part in extra.split(',') {
            let name = part.trim().to_string();
            if name.is_empty() {
                continue;
            }
            if seen.insert(name.clone()) {
                out.push(name);
                if out.len() >= 16 {
                    break;
                }
            }
        }
    }
    out
}

fn clamp_max_args() -> usize {
    let mut max_args = 8usize;
    if let Ok(v) = std::env::var("AIFO_NOTIFICATIONS_MAX_ARGS") {
        if let Ok(n) = v.trim().parse::<usize>() {
            let m = n.max(1).min(32);
            max_args = m;
        }
    }
    max_args
}

fn run_with_timeout(
    exec_abs: &PathBuf,
    args: &[String],
    timeout_secs: u64,
) -> Result<(i32, Vec<u8>), NotifyError> {
    let mut cmd = Command::new(exec_abs);
    cmd.args(args);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            let bn = exec_abs
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| exec_abs.display().to_string());
            return Err(NotifyError::ExecSpawn(format!(
                "host '{}' execution failed: {}",
                bn, e
            )));
        }
    };

    let start = std::time::Instant::now();
    // Poll until exit or timeout
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                // Read stdout and stderr; append stderr after stdout
                let mut all: Vec<u8> = Vec::new();
                if let Some(mut so) = child.stdout.take() {
                    let mut buf = Vec::new();
                    let _ = so.read_to_end(&mut buf);
                    all.extend_from_slice(&buf);
                }
                if let Some(mut se) = child.stderr.take() {
                    let mut buf = Vec::new();
                    let _ = se.read_to_end(&mut buf);
                    if !buf.is_empty() {
                        all.extend_from_slice(&buf);
                    }
                }
                let code = status.code().unwrap_or(1);
                return Ok((code, all));
            }
            Ok(None) => {
                if timeout_secs > 0 && start.elapsed() >= Duration::from_secs(timeout_secs) {
                    // Timeout: best-effort terminate and reap
                    let _ = child.kill();
                    // brief grace
                    std::thread::sleep(Duration::from_millis(250));
                    let _ = child.wait();
                    return Err(NotifyError::Timeout);
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(_e) => {
                // Treat as spawn/exec error; propagate as generic spawn error text
                let bn = exec_abs
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| exec_abs.display().to_string());
                return Err(NotifyError::ExecSpawn(format!(
                    "host '{}' execution failed: try_wait failed",
                    bn
                )));
            }
        }
    }
}

/// Parse ~/.aider.conf.yml and extract notifications-command as argv tokens.
pub(crate) fn parse_notifications_command_config() -> Result<Vec<String>, String> {
    // Allow tests (and power users) to override config path explicitly
    let path = if let Ok(p) = std::env::var("AIFO_NOTIFICATIONS_CONFIG") {
        let p = p.trim().to_string();
        if !p.is_empty() {
            PathBuf::from(p)
        } else {
            home::home_dir()
                .ok_or_else(|| "home directory not found".to_string())?
                .join(".aider.conf.yml")
        }
    } else {
        home::home_dir()
            .ok_or_else(|| "home directory not found".to_string())?
            .join(".aider.conf.yml")
    };
    let content =
        fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {}", path.display(), e))?;

    // Pre-split lines to allow simple multi-line parsing
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0usize;
    while i < lines.len() {
        let line = lines[i];
        let l = line.trim_start();
        if l.starts_with('#') || l.is_empty() {
            i += 1;
            continue;
        }
        if let Some(rest) = l.strip_prefix("notifications-command:") {
            let mut val = rest.trim().to_string();
            // Tolerate configs/tests that append a literal "\n" at end of line
            if val.ends_with("\\n") {
                val.truncate(val.len() - 2);
            }

            // Helper: parse inline JSON/YAML-like array ["say","--title","AIFO"]
            let parse_inline_array = |val: &str| -> Result<Vec<String>, String> {
                if val.len() < 2 {
                    return Err("notifications-command parsed to an empty command".to_string());
                }
                let inner = &val[1..val.len() - 1];
                let mut argv: Vec<String> = Vec::new();
                let mut cur = String::new();
                let mut in_single = false;
                let mut in_double = false;
                let mut esc = false;
                for ch in inner.chars() {
                    if esc {
                        let c = match ch {
                            'n' => '\n',
                            'r' => '\r',
                            't' => '\t',
                            other => other,
                        };
                        cur.push(c);
                        esc = false;
                        continue;
                    }
                    match ch {
                        '\\' if in_double || in_single => esc = true,
                        '"' if !in_single => {
                            if in_double {
                                in_double = false;
                                argv.push(cur.clone());
                                cur.clear();
                            } else {
                                in_double = true;
                            }
                        }
                        '\'' if !in_double => {
                            if in_single {
                                in_single = false;
                                argv.push(cur.clone());
                                cur.clear();
                            } else {
                                in_single = true;
                            }
                        }
                        ',' if !in_single && !in_double => { /* separator */ }
                        c => {
                            if in_single || in_double {
                                cur.push(c);
                            }
                        }
                    }
                }
                if !cur.is_empty() && !in_single && !in_double {
                    argv.push(cur);
                }
                if argv.is_empty() {
                    Err("notifications-command parsed to an empty command".to_string())
                } else {
                    Ok(argv)
                }
            };

            // Case 1: inline array
            if val.starts_with('[') && val.ends_with(']') {
                return parse_inline_array(&val);
            }

            // Case 2: explicit block scalars '|' or '>'
            if val == "|" || val == ">" || val.is_empty() {
                // Collect subsequent indented lines; also support YAML list items beginning with '-'
                let mut j = i + 1;
                // Skip blank/comment lines until first candidate
                while j < lines.len()
                    && (lines[j].trim().is_empty() || lines[j].trim_start().starts_with('#'))
                {
                    j += 1;
                }
                if j >= lines.len() {
                    return Err("notifications-command is empty or malformed".to_string());
                }
                let first = lines[j];
                let is_list = first.trim_start().starts_with('-');
                if is_list {
                    let mut argv: Vec<String> = Vec::new();
                    while j < lines.len() {
                        let ln = lines[j];
                        let t = ln.trim_start();
                        if !t.starts_with('-') {
                            break;
                        }
                        let item = t.trim_start_matches('-').trim();
                        if !item.is_empty() {
                            argv.push(strip_outer_quotes(item));
                        }
                        j += 1;
                    }
                    if argv.is_empty() {
                        return Err("notifications-command list is empty".to_string());
                    }
                    return Ok(argv);
                } else {
                    // Block scalar: concatenate trimmed lines with spaces into a single command string
                    let mut parts: Vec<String> = Vec::new();
                    while j < lines.len() {
                        let ln = lines[j];
                        let t = ln.trim_start();
                        if t.is_empty() || t.starts_with('#') {
                            j += 1;
                            continue;
                        }
                        // Stop if de-indented to column 0 and looks like a new key
                        if !ln.starts_with(' ') && t.contains(':') {
                            break;
                        }
                        parts.push(t.to_string());
                        j += 1;
                    }
                    let joined = parts.join(" ");
                    let argv = shell_like_split_args(&strip_outer_quotes(&joined));
                    if argv.is_empty() {
                        return Err("notifications-command parsed to an empty command".to_string());
                    }
                    return Ok(argv);
                }
            }

            // Case 3: single-line scalar
            let unquoted = strip_outer_quotes(&val);
            let argv = shell_like_split_args(&unquoted);
            if argv.is_empty() {
                return Err("notifications-command parsed to an empty command".to_string());
            }
            return Ok(argv);
        }
        i += 1;
    }
    Err("notifications-command not found in ~/.aider.conf.yml".to_string())
}

/// Validate and, if allowed, execute the requested host notification command with provided args.
/// Returns (exit_code, output_bytes) on success, or Err(reason) if rejected.
pub(crate) fn notifications_handle_request(
    cmd: &str,
    argv: &[String],
    _verbose: bool,
    timeout_secs: u64,
) -> Result<(i32, Vec<u8>), NotifyError> {
    let cfg = parse_notif_cfg()?;

    // Allowlist: default ["say"] with env extension
    let allowed = compute_allowlist_basenames();
    let basename = cfg
        .exec_abs
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    if !allowed.iter().any(|b| b == &basename) {
        return Err(NotifyError::Policy(format!(
            "command '{}' not allowed for notifications",
            basename
        )));
    }

    // Require request cmd to equal basename(exec_abs)
    if cmd != basename {
        return Err(NotifyError::Policy(format!(
            "only executable basename '{}' is accepted (got '{}')",
            basename, cmd
        )));
    }

    // Argument policy
    let final_args: Vec<String> = if cfg.has_trailing_args_placeholder {
        let cap = clamp_max_args();
        let mut args = cfg.fixed_args.clone();
        args.extend(argv.iter().take(cap).cloned());
        args
    } else {
        if cfg.fixed_args != argv {
            return Err(NotifyError::Policy(format!(
                "arguments mismatch: configured {:?} vs requested {:?}",
                cfg.fixed_args, argv
            )));
        }
        cfg.fixed_args.clone()
    };

    // Execute with timeout; capture stdout+stderr
    run_with_timeout(&cfg.exec_abs, &final_args, timeout_secs)
}
