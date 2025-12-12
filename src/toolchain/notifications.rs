/*!
Notifications command configuration parsing and execution.

This module replicates the existing logic for parsing ~/.aider.conf.yml and
executing a host notifications command (say) with a timeout, to be used by the
proxy notifications endpoint.
*/

#[cfg(unix)]
use nix::sys::signal::{kill, Signal};
#[cfg(unix)]
use nix::unistd::Pid;
use serde_yaml::Value as YamlValue;
use std::fs;
#[allow(unused_imports)]
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use crate::shell_like_split_args;

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

fn parse_notif_cfg() -> Result<NotifCfg, NotifyError> {
    // Reuse legacy tokenizer to obtain tokens; enforce new invariants on top.
    let tokens = parse_notifications_command_config().map_err(NotifyError::Policy)?;
    if tokens.is_empty() {
        return Err(NotifyError::Policy(
            "notifications-command is empty".to_string(),
        ));
    }
    let exec = &tokens[0];
    // Enforce absolute executable path
    if !exec.starts_with('/') {
        return Err(NotifyError::Policy(
            "notifications-command executable must be an absolute path".to_string(),
        ));
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
        for (_i, t) in tokens
            .iter()
            .enumerate()
            .take(tokens.len().saturating_sub(1))
        {
            if t == "{args}" {
                return Err(NotifyError::Policy(
                    "invalid notifications-command: '{args}' placeholder must be trailing"
                        .to_string(),
                ));
            }
        }
    } else {
        // Disallow non-trailing "{args}" anywhere (defensive)
        if tokens.iter().any(|t| t == "{args}") {
            return Err(NotifyError::Policy(
                "invalid notifications-command: '{args}' placeholder must be trailing".to_string(),
            ));
        }
    }

    let fixed_args: Vec<String> = if has_placeholder && tokens.len() >= 2 {
        tokens[1..tokens.len() - 1].to_vec()
    } else if tokens.len() >= 2 {
        tokens[1..].to_vec()
    } else {
        Vec::new()
    };

    let mut exec_abs_pb = PathBuf::from(exec);
    // Best-effort canonicalization to avoid symlink surprises; fall back to original on error.
    if let Ok(canon) = fs::canonicalize(&exec_abs_pb) {
        exec_abs_pb = canon;
    }
    Ok(NotifCfg {
        exec_abs: exec_abs_pb,
        fixed_args,
        has_trailing_args_placeholder: has_placeholder,
    })
}

/// Public helper to obtain the configured notifications executable basename after policy validation.
pub(crate) fn notifications_exec_basename() -> Result<String, String> {
    match parse_notif_cfg() {
        Ok(cfg) => {
            let bn = cfg
                .exec_abs
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string());
            Ok(bn)
        }
        Err(e) => match e {
            NotifyError::Policy(msg) => Err(msg),
            NotifyError::ExecSpawn(msg) => Err(msg),
            NotifyError::Timeout => Err("timeout".to_string()),
        },
    }
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

fn notifications_exec_in_safe_dir(exec_abs: &Path) -> bool {
    // Default allowlist of "safe" host directories for notifications executables.
    //
    // The list can be overridden via AIFO_NOTIFICATIONS_SAFE_DIRS, but only when
    // AIFO_NOTIFICATIONS_UNSAFE_ALLOWLIST=1 is set.
    let defaults = ["/usr/bin", "/bin", "/usr/local/bin", "/opt/homebrew/bin"];
    let allow_override = std::env::var("AIFO_NOTIFICATIONS_UNSAFE_ALLOWLIST")
        .ok()
        .as_deref()
        == Some("1");

    let dirs: Vec<PathBuf> = if allow_override {
        if let Ok(v) = std::env::var("AIFO_NOTIFICATIONS_SAFE_DIRS") {
            let mut out = Vec::new();
            for part in v.split(',') {
                let p = part.trim();
                if p.is_empty() {
                    continue;
                }
                out.push(PathBuf::from(p));
                if out.len() >= 16 {
                    break;
                }
            }
            if !out.is_empty() {
                // Canonicalize override dirs (best-effort) to align with exec canonicalization
                out.into_iter()
                    .map(|p| fs::canonicalize(&p).unwrap_or(p))
                    .collect()
            } else {
                // Fallback to defaults, canonicalized
                defaults
                    .iter()
                    .map(|d| {
                        let p = PathBuf::from(d);
                        fs::canonicalize(&p).unwrap_or(p)
                    })
                    .collect()
            }
        } else {
            // No override value; use defaults, canonicalized
            defaults
                .iter()
                .map(|d| {
                    let p = PathBuf::from(d);
                    fs::canonicalize(&p).unwrap_or(p)
                })
                .collect()
        }
    } else {
        // Enforced defaults without override; canonicalize to robustly match exec_abs
        defaults
            .iter()
            .map(|d| {
                let p = PathBuf::from(d);
                fs::canonicalize(&p).unwrap_or(p)
            })
            .collect()
    };

    dirs.iter().any(|d| exec_abs.starts_with(d))
}

