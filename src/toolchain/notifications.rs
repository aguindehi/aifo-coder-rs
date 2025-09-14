/*!
Notifications command configuration parsing and execution.

This module replicates the existing logic for parsing ~/.aider.conf.yml and
executing a host notifications command (say) with a timeout, to be used by the
proxy notifications endpoint.
*/

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use crate::{shell_like_split_args, strip_outer_quotes};

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

/// Validate and, if allowed, execute the host 'say' command with provided args.
/// Returns (exit_code, output_bytes) on success, or Err(reason) if rejected.
pub(crate) fn notifications_handle_request(
    argv: &[String],
    _verbose: bool,
    timeout_secs: u64,
) -> Result<(i32, Vec<u8>), String> {
    let cfg_argv = parse_notifications_command_config()?;
    if cfg_argv.is_empty() {
        return Err("notifications-command is empty".to_string());
    }
    if cfg_argv[0] != "say" {
        return Err("only 'say' is allowed as notifications-command executable".to_string());
    }
    let cfg_args = &cfg_argv[1..];
    if cfg_args != argv {
        return Err(format!(
            "arguments mismatch: configured {:?} vs requested {:?}",
            cfg_args, argv
        ));
    }

    // Execute 'say' on the host with a timeout.
    let (tx, rx) = std::sync::mpsc::channel();
    let args_vec: Vec<String> = argv.to_vec();
    std::thread::spawn(move || {
        let mut cmd = Command::new("say");
        for a in &args_vec {
            cmd.arg(a);
        }
        let out = cmd.output();
        let _ = tx.send(out);
    });
    match rx.recv_timeout(Duration::from_secs(timeout_secs)) {
        Ok(Ok(o)) => {
            let mut b = o.stdout;
            if !o.stderr.is_empty() {
                b.extend_from_slice(&o.stderr);
            }
            Ok((o.status.code().unwrap_or(1), b))
        }
        Ok(Err(e)) => Err(format!("failed to execute host 'say': {}", e)),
        Err(_timeout) => Err("host 'say' execution timed out".to_string()),
    }
}
