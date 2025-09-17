#[cfg(unix)]
use nix::sys::signal::{self, Signal};
#[cfg(target_os = "linux")]
use nix::sys::signal::{SaFlags, SigAction, SigHandler, SigSet};
#[cfg(target_os = "linux")]
use nix::unistd::Pid;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::{self, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const PROTO_VERSION: &str = "2";

#[cfg(unix)]
static SIGINT_COUNT: AtomicU32 = AtomicU32::new(0);
#[cfg(unix)]
static GOT_TERM: AtomicBool = AtomicBool::new(false);
#[cfg(unix)]
static GOT_HUP: AtomicBool = AtomicBool::new(false);

#[cfg(unix)]
extern "C" fn handle_sigint(_sig: i32) {
    SIGINT_COUNT.fetch_add(1, Ordering::SeqCst);
}
#[cfg(unix)]
extern "C" fn handle_term(_sig: i32) {
    GOT_TERM.store(true, Ordering::SeqCst);
}
#[cfg(unix)]
extern "C" fn handle_hup(_sig: i32) {
    GOT_HUP.store(true, Ordering::SeqCst);
}

#[cfg(target_os = "linux")]
fn install_signal_handlers() {
    let act_int = SigAction::new(
        SigHandler::Handler(handle_sigint),
        SaFlags::SA_RESTART,
        SigSet::empty(),
    );
    let act_term = SigAction::new(
        SigHandler::Handler(handle_term),
        SaFlags::SA_RESTART,
        SigSet::empty(),
    );
    let act_hup = SigAction::new(
        SigHandler::Handler(handle_hup),
        SaFlags::SA_RESTART,
        SigSet::empty(),
    );
    unsafe {
        let _ = signal::sigaction(Signal::SIGINT, &act_int);
        let _ = signal::sigaction(Signal::SIGTERM, &act_term);
        let _ = signal::sigaction(Signal::SIGHUP, &act_hup);
    }
}

#[cfg(target_os = "linux")]
fn kill_parent_shell_if_interactive() {
    let interactive = atty::is(atty::Stream::Stdin) || atty::is(atty::Stream::Stdout);
    if std::env::var("AIFO_SHIM_KILL_PARENT_SHELL_ON_SIGINT")
        .ok()
        .as_deref()
        .unwrap_or("1")
        != "1"
        || !interactive
    {
        return;
    }
    // Read PPID and possible PGID
    let mut ppid: i32 = 0;
    let mut pgid: i32 = 0;
    if let Ok(stat) = fs::read_to_string("/proc/self/stat") {
        if let Some(rp) = stat.rfind(')') {
            let rest = &stat[rp + 1..];
            let parts: Vec<&str> = rest.split_whitespace().collect();
            if parts.len() >= 6 {
                ppid = parts[1].parse::<i32>().unwrap_or(0);
                pgid = parts[2].parse::<i32>().unwrap_or(0);
            }
        }
    }
    if ppid <= 1 {
        return;
    }
    // Try to identify if parent is a shell
    let mut is_shell = false;
    if let Ok(comm) = fs::read_to_string(format!("/proc/{}/comm", ppid)) {
        let c = comm.trim();
        is_shell = matches!(
            c,
            "sh" | "bash" | "dash" | "zsh" | "ksh" | "ash" | "busybox" | "busybox-sh"
        );
    }
    if !is_shell {
        return;
    }
    let _ = signal::kill(Pid::from_raw(ppid), Signal::SIGHUP);
    std::thread::sleep(std::time::Duration::from_millis(50));
    let _ = signal::kill(Pid::from_raw(ppid), Signal::SIGTERM);
    std::thread::sleep(std::time::Duration::from_millis(300));
    if pgid > 0 && pgid == ppid {
        let _ = signal::kill(Pid::from_raw(-pgid), Signal::SIGHUP);
        std::thread::sleep(std::time::Duration::from_millis(50));
        let _ = signal::kill(Pid::from_raw(-pgid), Signal::SIGTERM);
        std::thread::sleep(std::time::Duration::from_millis(300));
    }
    let _ = signal::kill(Pid::from_raw(ppid), Signal::SIGKILL);
}

#[cfg(not(target_os = "linux"))]
fn kill_parent_shell_if_interactive() {}

fn post_signal(url: &str, token: &str, exec_id: &str, signal_name: &str, verbose: bool) {
    let mut args: Vec<String> = vec![
        "-sS".into(),
        "-X".into(),
        "POST".into(),
        "-H".into(),
        format!("Authorization: Bearer {}", token),
        "-H".into(),
        "X-Aifo-Proto: 2".into(),
        "-H".into(),
        "Content-Type: application/x-www-form-urlencoded".into(),
        "--data-urlencode".into(),
        format!("exec_id={}", exec_id),
        "--data-urlencode".into(),
        format!("signal={}", signal_name),
    ];
    let mut final_url = url.to_string();
    if url.starts_with("unix://") {
        let sock = url.trim_start_matches("unix://").to_string();
        args.push("--unix-socket".into());
        args.push(sock);
        final_url = "http://localhost/signal".to_string();
    } else {
        if let Some(idx) = final_url.rfind("/exec") {
            final_url.truncate(idx);
        }
        final_url.push_str("/signal");
    }
    args.push(final_url);
    let mut cmd = Command::new("curl");
    cmd.args(&args);
    if verbose {
        let joined = args.join(" ");
        eprintln!(
            "\raifo-shim: posting signal {} via curl {}",
            signal_name, joined
        );
    }
    let _ = cmd.status();
}

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
    let base_dir = PathBuf::from(&home).join(".aifo-exec").join(&exec_id);
    let _ = fs::create_dir_all(&base_dir);
    // Record agent_ppid, agent_tpgid, controlling tty, and no_shell_on_tty marker
    if let Ok(stat) = fs::read_to_string("/proc/self/stat") {
        if let Some(rp) = stat.rfind(')') {
            let rest = &stat[rp + 1..];
            let parts: Vec<&str> = rest.split_whitespace().collect();
            if parts.len() >= 6 {
                let _ = fs::write(base_dir.join("agent_ppid"), parts[1]);
                let _ = fs::write(base_dir.join("agent_tpgid"), parts[5]);
            }
        }
    }
    let tty_path = std::fs::read_link("/proc/self/fd/0")
        .ok()
        .or_else(|| std::fs::read_link("/proc/self/fd/1").ok());
    if let Some(tp) = tty_path {
        let _ = fs::write(base_dir.join("tty"), tp.to_string_lossy().as_bytes());
    }
    let _ = fs::write(base_dir.join("no_shell_on_tty"), b"");

    if verbose {
        eprintln!("aifo-shim: tool={} cwd={} exec_id={}", tool, cwd, exec_id);
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

    #[cfg(target_os = "linux")]
    install_signal_handlers();

    let mut cmd = Command::new("curl");
    cmd.args(&args);
    cmd.stdout(Stdio::inherit());
    cmd.stderr(Stdio::inherit());
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("aifo-shim: failed to spawn curl: {}", e);
            let _ = fs::remove_dir_all(&tmp_dir);
            process::exit(86);
        }
    };

    // Poll for signals while streaming
    let mut status_success: bool = false;
    loop {
        // Check if child exited
        if let Ok(Some(st)) = child.try_wait() {
            status_success = st.success();
            break;
        }
        // Handle signals (Unix)
        #[cfg(unix)]
        {
            let cnt = SIGINT_COUNT.load(Ordering::SeqCst);
            if cnt >= 1 {
                let sig = if cnt == 1 {
                    "INT"
                } else if cnt == 2 {
                    "TERM"
                } else {
                    "KILL"
                };
                post_signal(&url, &token, &exec_id, sig, verbose);
                #[cfg(target_os = "linux")]
                {
                    if sig != "KILL" {
                        kill_parent_shell_if_interactive();
                    }
                }
                let _ = child.kill();
                // Determine exit code mapping
                let code = if std::env::var("AIFO_SHIM_EXIT_ZERO_ON_SIGINT")
                    .ok()
                    .as_deref()
                    .unwrap_or("1")
                    == "1"
                {
                    0
                } else {
                    match sig {
                        "INT" => 130,
                        "TERM" => 143,
                        _ => 137,
                    }
                };
                // Keep markers for proxy cleanup
                let _ = child.wait();
                let _ = fs::remove_dir_all(&tmp_dir);
                process::exit(code);
            }
            if GOT_TERM.load(Ordering::SeqCst) {
                post_signal(&url, &token, &exec_id, "TERM", verbose);
                #[cfg(target_os = "linux")]
                {
                    kill_parent_shell_if_interactive();
                }
                let _ = child.kill();
                let code = if std::env::var("AIFO_SHIM_EXIT_ZERO_ON_SIGINT")
                    .ok()
                    .as_deref()
                    .unwrap_or("1")
                    == "1"
                {
                    0
                } else {
                    143
                };
                let _ = child.wait();
                let _ = fs::remove_dir_all(&tmp_dir);
                process::exit(code);
            }
            if GOT_HUP.load(Ordering::SeqCst) {
                post_signal(&url, &token, &exec_id, "HUP", verbose);
                #[cfg(target_os = "linux")]
                {
                    kill_parent_shell_if_interactive();
                }
                let _ = child.kill();
                let code = if std::env::var("AIFO_SHIM_EXIT_ZERO_ON_SIGINT")
                    .ok()
                    .as_deref()
                    .unwrap_or("1")
                    == "1"
                {
                    0
                } else {
                    129
                };
                let _ = child.wait();
                let _ = fs::remove_dir_all(&tmp_dir);
                process::exit(code);
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    if !status_success {
        status_success = child.wait().map(|s| s.success()).unwrap_or(false);
    }

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
    } else if status_success {
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
        // In verbose mode, inform user and give proxy logs a brief moment to flush before exiting.
        if env::var("AIFO_TOOLCHAIN_VERBOSE").ok().as_deref() == Some("1") {
            eprintln!("aifo-coder: disconnect, waiting for process termination...");
            let wait_secs: u64 = env::var("AIFO_SHIM_DISCONNECT_WAIT_SECS")
                .ok()
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(1);
            if wait_secs > 0 {
                std::thread::sleep(std::time::Duration::from_secs(wait_secs));
            }
            eprintln!("aifo-coder: terminating now");
            // Ensure the agent prompt appears on a fresh, clean line
            eprintln!();
        }
    }
    process::exit(exit_code);
}