fn parse_env_allowlist() -> Vec<String> {
    let list = match std::env::var("AIFO_NOTIFICATIONS_ENV_ALLOW") {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    let mut out: Vec<String> = Vec::new();
    for part in list.split(',') {
        let key = part.trim();
        if key.is_empty() || key.len() > 64 {
            continue;
        }
        if !key
            .bytes()
            .all(|b| matches!(b, b'A'..=b'Z' | b'0'..=b'9' | b'_'))
        {
            continue;
        }
        let ks = key.to_string();
        if !out.contains(&ks) {
            out.push(ks);
        }
        if out.len() >= 16 {
            break;
        }
    }
    out
}

fn clamp_max_args() -> usize {
    let mut max_args = 8usize;
    if let Ok(v) = std::env::var("AIFO_NOTIFICATIONS_MAX_ARGS") {
        if let Ok(n) = v.trim().parse::<usize>() {
            let m = n.clamp(1, 32);
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
    // Optional: trim child environment for notifications (opt-in via AIFO_NOTIFICATIONS_TRIM_ENV=1)
    if std::env::var("AIFO_NOTIFICATIONS_TRIM_ENV").ok().as_deref() == Some("1") {
        cmd.env_clear();
        // Preserve minimal environment: PATH, HOME, LANG (or defaults), and any LC_* variables.
        if let Ok(v) = std::env::var("PATH") {
            if !v.is_empty() {
                cmd.env("PATH", v);
            }
        } else {
            cmd.env(
                "PATH",
                "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
            );
        }
        if let Ok(v) = std::env::var("HOME") {
            if !v.is_empty() {
                cmd.env("HOME", v);
            }
        }
        if let Ok(v) = std::env::var("LANG") {
            if !v.is_empty() {
                cmd.env("LANG", v);
            }
        } else {
            cmd.env("LANG", "C.UTF-8");
        }
        for (k, v) in std::env::vars() {
            if k.starts_with("LC_") && !v.is_empty() {
                cmd.env(&k, v);
            }
        }
        // User-requested additional variables allowlist (comma-separated names)
        for key in parse_env_allowlist() {
            if let Ok(val) = std::env::var(&key) {
                cmd.env(&key, val);
            }
        }
    }
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = {
        // Retry on transient EBUSY (Text file busy) a few times with small sleeps
        let mut attempts = 0usize;
        loop {
            match cmd.spawn() {
                Ok(c) => break c,
                Err(e) => {
                    let ebusy = e.raw_os_error() == Some(26); // ETXTBUSY on Unix
                    if ebusy && attempts < 10 {
                        attempts += 1;
                        std::thread::sleep(Duration::from_millis(50));
                        continue;
                    }
                    let bn = exec_abs
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| exec_abs.display().to_string());
                    return Err(NotifyError::ExecSpawn(format!(
                        "host '{}' execution failed: {}",
                        bn, e
                    )));
                }
            }
        }
    };

    // Read stdout/stderr concurrently to avoid blocking pipes
    let so = child.stdout.take();
    let se = child.stderr.take();
    let out_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut s) = so {
            use std::io::Read as _;
            let _ = s.read_to_end(&mut buf);
        }
        buf
    });
    let err_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut s) = se {
            use std::io::Read as _;
            let _ = s.read_to_end(&mut buf);
        }
        buf
    });

    let start = std::time::Instant::now();
    // Poll until exit or timeout
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                // Join readers and collect output; append stderr after stdout
                let mut all: Vec<u8> = Vec::new();
                let out = out_handle.join().unwrap_or_default();
                if !out.is_empty() {
                    all.extend_from_slice(&out);
                }
                let err = err_handle.join().unwrap_or_default();
                if !err.is_empty() {
                    all.extend_from_slice(&err);
                }
                let code = status.code().unwrap_or(1);
                return Ok((code, all));
            }
            Ok(None) => {
                if timeout_secs > 0 && start.elapsed() >= Duration::from_secs(timeout_secs) {
                    // Timeout: cooperative termination (TERM then KILL), ensure wait/reap
                    #[cfg(unix)]
                    {
                        let _ = kill(Pid::from_raw(child.id() as i32), Signal::SIGTERM);
                        let deadline = std::time::Instant::now() + Duration::from_millis(250);
                        loop {
                            match child.try_wait() {
                                Ok(Some(_)) => break,
                                Ok(None) => {
                                    if std::time::Instant::now() >= deadline {
                                        break;
                                    }
                                }
                                Err(_) => break,
                            }
                            std::thread::sleep(Duration::from_millis(25));
                        }
                        if let Ok(None) = child.try_wait() {
                            let _ = kill(Pid::from_raw(child.id() as i32), Signal::SIGKILL);
                        }
                    }
                    #[cfg(not(unix))]
                    {
                        let _ = child.kill();
                    }
                    let _ = child.wait();
                    // Ensure readers finish after pipes are closed
                    let _ = out_handle.join();
                    let _ = err_handle.join();
                    return Err(NotifyError::Timeout);
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(e) => {
                // Treat as spawn/exec error; propagate as generic spawn error text
                let bn = exec_abs
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| exec_abs.display().to_string());
                // Ensure readers finish; best-effort
                let _ = out_handle.join();
                let _ = err_handle.join();
                return Err(NotifyError::ExecSpawn(format!(
                    "host '{}' execution failed: {}",
                    bn, e
                )));
            }
        }
    }
}

