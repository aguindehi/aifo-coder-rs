use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::{self, Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

const PROTO_VERSION: &str = "2";

fn main() {
    let url = match env::var("AIFO_TOOLEEXEC_URL") {
        Ok(v) if !v.trim().is_empty() => v,
        _ => {
            eprintln!("aifo-shim: proxy not configured. Please launch agent with --toolchain.");
            process::exit(86);
        }
    };
    let token = match env::var("AIFO_TOOLEEXEC_TOKEN") {
        Ok(v) if !v.trim().is_empty() => v,
        _ => {
            eprintln!("aifo-shim: proxy token missing. Please launch agent with --toolchain.");
            process::exit(86);
        }
    };
    let verbose = env::var("AIFO_TOOLCHAIN_VERBOSE").ok().as_deref() == Some("1");

    let tool = std::env::args_os()
        .next()
        .and_then(|p| {
            let pb = PathBuf::from(p);
            pb.file_name().map(|s| s.to_string_lossy().to_string())
        })
        .unwrap_or_else(|| "unknown".to_string());

    let cwd = env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| ".".to_string());

    if verbose {
        eprintln!("aifo-shim: tool={} cwd={}", tool, cwd);
        eprintln!(
            "aifo-shim: preparing request to {} (proto={})",
            url, PROTO_VERSION
        );
    }

    // Form parts to be encoded with --data-urlencode
    let mut form_parts: Vec<(String, String)> = Vec::new();
    form_parts.push(("tool".to_string(), tool.clone()));
    form_parts.push(("cwd".to_string(), cwd.clone()));
    for a in std::env::args().skip(1) {
        form_parts.push(("arg".to_string(), a));
    }

    // Prepare temp directory for header dump
    let tmp_base = env::var("TMPDIR")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "/tmp".to_string());
    let tmp_dir = format!("{}/aifo-shim.{}", tmp_base, std::process::id());
    let _ = fs::create_dir_all(&tmp_dir);
    let header_path = format!("{}/h", tmp_dir);

    // Generate ExecId (prefer env AIFO_EXEC_ID) and record agent markers for proxy disconnect handling
    let exec_id: String = match env::var("AIFO_EXEC_ID") {
        Ok(v) if !v.trim().is_empty() => v,
        _ => {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_else(|_| std::time::Duration::from_secs(0))
                .as_nanos();
            let pid = std::process::id() as u128;
            format!("{:032x}", now ^ pid)
        }
    };
    let home = env::var("HOME").unwrap_or_else(|_| "/home/coder".to_string());
    let base_dir = PathBuf::from(&home)
        .join(".aifo-exec")
        .join(&exec_id);
    let _ = fs::create_dir_all(&base_dir);

    // Best-effort: write agent_ppid and agent_tpgid, and controlling tty path
    if let Ok(stat) = fs::read_to_string("/proc/self/stat") {
        if let Some(rp) = stat.rfind(')') {
            let rest = &stat[rp + 1..];
            let parts: Vec<&str> = rest.split_whitespace().collect();
            // Fields after ')' are: state, ppid(1), pgrp(2), session(3), tty_nr(4), tpgid(5), ...
            if parts.len() >= 6 {
                let _ = fs::write(base_dir.join("agent_ppid"), parts[1]);
                let _ = fs::write(base_dir.join("agent_tpgid"), parts[5]);
            }
        }
    }
    // Controlling TTY (stdin or stdout)
    let tty_path = std::fs::read_link("/proc/self/fd/0")
        .ok()
        .or_else(|| std::fs::read_link("/proc/self/fd/1").ok());
    if let Some(tp) = tty_path {
        let _ = fs::write(base_dir.join("tty"), tp.to_string_lossy().as_bytes());
    }

    // Build curl command
    let mut args: Vec<String> = Vec::new();
    args.push("-sS".to_string());
    args.push("--no-buffer".to_string());
    args.push("-D".to_string());
    args.push(header_path.clone());
    args.push("-X".to_string());
    args.push("POST".to_string());
    args.push("-H".to_string());
    args.push(format!("Authorization: Bearer {}", token));
    args.push("-H".to_string());
    args.push(format!("X-Aifo-Proto: {}", PROTO_VERSION));
    args.push("-H".to_string());
    args.push("TE: trailers".to_string());
    args.push("-H".to_string());
    args.push("Content-Type: application/x-www-form-urlencoded".to_string());
    // Provide ExecId header so proxy can correlate; also exported via env for docker exec path
    args.push("-H".to_string());
    args.push(format!("X-Aifo-Exec-Id: {}", exec_id));

    for (k, v) in &form_parts {
        args.push("--data-urlencode".to_string());
        args.push(format!("{}={}", k, v));
    }

    let mut final_url = url.clone();
    if url.starts_with("unix://") {
        // unix socket mode
        let sock_path = url.trim_start_matches("unix://").to_string();
        args.push("--unix-socket".to_string());
        args.push(sock_path);
        final_url = "http://localhost/exec".to_string();
    }
    args.push(final_url);

    let mut cmd = Command::new("curl");
    cmd.args(&args);
    cmd.stdout(Stdio::inherit());
    cmd.stderr(Stdio::inherit());

    let status = match cmd.status() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("aifo-shim: failed to spawn curl: {}", e);
            let _ = fs::remove_dir_all(&tmp_dir);
            process::exit(86);
        }
    };

    // Parse X-Exit-Code from headers/trailers
    let mut exit_code: i32 = 1;
    let mut had_header = false;
    if let Ok(hdr) = fs::read_to_string(&header_path) {
        for line in hdr.lines() {
            if let Some(v) = line.strip_prefix("X-Exit-Code:") {
                if let Ok(code) = v.trim().parse::<i32>() {
                    exit_code = code;
                    had_header = true;
                }
            }
        }
    } else if status.success() {
        // If curl succeeded but header file missing, assume success
        exit_code = 0;
        had_header = true;
    }

    // Cleanup tmp; keep agent markers on disconnect (no header) so proxy can close lingering shell
    let _ = fs::remove_dir_all(&tmp_dir);
    if had_header {
        let _ = fs::remove_dir_all(&base_dir);
    } else {
        // Optional default: zero exit on disconnect unless opted out
        if env::var("AIFO_SHIM_EXIT_ZERO_ON_DISCONNECT")
            .ok()
            .as_deref()
            != Some("0")
        {
            exit_code = 0;
        }
    }
    process::exit(exit_code);
}