/// Parse ~/.aider.conf.yml and extract notifications-command as argv tokens (serde_yaml full-doc).
pub(crate) fn parse_notifications_command_config() -> Result<Vec<String>, String> {
    // Resolve config path from env override or default ~/.aider.conf.yml
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

    let content_raw =
        fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {}", path.display(), e))?;
    // Tolerate a trailing literal "\n" token sometimes written by helpers
    let content = {
        let s = content_raw.trim_end_matches(&[' ', '\t', '\r', '\n'][..]);
        if s.ends_with("\\n") {
            let mut t = s.to_string();
            t.truncate(t.len().saturating_sub(2));
            t
        } else {
            s.to_string()
        }
    };

    // Parse entire YAML document and locate the "notifications-command" node
    let doc: YamlValue = serde_yaml::from_str(&content)
        .map_err(|e| format!("cannot parse {}: {}", path.display(), e))?;

    // Extract node
    let node = match &doc {
        YamlValue::Mapping(map) => {
            let mut found: Option<&YamlValue> = None;
            for (k, v) in map {
                if let YamlValue::String(ks) = k {
                    if ks == "notifications-command" {
                        found = Some(v);
                        break;
                    }
                }
            }
            match found {
                Some(v) => v,
                None => {
                    return Err("notifications-command not found in ~/.aider.conf.yml".to_string())
                }
            }
        }
        _ => {
            return Err("notifications-command not found in ~/.aider.conf.yml".to_string());
        }
    };

    // Normalize node to tokens (String or Seq<String>)
    let tokens: Vec<String> = match node {
        YamlValue::Sequence(seq) => {
            let mut out: Vec<String> = Vec::new();
            for item in seq {
                match item {
                    YamlValue::String(s) => out.push(s.clone()),
                    _ => {
                        return Err(
                            "notifications-command must be a sequence of strings".to_string()
                        )
                    }
                }
            }
            if out.is_empty() {
                return Err("notifications-command is empty or malformed".to_string());
            }
            out
        }
        YamlValue::String(s) => {
            let argv = shell_like_split_args(s);
            if argv.is_empty() {
                return Err("notifications-command parsed to an empty command".to_string());
            }
            argv
        }
        _ => return Err("notifications-command must be a string or sequence".to_string()),
    };

    if tokens.is_empty() {
        return Err("notifications-command is empty".to_string());
    }

    Ok(tokens)
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

    if !notifications_exec_in_safe_dir(&cfg.exec_abs) {
        return Err(NotifyError::Policy(format!(
            "notifications executable '{}' is not in a safe directory",
            cfg.exec_abs.display()
        )));
    }

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
    if cmd.len() > 128 {
        return Err(NotifyError::Policy("cmd too long".to_string()));
    }
    if cmd != basename {
        return Err(NotifyError::Policy(format!(
            "only executable basename '{}' is accepted (got '{}')",
            basename, cmd
        )));
    }

    // Argument policy
    if argv.len() > 128 || argv.iter().any(|a| a.len() > 4096) {
        return Err(NotifyError::Policy("too many or too long args".to_string()));
    }

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
