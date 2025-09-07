use atty;
use clap::ValueEnum;
use once_cell::sync::Lazy;
use std::io::{Read, Write};
use std::time::{Duration, SystemTime};
mod color;
mod util;
mod apparmor;
mod registry;
mod docker;
mod lock;
mod toolchain;
mod fork;
pub use color::*;
pub use util::*;
pub use apparmor::*;
pub use registry::*;
pub use docker::*;
pub use lock::*;
pub use toolchain::*;
pub use fork::*;

#[cfg(windows)]
fn ps_quote_inner(s: &str) -> String {
    let esc = s.replace('\'', "''");
    format!("'{}'", esc)
}

#[cfg(windows)]
/// Build the PowerShell inner command for fork panes (used by tests).
pub fn fork_ps_inner_string(
    agent: &str,
    sid: &str,
    i: usize,
    pane_dir: &std::path::Path,
    pane_state_dir: &std::path::Path,
    child_args: &[String],
) -> String {
    let cname = format!("aifo-coder-{}-{}-{}", agent, sid, i);
    let kv = [
        ("AIFO_CODER_SKIP_LOCK", "1".to_string()),
        ("AIFO_CODER_CONTAINER_NAME", cname.clone()),
        ("AIFO_CODER_HOSTNAME", cname),
        ("AIFO_CODER_FORK_SESSION", sid.to_string()),
        ("AIFO_CODER_FORK_INDEX", i.to_string()),
        (
            "AIFO_CODER_FORK_STATE_DIR",
            pane_state_dir.display().to_string(),
        ),
    ];
    let mut assigns: Vec<String> = Vec::new();
    for (k, v) in kv {
        assigns.push(format!("$env:{}={}", k, ps_quote_inner(&v)));
    }
    let mut words: Vec<String> = vec!["aifo-coder".to_string()];
    words.extend(child_args.iter().cloned());
    let cmd = words
        .iter()
        .map(|w| ps_quote_inner(w))
        .collect::<Vec<_>>()
        .join(" ");
    let setloc = format!(
        "Set-Location {}",
        ps_quote_inner(&pane_dir.display().to_string())
    );
    format!("{}; {}; {}", setloc, assigns.join("; "), cmd)
}

#[cfg(windows)]
/// Build the Git Bash inner command for fork panes (used by tests).
pub fn fork_bash_inner_string(
    agent: &str,
    sid: &str,
    i: usize,
    pane_dir: &std::path::Path,
    pane_state_dir: &std::path::Path,
    child_args: &[String],
) -> String {
    let cname = format!("aifo-coder-{}-{}-{}", agent, sid, i);
    let kv = [
        ("AIFO_CODER_SKIP_LOCK", "1".to_string()),
        ("AIFO_CODER_CONTAINER_NAME", cname.clone()),
        ("AIFO_CODER_HOSTNAME", cname),
        ("AIFO_CODER_FORK_SESSION", sid.to_string()),
        ("AIFO_CODER_FORK_INDEX", i.to_string()),
        (
            "AIFO_CODER_FORK_STATE_DIR",
            pane_state_dir.display().to_string(),
        ),
    ];
    let mut exports: Vec<String> = Vec::new();
    for (k, v) in kv {
        exports.push(format!("export {}={}", k, shell_escape(&v)));
    }
    let mut words: Vec<String> = vec!["aifo-coder".to_string()];
    words.extend(child_args.iter().cloned());
    let cmd = shell_join(&words);
    let cddir = shell_escape(&pane_dir.display().to_string());
    format!("cd {} && {}; {}; exec bash", cddir, exports.join("; "), cmd)
}

#[cfg(windows)]
/// Map layout to wt.exe split orientation flag.
pub fn wt_orient_for_layout(layout: &str, i: usize) -> &'static str {
    match layout {
        "even-h" => "-H",
        "even-v" => "-V",
        _ => {
            if i % 2 == 0 {
                "-H"
            } else {
                "-V"
            }
        }
    }
}

#[cfg(windows)]
/// Build argument vector for `wt new-tab -d <dir> <psbin> -NoExit -Command <inner>`.
pub fn wt_build_new_tab_args(
    psbin: &std::path::Path,
    pane_dir: &std::path::Path,
    inner: &str,
) -> Vec<String> {
    vec![
        "wt".to_string(),
        "new-tab".to_string(),
        "-d".to_string(),
        pane_dir.display().to_string(),
        psbin.display().to_string(),
        "-NoExit".to_string(),
        "-Command".to_string(),
        inner.to_string(),
    ]
}

#[cfg(windows)]
/// Build argument vector for `wt split-pane <orient> -d <dir> <psbin> -NoExit -Command <inner>`.
pub fn wt_build_split_args(
    orient: &str,
    psbin: &std::path::Path,
    pane_dir: &std::path::Path,
    inner: &str,
) -> Vec<String> {
    vec![
        "wt".to_string(),
        "split-pane".to_string(),
        orient.to_string(),
        "-d".to_string(),
        pane_dir.display().to_string(),
        psbin.display().to_string(),
        "-NoExit".to_string(),
        "-Command".to_string(),
        inner.to_string(),
    ]
}

#[cfg(windows)]
/// Build a PowerShell Wait-Process command from a list of PIDs.
pub fn ps_wait_process_cmd(ids: &[&str]) -> String {
    format!("Wait-Process -Id {}", ids.join(","))
}


#[allow(dead_code)]
static PASS_ENV_VARS: Lazy<Vec<&'static str>> = Lazy::new(|| {
    vec![
        // AIFO master env (single source of truth)
        "AIFO_API_KEY",
        "AIFO_API_BASE",
        "AIFO_API_VERSION",
        // Git author/committer overrides
        "GIT_AUTHOR_NAME",
        "GIT_AUTHOR_EMAIL",
        "GIT_COMMITTER_NAME",
        "GIT_COMMITTER_EMAIL",
        // GPG signing controls
        "AIFO_CODER_GIT_SIGN",
        "GIT_SIGNING_KEY",
        // Timezone
        "TZ",
        // Editor preferences
        "EDITOR",
        "VISUAL",
        "TERM",
        // Tool-exec proxy (Phase 2)
        "AIFO_TOOLEEXEC_URL",
        "AIFO_TOOLEEXEC_TOKEN",
    ]
});

// -------- Color mode and helpers --------


/// Print a standardized warning line to stderr (color-aware).
pub fn warn_print(msg: &str) {
    let use_err = color_enabled_stderr();
    eprintln!(
        "{}",
        paint(use_err, "\x1b[33;1m", &format!("warning: {}", msg))
    );
}

/// Print warning lines and, when interactive, prompt the user to continue or abort.
/// Returns true to continue, false to abort.
pub fn warn_prompt_continue_or_quit(lines: &[&str]) -> bool {
    let use_err = color_enabled_stderr();
    for l in lines {
        eprintln!(
            "{}",
            paint(use_err, "\x1b[33;1m", &format!("warning: {}", l))
        );
    }

    // Only prompt when interactive and not disabled by env/CI
    let interactive = atty::is(atty::Stream::Stdin) && atty::is(atty::Stream::Stderr);
    let disabled = std::env::var("AIFO_CODER_NO_WARN_PAUSE").ok().as_deref() == Some("1")
        || std::env::var("CI").ok().as_deref() == Some("1");
    if !(interactive && !disabled) {
        return true;
    }

    eprint!(
        "{}",
        paint(
            use_err,
            "\x1b[90m",
            "Press Enter to continue, or 'q' to abort: "
        )
    );
    let _ = std::io::stderr().flush();

    // Windows: read a single key without waiting for Enter using _getch
    #[cfg(windows)]
    {
        unsafe {
            #[link(name = "msvcrt")]
            extern "C" {
                fn _getch() -> i32;
            }
            let ch = _getch();
            let ch = (ch as u8) as char;
            if ch == 'q' || ch == 'Q' {
                // End the prompt line and add an extra blank line for visual separation
                eprintln!();
                eprintln!();
                return false;
            } else {
                // End the prompt line and add an extra blank line for visual separation
                eprintln!();
                eprintln!();
                return true;
            }
        }
    }

    // Unix: temporarily switch terminal to non-canonical, no-echo mode to read a single byte
    #[cfg(unix)]
    {
        // Save current stty state
        let saved = std::process::Command::new("stty")
            .arg("-g")
            .output()
            .ok()
            .and_then(|o| {
                if o.status.success() {
                    Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                } else {
                    None
                }
            });

        // Best-effort: set non-canonical mode, no echo, 1-byte min
        let _ = std::process::Command::new("stty")
            .args(["-icanon", "min", "1", "-echo"])
            .status();

        let mut buf = [0u8; 1];
        let _ = std::io::stdin().read(&mut buf);

        // Restore previous stty state (or sane fallback)
        if let Some(state) = saved {
            let _ = std::process::Command::new("stty").arg(&state).status();
        } else {
            let _ = std::process::Command::new("stty").arg("sane").status();
        }

        let ch = buf[0] as char;
        if ch == 'q' || ch == 'Q' {
            // End the prompt line and add an extra blank line for visual separation
            eprintln!();
            eprintln!();
            return false;
        } else {
            // End the prompt line and add an extra blank line for visual separation
            eprintln!();
            eprintln!();
            return true;
        }
    }

    #[cfg(not(any(unix, windows)))]
    {
        // Fallback: line-based input (non-tty or platforms without single-key support)
        let mut s = String::new();
        let _ = std::io::stdin().read_line(&mut s);
        // After confirmation, print a blank line for visual separation
        eprintln!();
        eprintln!();
        let c = s.trim().chars().next().unwrap_or('\n');
        return c != 'q' && c != 'Q';
    }
}

/**
 Merging strategy for post-fork actions.
 - None: do nothing (default).
 - Fetch: fetch pane branches back into the original repository as local branches.
 - Octopus: fetch branches then attempt an octopus merge into a merge/<sid> branch.
*/
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, ValueEnum)]
pub enum MergingStrategy {
    #[value(name = "none")]
    None,
    #[value(name = "fetch")]
    Fetch,
    #[value(name = "octopus")]
    Octopus,
}









/// Determine the preferred registry prefix for image references.
/// Precedence:
/// 1) If AIFO_CODER_REGISTRY_PREFIX is set:
///    - empty string forces Docker Hub (no prefix)
///    - non-empty is normalized to end with a single '/' and used as-is
/// 2) Otherwise, if repository.migros.net:443 is reachable, use "repository.migros.net/"
/// 3) Fallback: empty string (Docker Hub)
#[cfg(any())]
pub(crate) fn preferred_registry_prefix_legacy() -> String {
    // Env override always takes precedence within the current process
    if let Ok(pref) = env::var("AIFO_CODER_REGISTRY_PREFIX") {
        let trimmed = pref.trim();
        if trimmed.is_empty() {
            eprintln!("aifo-coder: AIFO_CODER_REGISTRY_PREFIX override set to empty; using Docker Hub (no registry prefix).");
            let v = String::new();
            let _ = REGISTRY_PREFIX_CACHE.set(v.clone());
            let _ = REGISTRY_PREFIX_SOURCE.set("env-empty".to_string());
            write_registry_cache_disk(&v);
            return v;
        }
        let mut s = trimmed.trim_end_matches('/').to_string();
        s.push('/');
        eprintln!(
            "aifo-coder: Using AIFO_CODER_REGISTRY_PREFIX override: '{}'",
            s
        );
        let v = s;
        let _ = REGISTRY_PREFIX_CACHE.set(v.clone());
        let _ = REGISTRY_PREFIX_SOURCE.set("env".to_string());
        write_registry_cache_disk(&v);
        return v;
    }
    // Test override (without env): allow forcing probe result deterministically (does not touch OnceCell caches)
    if let Some(mode) = REGISTRY_PROBE_OVERRIDE
        .lock()
        .expect("probe override lock")
        .clone()
    {
        return match mode {
            RegistryProbeTestMode::CurlOk => "repository.migros.net/".to_string(),
            RegistryProbeTestMode::CurlFail => String::new(),
            RegistryProbeTestMode::TcpOk => "repository.migros.net/".to_string(),
            RegistryProbeTestMode::TcpFail => String::new(),
        };
    }
    // Test override (without env): allow forcing probe result deterministically (does not touch OnceCell caches)
    if let Some(mode) = REGISTRY_PROBE_OVERRIDE
        .lock()
        .expect("probe override lock")
        .clone()
    {
        return match mode {
            RegistryProbeTestMode::CurlOk => "repository.migros.net/".to_string(),
            RegistryProbeTestMode::CurlFail => String::new(),
            RegistryProbeTestMode::TcpOk => "repository.migros.net/".to_string(),
            RegistryProbeTestMode::TcpFail => String::new(),
        };
    }
    // Test override (without env): allow forcing probe result deterministically (does not touch OnceCell caches)
    if let Some(mode) = REGISTRY_PROBE_OVERRIDE
        .lock()
        .expect("probe override lock")
        .clone()
    {
        return match mode {
            RegistryProbeTestMode::CurlOk => "repository.migros.net/".to_string(),
            RegistryProbeTestMode::CurlFail => String::new(),
            RegistryProbeTestMode::TcpOk => "repository.migros.net/".to_string(),
            RegistryProbeTestMode::TcpFail => String::new(),
        };
    }
    // Test override (without env): allow forcing probe result deterministically (does not touch OnceCell caches)
    if let Some(mode) = REGISTRY_PROBE_OVERRIDE
        .lock()
        .expect("probe override lock")
        .clone()
    {
        return match mode {
            RegistryProbeTestMode::CurlOk => "repository.migros.net/".to_string(),
            RegistryProbeTestMode::CurlFail => String::new(),
            RegistryProbeTestMode::TcpOk => "repository.migros.net/".to_string(),
            RegistryProbeTestMode::TcpFail => String::new(),
        };
    }
    // Test override (without env): allow forcing probe result deterministically (does not touch OnceCell caches)
    if let Some(mode) = REGISTRY_PROBE_OVERRIDE
        .lock()
        .expect("probe override lock")
        .clone()
    {
        return match mode {
            RegistryProbeTestMode::CurlOk => "repository.migros.net/".to_string(),
            RegistryProbeTestMode::CurlFail => String::new(),
            RegistryProbeTestMode::TcpOk => "repository.migros.net/".to_string(),
            RegistryProbeTestMode::TcpFail => String::new(),
        };
    }
    // Test override (without env): allow forcing probe result deterministically (does not touch OnceCell caches)
    if let Some(mode) = REGISTRY_PROBE_OVERRIDE
        .lock()
        .expect("probe override lock")
        .clone()
    {
        return match mode {
            RegistryProbeTestMode::CurlOk => "repository.migros.net/".to_string(),
            RegistryProbeTestMode::CurlFail => String::new(),
            RegistryProbeTestMode::TcpOk => "repository.migros.net/".to_string(),
            RegistryProbeTestMode::TcpFail => String::new(),
        };
    }
    // Test override (without env): allow forcing probe result deterministically (does not touch OnceCell caches)
    if let Some(mode) = REGISTRY_PROBE_OVERRIDE
        .lock()
        .expect("probe override lock")
        .clone()
    {
        return match mode {
            RegistryProbeTestMode::CurlOk => "repository.migros.net/".to_string(),
            RegistryProbeTestMode::CurlFail => String::new(),
            RegistryProbeTestMode::TcpOk => "repository.migros.net/".to_string(),
            RegistryProbeTestMode::TcpFail => String::new(),
        };
    }
    // Test override (without env): allow forcing probe result deterministically (does not touch OnceCell caches)
    if let Some(mode) = REGISTRY_PROBE_OVERRIDE
        .lock()
        .expect("probe override lock")
        .clone()
    {
        return match mode {
            RegistryProbeTestMode::CurlOk => "repository.migros.net/".to_string(),
            RegistryProbeTestMode::CurlFail => String::new(),
            RegistryProbeTestMode::TcpOk => "repository.migros.net/".to_string(),
            RegistryProbeTestMode::TcpFail => String::new(),
        };
    }
    // Test override (without env): allow forcing probe result deterministically (does not touch OnceCell caches)
    if let Some(mode) = REGISTRY_PROBE_OVERRIDE
        .lock()
        .expect("probe override lock")
        .clone()
    {
        return match mode {
            RegistryProbeTestMode::CurlOk => "repository.migros.net/".to_string(),
            RegistryProbeTestMode::CurlFail => String::new(),
            RegistryProbeTestMode::TcpOk => "repository.migros.net/".to_string(),
            RegistryProbeTestMode::TcpFail => String::new(),
        };
    }
    // Test override (without env): allow forcing probe result deterministically (does not touch OnceCell caches)
    if let Some(mode) = REGISTRY_PROBE_OVERRIDE
        .lock()
        .expect("probe override lock")
        .clone()
    {
        return match mode {
            RegistryProbeTestMode::CurlOk => "repository.migros.net/".to_string(),
            RegistryProbeTestMode::CurlFail => String::new(),
            RegistryProbeTestMode::TcpOk => "repository.migros.net/".to_string(),
            RegistryProbeTestMode::TcpFail => String::new(),
        };
    }
    // Test override (without env): allow forcing probe result deterministically (does not touch OnceCell caches)
    if let Some(mode) = REGISTRY_PROBE_OVERRIDE
        .lock()
        .expect("probe override lock")
        .clone()
    {
        return match mode {
            RegistryProbeTestMode::CurlOk => "repository.migros.net/".to_string(),
            RegistryProbeTestMode::CurlFail => String::new(),
            RegistryProbeTestMode::TcpOk => "repository.migros.net/".to_string(),
            RegistryProbeTestMode::TcpFail => String::new(),
        };
    }
    // Test override (without env): allow forcing probe result deterministically (does not touch OnceCell caches)
    if let Some(mode) = REGISTRY_PROBE_OVERRIDE
        .lock()
        .expect("probe override lock")
        .clone()
    {
        return match mode {
            RegistryProbeTestMode::CurlOk => "repository.migros.net/".to_string(),
            RegistryProbeTestMode::CurlFail => String::new(),
            RegistryProbeTestMode::TcpOk => "repository.migros.net/".to_string(),
            RegistryProbeTestMode::TcpFail => String::new(),
        };
    }
    // Test hook: allow forcing probe result deterministically (does not touch OnceCell caches)
    if let Ok(mode) = env::var("AIFO_CODER_TEST_REGISTRY_PROBE") {
        let ml = mode.to_ascii_lowercase();
        return match ml.as_str() {
            "curl-ok" => "repository.migros.net/".to_string(),
            "curl-fail" => String::new(),
            "tcp-ok" => "repository.migros.net/".to_string(),
            "tcp-fail" => String::new(),
            _ => String::new(),
        };
    }
    if let Some(v) = REGISTRY_PREFIX_CACHE.get() {
        return v.clone();
    }

    // Disk cache disabled: always probe with curl/TCP in this run.

    // Prefer probing with curl for HTTPS reachability using short timeouts.
    if which("curl").is_ok() {
        eprintln!("aifo-coder: checking https://repository.migros.net/v2/ availability with: curl --connect-timeout 1 --max-time 2 -sSI ...");
        let status = Command::new("curl")
            .args([
                "--connect-timeout",
                "1",
                "--max-time",
                "2",
                "-sSI",
                "https://repository.migros.net/v2/",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        if let Ok(st) = status {
            if st.success() {
                eprintln!("aifo-coder: repository.migros.net reachable; using registry prefix 'repository.migros.net/'.");
                let v = "repository.migros.net/".to_string();
                let _ = REGISTRY_PREFIX_CACHE.set(v.clone());
                let _ = REGISTRY_PREFIX_SOURCE.set("curl".to_string());
                write_registry_cache_disk(&v);
                return v;
            } else {
                eprintln!("aifo-coder: repository.migros.net not reachable (curl non-zero exit); using Docker Hub (no prefix).");
                let v = String::new();
                let _ = REGISTRY_PREFIX_CACHE.set(v.clone());
                let _ = REGISTRY_PREFIX_SOURCE.set("curl".to_string());
                write_registry_cache_disk(&v);
                return v;
            }
        } else {
            eprintln!(
                "aifo-coder: curl invocation failed; falling back to TCP reachability check."
            );
        }
    } else {
        eprintln!("aifo-coder: curl not found; falling back to TCP reachability check.");
    }

    // Fallback quick TCP probe (short timeout).
    let v = if is_host_port_reachable("repository.migros.net", 443, 300) {
        eprintln!("aifo-coder: repository.migros.net appears reachable via TCP; using registry prefix 'repository.migros.net/'.");
        "repository.migros.net/".to_string()
    } else {
        eprintln!("aifo-coder: repository.migros.net not reachable via TCP; using Docker Hub (no prefix).");
        String::new()
    };
    let _ = REGISTRY_PREFIX_CACHE.set(v.clone());
    let _ = REGISTRY_PREFIX_SOURCE.set("tcp".to_string());
    write_registry_cache_disk(&v);
    v
}



 /// Render a docker -v host:container pair.
pub fn path_pair(host: &std::path::Path, container: &str) -> std::ffi::OsString {
    std::ffi::OsString::from(format!("{}:{container}", host.display()))
}

/// Ensure a file exists by creating parent directories as needed.
pub fn ensure_file_exists(p: &std::path::Path) -> std::io::Result<()> {
    if !p.exists() {
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::File::create(p)?;
    }
    Ok(())
}




//// Repository-scoped locking helpers and candidate paths





/// Build the docker run command for the given agent invocation, and return a preview string.
#[allow(dead_code)]
#[cfg(any())]
pub(crate) fn build_docker_cmd_legacy(
    agent: &str,
    passthrough: &[String],
    image: &str,
    apparmor_profile: Option<&str>,
) -> io::Result<(Command, String)> {
    let runtime = container_runtime_path()?;

    // TTY flags
    let tty_flags: Vec<&str> = if atty::is(atty::Stream::Stdin) || atty::is(atty::Stream::Stdout) {
        vec!["-it"]
    } else {
        vec!["-i"]
    };

    let pwd = {
        let p = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        fs::canonicalize(&p).unwrap_or(p)
    };

    // UID/GID mapping
    #[cfg(unix)]
    let (uid, gid) = { (u32::from(getuid()), u32::from(getgid())) };

    // Forward selected env vars (inherit from host)
    let mut env_flags: Vec<OsString> = Vec::new();
    for var in PASS_ENV_VARS.iter().copied() {
        if let Ok(val) = env::var(var) {
            if !val.is_empty() {
                env_flags.push(OsString::from("-e"));
                env_flags.push(OsString::from(var));
            }
        }
    }

    // Always set these inside container
    env_flags.push(OsString::from("-e"));
    env_flags.push(OsString::from("HOME=/home/coder"));
    env_flags.push(OsString::from("-e"));
    env_flags.push(OsString::from("USER=coder"));
    env_flags.push(OsString::from("-e"));
    env_flags.push(OsString::from("CODEX_HOME=/home/coder/.codex"));
    env_flags.push(OsString::from("-e"));
    env_flags.push(OsString::from("GNUPGHOME=/home/coder/.gnupg"));

    // XDG_RUNTIME_DIR for gpg-agent sockets
    #[cfg(unix)]
    {
        env_flags.push(OsString::from("-e"));
        env_flags.push(OsString::from(format!(
            "XDG_RUNTIME_DIR=/tmp/runtime-{uid}"
        )));
    }
    // Ensure pinentry can bind to the terminal when interactive sessions are used
    if atty::is(atty::Stream::Stdin) || atty::is(atty::Stream::Stdout) {
        env_flags.push(OsString::from("-e"));
        env_flags.push(OsString::from("GPG_TTY=/dev/tty"));
    }

    // Map unified AIFO_* environment to agent-specific variables
    if let Ok(v) = env::var("AIFO_API_KEY") {
        if !v.is_empty() {
            // OpenAI-style
            env_flags.push(OsString::from("-e"));
            env_flags.push(OsString::from(format!("OPENAI_API_KEY={v}")));
            // Azure-style
            env_flags.push(OsString::from("-e"));
            env_flags.push(OsString::from(format!("AZURE_OPENAI_API_KEY={v}")));
            env_flags.push(OsString::from("-e"));
            env_flags.push(OsString::from(format!("AZURE_API_KEY={v}")));
        }
    }
    if let Ok(v) = env::var("AIFO_API_BASE") {
        if !v.is_empty() {
            // OpenAI-style base URL
            env_flags.push(OsString::from("-e"));
            env_flags.push(OsString::from(format!("OPENAI_BASE_URL={v}")));
            env_flags.push(OsString::from("-e"));
            env_flags.push(OsString::from(format!("OPENAI_API_BASE={v}")));
            // Azure-style endpoint/base
            env_flags.push(OsString::from("-e"));
            env_flags.push(OsString::from(format!("AZURE_OPENAI_ENDPOINT={v}")));
            env_flags.push(OsString::from("-e"));
            env_flags.push(OsString::from(format!("AZURE_API_BASE={v}")));
            // Hint some clients that this is Azure-backed endpoint
            env_flags.push(OsString::from("-e"));
            env_flags.push(OsString::from("OPENAI_API_TYPE=azure"));
        }
    }
    if let Ok(v) = env::var("AIFO_API_VERSION") {
        if !v.is_empty() {
            // OpenAI-style API version (used by some clients for Azure)
            env_flags.push(OsString::from("-e"));
            env_flags.push(OsString::from(format!("OPENAI_API_VERSION={v}")));
            // Azure-style version
            env_flags.push(OsString::from("-e"));
            env_flags.push(OsString::from(format!("AZURE_OPENAI_API_VERSION={v}")));
            env_flags.push(OsString::from("-e"));
            env_flags.push(OsString::from(format!("AZURE_API_VERSION={v}")));
        }
    }
    // Phase 2: pass through tool-exec proxy URL and token if set
    if let Ok(v) = env::var("AIFO_TOOLEEXEC_URL") {
        if !v.is_empty() {
            env_flags.push(OsString::from("-e"));
            env_flags.push(OsString::from(format!("AIFO_TOOLEEXEC_URL={v}")));
        }
    }
    if let Ok(v) = env::var("AIFO_TOOLEEXEC_TOKEN") {
        if !v.is_empty() {
            env_flags.push(OsString::from("-e"));
            env_flags.push(OsString::from(format!("AIFO_TOOLEEXEC_TOKEN={v}")));
        }
    }

    // Disable commit signing for Aider if requested
    if agent == "aider" {
        if let Ok(v) = env::var("AIFO_CODER_GIT_SIGN") {
            let vl = v.to_lowercase();
            if ["0", "false", "no", "off"].contains(&vl.as_str()) {
                env_flags.push(OsString::from("-e"));
                env_flags.push(OsString::from("GIT_CONFIG_COUNT=1"));
                env_flags.push(OsString::from("-e"));
                env_flags.push(OsString::from("GIT_CONFIG_KEY_0=commit.gpgsign"));
                env_flags.push(OsString::from("-e"));
                env_flags.push(OsString::from("GIT_CONFIG_VALUE_0=false"));
            }
        }
    }

    // Volume mounts and host prep
    let mut volume_flags: Vec<OsString> = Vec::new();
    let host_home = home::home_dir().unwrap_or_else(|| PathBuf::from(""));

    // Per-pane state mounts (Phase 1): when AIFO_CODER_FORK_STATE_DIR is set, mount per-pane
    // .aider/.codex/.crush directories instead of HOME-based equivalents to avoid shared-state races.
    if let Ok(state_dir) = env::var("AIFO_CODER_FORK_STATE_DIR") {
        let sd = state_dir.trim();
        if !sd.is_empty() {
            let base = PathBuf::from(sd);
            let aider_dir = base.join(".aider");
            let codex_dir = base.join(".codex");
            let crush_dir = base.join(".crush");
            let _ = fs::create_dir_all(&aider_dir);
            let _ = fs::create_dir_all(&codex_dir);
            let _ = fs::create_dir_all(&crush_dir);
            volume_flags.push(OsString::from("-v"));
            volume_flags.push(path_pair(&aider_dir, "/home/coder/.aider"));
            volume_flags.push(OsString::from("-v"));
            volume_flags.push(path_pair(&codex_dir, "/home/coder/.codex"));
            volume_flags.push(OsString::from("-v"));
            volume_flags.push(path_pair(&crush_dir, "/home/coder/.crush"));
        } else {
            // Fallback to legacy HOME-based mounts if the env var is empty
            // Crush state
            let crush_dir = host_home.join(".local").join("share").join("crush");
            fs::create_dir_all(&crush_dir).ok();
            volume_flags.push(OsString::from("-v"));
            volume_flags.push(path_pair(&crush_dir, "/home/coder/.local/share/crush"));
            // Additional Crush state directory (~/.crush)
            let crush_state_dir = host_home.join(".crush");
            fs::create_dir_all(&crush_state_dir).ok();
            volume_flags.push(OsString::from("-v"));
            volume_flags.push(path_pair(&crush_state_dir, "/home/coder/.crush"));

            // Codex state
            let codex_dir = host_home.join(".codex");
            fs::create_dir_all(&codex_dir).ok();
            volume_flags.push(OsString::from("-v"));
            volume_flags.push(path_pair(&codex_dir, "/home/coder/.codex"));

            // Aider state dir
            let aider_dir = host_home.join(".aider");
            fs::create_dir_all(&aider_dir).ok();
            volume_flags.push(OsString::from("-v"));
            volume_flags.push(path_pair(&aider_dir, "/home/coder/.aider"));
        }
    } else {
        // Legacy HOME-based mounts (non-fork mode)
        // Crush state
        let crush_dir = host_home.join(".local").join("share").join("crush");
        fs::create_dir_all(&crush_dir).ok();
        volume_flags.push(OsString::from("-v"));
        volume_flags.push(path_pair(&crush_dir, "/home/coder/.local/share/crush"));
        // Additional Crush state directory (~/.crush)
        let crush_state_dir = host_home.join(".crush");
        fs::create_dir_all(&crush_state_dir).ok();
        volume_flags.push(OsString::from("-v"));
        volume_flags.push(path_pair(&crush_state_dir, "/home/coder/.crush"));

        // Codex state
        let codex_dir = host_home.join(".codex");
        fs::create_dir_all(&codex_dir).ok();
        volume_flags.push(OsString::from("-v"));
        volume_flags.push(path_pair(&codex_dir, "/home/coder/.codex"));

        // Aider state dir
        let aider_dir = host_home.join(".aider");
        fs::create_dir_all(&aider_dir).ok();
        volume_flags.push(OsString::from("-v"));
        volume_flags.push(path_pair(&aider_dir, "/home/coder/.aider"));
    }

    // Aider root-level config files
    for fname in [
        ".aider.conf.yml",
        ".aider.model.metadata.json",
        ".aider.model.settings.yml",
    ] {
        let src = host_home.join(fname);
        ensure_file_exists(&src).ok();
        volume_flags.push(OsString::from("-v"));
        volume_flags.push(path_pair(&src, &format!("/home/coder/{fname}")));
    }

    // Git config
    let gitconfig = host_home.join(".gitconfig");
    ensure_file_exists(&gitconfig).ok();
    volume_flags.push(OsString::from("-v"));
    volume_flags.push(path_pair(&gitconfig, "/home/coder/.gitconfig"));

    // Timezone files (optional)
    for (host_path, container_path) in [
        ("/etc/localtime", "/etc/localtime"),
        ("/etc/timezone", "/etc/timezone"),
    ] {
        let hp = Path::new(host_path);
        if hp.exists() {
            volume_flags.push(OsString::from("-v"));
            volume_flags.push(OsString::from(format!(
                "{}:{}:ro",
                hp.display(),
                container_path
            )));
        }
    }

    // Host logs dir
    let host_logs_dir = pwd.join("build").join("logs");
    fs::create_dir_all(&host_logs_dir).ok();
    volume_flags.push(OsString::from("-v"));
    volume_flags.push(path_pair(&host_logs_dir, "/var/log/host"));

    // GnuPG: mount host ~/.gnupg read-only to /home/coder/.gnupg-host
    let gnupg_dir = host_home.join(".gnupg");
    fs::create_dir_all(&gnupg_dir).ok();
    // Best effort permission
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&gnupg_dir, fs::Permissions::from_mode(0o700));
    }
    volume_flags.push(OsString::from("-v"));
    volume_flags.push(OsString::from(format!(
        "{}:/home/coder/.gnupg-host:ro",
        gnupg_dir.display()
    )));
    // Optional: mount host-provided shim directory into PATH front inside the agent
    if let Ok(shim_dir) = env::var("AIFO_SHIM_DIR") {
        if !shim_dir.trim().is_empty() {
            volume_flags.push(OsString::from("-v"));
            volume_flags.push(OsString::from(format!("{}:/opt/aifo/bin:ro", shim_dir)));
        }
    }

    // Phase 4 (Linux): mount unix socket directory if unix transport is enabled
    if let Ok(dir) = env::var("AIFO_TOOLEEXEC_UNIX_DIR") {
        if !dir.trim().is_empty() {
            volume_flags.push(OsString::from("-v"));
            volume_flags.push(OsString::from(format!("{}:/run/aifo", dir)));
        }
    }

    // User mapping
    #[allow(unused_mut)]
    let mut user_flags: Vec<OsString> = Vec::new();
    #[cfg(unix)]
    {
        user_flags.push(OsString::from("--user"));
        user_flags.push(OsString::from(format!("{uid}:{gid}")));
    }

    // AppArmor security flags
    let mut security_flags: Vec<OsString> = Vec::new();
    if let Some(profile) = apparmor_profile {
        if docker_supports_apparmor() {
            security_flags.push(OsString::from("--security-opt"));
            security_flags.push(OsString::from(format!("apparmor={profile}")));
        } else {
            warn_print(
                "docker daemon does not report apparmor support. continuing without apparmor.",
            );
        }
    }
    // Image prefix used for container naming
    let prefix = env::var("AIFO_CODER_IMAGE_PREFIX").unwrap_or_else(|_| "aifo-coder".to_string());

    // Container name/hostname
    // Default to a unique per-run container name to avoid conflicts across concurrent runs,
    // while still honoring explicit overrides via environment variables.
    let container_name = env::var("AIFO_CODER_CONTAINER_NAME")
        .unwrap_or_else(|_| format!("{}-{}-{}", prefix, agent, create_session_id()));
    let hostname = env::var("AIFO_CODER_HOSTNAME").unwrap_or_else(|_| container_name.clone());
    let name_flags = vec![
        OsString::from("--name"),
        OsString::from(&container_name),
        OsString::from("--hostname"),
        OsString::from(&hostname),
    ];

    // Agent command vector and join with shell escaping
    let mut agent_cmd = vec![agent.to_string()];
    agent_cmd.extend(passthrough.iter().cloned());
    let agent_joined = shell_join(&agent_cmd);

    // Shell command inside container (copied from Python implementation)
    let sh_cmd = format!(
        "set -e; umask 077; \
         export PATH=\"/opt/aifo/bin:/opt/venv/bin:$PATH\"; \
         uid=\"$(id -u)\"; gid=\"$(id -g)\"; \
         mkdir -p \"$HOME\" \"$GNUPGHOME\"; chmod 700 \"$HOME\" \"$GNUPGHOME\" 2>/dev/null || true; chown \"$uid:$gid\" \"$HOME\" 2>/dev/null || true; \
         if (command -v getent >/dev/null 2>&1 && ! getent passwd \"$uid\" >/dev/null 2>&1) || (! command -v getent >/dev/null 2>&1 && ! grep -q \"^[^:]*:[^:]*:$uid:\" /etc/passwd); then \
           mkdir -p \"$HOME/.nss_wrapper\"; \
           PASSWD_FILE=\"$HOME/.nss_wrapper/passwd\"; GROUP_FILE=\"$HOME/.nss_wrapper/group\"; \
           echo \"coder:x:${{uid}}:${{gid}}:,,,:$HOME:/bin/sh\" > \"$PASSWD_FILE\"; \
           echo \"coder:x:${{gid}}:\" > \"$GROUP_FILE\"; \
           for so in /usr/lib/*/libnss_wrapper.so /usr/lib/*/libnss_wrapper.so.* /usr/lib/libnss_wrapper.so /lib/*/libnss_wrapper.so /lib/*/libnss_wrapper.so.*; do if [ -f \"$so\" ]; then export LD_PRELOAD=\"${{LD_PRELOAD:+$LD_PRELOAD:}}$so\"; break; fi; done; \
           export NSS_WRAPPER_PASSWD=\"$PASSWD_FILE\" NSS_WRAPPER_GROUP=\"$GROUP_FILE\" USER=\"coder\" LOGNAME=\"coder\"; \
         fi; \
         if [ -n \"${{XDG_RUNTIME_DIR:-}}\" ]; then mkdir -p \"$XDG_RUNTIME_DIR/gnupg\" || true; chmod 700 \"$XDG_RUNTIME_DIR\" \"$XDG_RUNTIME_DIR/gnupg\" 2>/dev/null || true; fi; \
         mkdir -p \"$HOME/.aifo-logs\" || true; \
         if [ -t 0 ] || [ -t 1 ]; then export GPG_TTY=\"$(tty 2>/dev/null || echo /dev/tty)\"; fi; \
         touch \"$GNUPGHOME/gpg-agent.conf\"; sed -i \"/^pinentry-program /d\" \"$GNUPGHOME/gpg-agent.conf\" 2>/dev/null || true; echo \"pinentry-program /usr/bin/pinentry-curses\" >> \"$GNUPGHOME/gpg-agent.conf\"; \
         sed -i \"/^log-file /d;/^debug-level /d;/^verbose$/d\" \"$GNUPGHOME/gpg-agent.conf\" 2>/dev/null || true; \
         echo \"log-file /home/coder/.gnupg/gpg-agent.log\" >> \"$GNUPGHOME/gpg-agent.conf\"; echo \"debug-level basic\" >> \"$GNUPGHOME/gpg-agent.conf\"; echo \"verbose\" >> \"$GNUPGHOME/gpg-agent.conf\"; \
         if ! grep -q \"^allow-loopback-pinentry\" \"$GNUPGHOME/gpg-agent.conf\" 2>/dev/null; then echo \"allow-loopback-pinentry\" >> \"$GNUPGHOME/gpg-agent.conf\"; fi; \
         if ! grep -q \"^default-cache-ttl \" \"$GNUPGHOME/gpg-agent.conf\" 2>/dev/null; then echo \"default-cache-ttl 7200\" >> \"$GNUPGHOME/gpg-agent.conf\"; fi; \
         if ! grep -q \"^max-cache-ttl \" \"$GNUPGHOME/gpg-agent.conf\" 2>/dev/null; then echo \"max-cache-ttl 86400\" >> \"$GNUPGHOME/gpg-agent.conf\"; fi; \
         for item in private-keys-v1.d openpgp-revocs.d pubring.kbx trustdb.gpg gpg.conf; do \
           if [ ! -e \"$GNUPGHOME/$item\" ] && [ -e \"/home/coder/.gnupg-host/$item\" ]; then \
             cp -a \"/home/coder/.gnupg-host/$item\" \"$GNUPGHOME/\" 2>/dev/null || true; \
           fi; \
         done; \
         touch \"$GNUPGHOME/gpg.conf\"; sed -i \"/^pinentry-mode /d\" \"$GNUPGHOME/gpg.conf\" 2>/dev/null || true; echo \"pinentry-mode loopback\" >> \"$GNUPGHOME/gpg.conf\"; \
         chmod -R go-rwx \"$GNUPGHOME\" 2>/dev/null || true; \
         unset GPG_AGENT_INFO; gpgconf --kill gpg-agent >/dev/null 2>&1 || true; \
         gpgconf --launch gpg-agent >/dev/null 2>&1 || true; \
         if [ -f \"/var/log/host/apparmor.log\" ]; then (nohup sh -c \"tail -n0 -F /var/log/host/apparmor.log >> \\\"$HOME/.aifo-logs/apparmor.log\\\" 2>&1\" >/dev/null 2>&1 &); fi; \
         repo_name=\"$(git -C /workspace config --get user.name 2>/dev/null || true)\"; \
         repo_email=\"$(git -C /workspace config --get user.email 2>/dev/null || true)\"; \
         global_name=\"$(git config --global --get user.name 2>/dev/null || true)\"; \
         global_email=\"$(git config --global --get user.email 2>/dev/null || true)\"; \
         name=\"${{GIT_AUTHOR_NAME:-${{repo_name}}}}\"; [ -z \"$name\" ] || [ \"$name\" = \"Your Name\" ] && name=\"${{global_name:-$name}}\"; \
         email=\"${{GIT_AUTHOR_EMAIL:-${{repo_email}}}}\"; [ -z \"$email\" ] || [ \"$email\" = \"you@example.com\" ] && email=\"${{global_email:-$email}}\"; \
         if [ -n \"$name\" ]; then export GIT_AUTHOR_NAME=\"$name\" GIT_COMMITTER_NAME=\"$name\"; fi; \
         if [ -n \"$email\" ]; then export GIT_AUTHOR_EMAIL=\"$email\" GIT_COMMITTER_EMAIL=\"$email\"; fi; \
         case \"${{AIFO_CODER_GIT_SIGN:-}}\" in 0|false|FALSE|no|NO|off) want_sign=0 ;; *) want_sign=1 ;; esac; \
         if [ -d \"/workspace/.git\" ]; then \
           if [ \"$want_sign\" = \"1\" ]; then \
             git -C /workspace config --get commit.gpgsign >/dev/null 2>&1 || git -C /workspace config commit.gpgsign true; \
             git -C /workspace config --get gpg.program >/dev/null 2>&1 || git -C /workspace config gpg.program gpg; \
             if [ -n \"${{GIT_SIGNING_KEY:-}}\" ]; then git -C /workspace config user.signingkey \"$GIT_SIGNING_KEY\"; \
             else skey=\"$(gpg --list-secret-keys --with-colons 2>/dev/null | grep ^fpr: | head -n1 | cut -d: -f10)\"; [ -n \"$skey\" ] && git -C /workspace config user.signingkey \"$skey\" || true; fi; \
           else \
             git -C /workspace config commit.gpgsign false || true; \
           fi; \
         fi; \
         exec {agent_joined}"
    );

    // docker run command
    let mut cmd = Command::new(runtime);
    let mut preview_args: Vec<String> = Vec::new();

    // program
    preview_args.push("docker".to_string());

    // subcommand and common flags
    cmd.arg("run").arg("--rm");
    preview_args.push("run".to_string());
    preview_args.push("--rm".to_string());

    // TTY flags
    for f in tty_flags {
        cmd.arg(f);
        preview_args.push(f.to_string());
    }

    // name/hostname
    for f in name_flags {
        preview_args.push(f.to_string_lossy().to_string());
        cmd.arg(f);
    }
    // Phase 2: join the ephemeral session network if provided
    if let Ok(net) = env::var("AIFO_SESSION_NETWORK") {
        if !net.trim().is_empty() {
            cmd.arg("--network").arg(&net);
            preview_args.push("--network".to_string());
            preview_args.push(net);
        }
    }
    // Phase 2 (Linux): make host.docker.internal resolvable to host-gateway
    #[cfg(target_os = "linux")]
    {
        if env::var("AIFO_TOOLEEXEC_ADD_HOST").ok().as_deref() == Some("1") {
            cmd.arg("--add-host")
                .arg("host.docker.internal:host-gateway");
            preview_args.push("--add-host".to_string());
            preview_args.push("host.docker.internal:host-gateway".to_string());
        }
    }

    // volumes
    for f in &volume_flags {
        preview_args.push(f.to_string_lossy().to_string());
        cmd.arg(f);
    }
    let workspace_mount = format!("{}:/workspace", pwd.display());
    cmd.arg("-v").arg(&workspace_mount);
    preview_args.push("-v".to_string());
    preview_args.push(workspace_mount);

    // workdir
    cmd.arg("-w").arg("/workspace");
    preview_args.push("-w".to_string());
    preview_args.push("/workspace".to_string());

    // env flags
    for f in &env_flags {
        preview_args.push(f.to_string_lossy().to_string());
        cmd.arg(f);
    }

    // user flags
    for f in &user_flags {
        preview_args.push(f.to_string_lossy().to_string());
        cmd.arg(f);
    }

    // security flags
    for f in &security_flags {
        preview_args.push(f.to_string_lossy().to_string());
        cmd.arg(f);
    }

    // image
    cmd.arg(image);
    preview_args.push(image.to_string());

    // shell and command
    cmd.arg("/bin/sh").arg("-lc").arg(&sh_cmd);
    preview_args.push("/bin/sh".to_string());
    preview_args.push("-lc".to_string());
    preview_args.push(sh_cmd.clone());

    // Render preview string with conservative shell escaping
    let preview = {
        let mut parts = Vec::with_capacity(preview_args.len());
        for p in preview_args {
            parts.push(shell_escape(&p));
        }
        parts.join(" ")
    };

    Ok((cmd, preview))
}






pub fn create_session_id() -> String {
    // Compose a short, mostly-unique ID from time and pid without extra deps
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0));
    let pid = std::process::id() as u128;
    let nanos = now.as_nanos();
    let mix = nanos ^ (pid as u128);
    // base36 encode last 40 bits for brevity
    let mut v = (mix & 0xffffffffff) as u64;
    let mut s = String::new();
    let alphabet = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if v == 0 {
        s.push('0');
    } else {
        while v > 0 {
            let idx = (v % 36) as usize;
            s.push(alphabet[idx] as char);
            v /= 36;
        }
    }
    s.chars().rev().collect()
}

#[cfg(any())]
pub fn normalize_toolchain_kind(kind: &str) -> String {
    let lower = kind.to_ascii_lowercase();
    match lower.as_str() {
        "rust" => "rust".to_string(),
        "node" => "node".to_string(),
        "ts" | "typescript" => "node".to_string(), // typescript uses the node sidecar
        "python" | "py" => "python".to_string(),
        "c" | "cpp" | "c-cpp" | "c_cpp" | "c++" => "c-cpp".to_string(),
        "go" | "golang" => "go".to_string(),
        _ => lower,
    }
}

#[cfg(any())]
fn default_toolchain_image(kind: &str) -> String {
    match kind {
        "rust" => "rust:1.80-slim".to_string(),
        "node" => "node:20-bookworm-slim".to_string(),
        "python" => "python:3.12-slim".to_string(),
        "c-cpp" => "aifo-cpp-toolchain:latest".to_string(),
        "go" => "golang:1.22-bookworm".to_string(),
        _ => "node:20-bookworm-slim".to_string(),
    }
}

/// Compute default image from kind@version (best-effort).
#[cfg(any())]
pub fn default_toolchain_image_for_version(kind: &str, version: &str) -> String {
    match kind {
        "rust" => format!("rust:{}-slim", version),
        "node" | "typescript" => format!("node:{}-bookworm-slim", version),
        "python" => format!("python:{}-slim", version),
        "go" => format!("golang:{}-bookworm", version),
        "c-cpp" => "aifo-cpp-toolchain:latest".to_string(), // no version mapping
        _ => default_toolchain_image(kind),
    }
}

#[cfg(any())]
fn sidecar_container_name(kind: &str, id: &str) -> String {
    format!("aifo-tc-{kind}-{id}")
}

#[cfg(any())]
fn sidecar_network_name(id: &str) -> String {
    format!("aifo-net-{id}")
}

#[cfg(any())]
fn ensure_network_exists(runtime: &Path, name: &str, verbose: bool) -> bool {
    // Fast path: already exists
    let exists = Command::new(runtime)
        .arg("network")
        .arg("inspect")
        .arg(name)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if exists {
        return true;
    }

    // Create the network (best-effort)
    if verbose {
        eprintln!(
            "aifo-coder: docker: {}",
            shell_join(&vec![
                "docker".to_string(),
                "network".to_string(),
                "create".to_string(),
                name.to_string()
            ])
        );
    }
    let mut cmd = Command::new(runtime);
    cmd.arg("network").arg("create").arg(name);
    if !verbose {
        cmd.stdout(Stdio::null()).stderr(Stdio::null());
    }
    let _ = cmd.status();

    // Verify with brief retries to absorb races between concurrent creators
    for _ in 0..20 {
        let ok = Command::new(runtime)
            .arg("network")
            .arg("inspect")
            .arg(name)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if ok {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

#[cfg(any())]
fn remove_network(runtime: &Path, name: &str, verbose: bool) {
    // Only attempt removal if network exists to avoid noisy errors
    let exists = Command::new(runtime)
        .arg("network")
        .arg("inspect")
        .arg(name)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !exists {
        return;
    }

    let mut cmd = Command::new(runtime);
    cmd.arg("network").arg("rm").arg(name);
    if !verbose {
        cmd.stdout(Stdio::null()).stderr(Stdio::null());
    }
    if verbose {
        eprintln!(
            "aifo-coder: docker: {}",
            shell_join(&vec![
                "docker".to_string(),
                "network".to_string(),
                "rm".to_string(),
                name.to_string()
            ])
        );
    }
    let _ = cmd.status();
}

#[cfg(any())]
fn build_sidecar_run_preview(
    name: &str,
    network: Option<&str>,
    uidgid: Option<(u32, u32)>,
    kind: &str,
    image: &str,
    no_cache: bool,
    pwd: &Path,
    apparmor: Option<&str>,
) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "docker".to_string(),
        "run".to_string(),
        "-d".to_string(),
        "--rm".to_string(),
    ];
    args.push("--name".to_string());
    args.push(name.to_string());
    if let Some(net) = network {
        args.push("--network".to_string());
        args.push(net.to_string());
    }
    if let Some((uid, gid)) = uidgid {
        args.push("--user".to_string());
        args.push(format!("{uid}:{gid}"));
    }
    // mounts
    args.push("-v".to_string());
    args.push(format!("{}:/workspace", pwd.display()));

    match kind {
        "rust" => {
            if !no_cache {
                args.push("-v".to_string());
                args.push("aifo-cargo-registry:/usr/local/cargo/registry".to_string());
                args.push("-v".to_string());
                args.push("aifo-cargo-git:/usr/local/cargo/git".to_string());
            }
            args.push("-e".to_string());
            args.push("CARGO_HOME=/usr/local/cargo".to_string());
        }
        "node" => {
            if !no_cache {
                args.push("-v".to_string());
                args.push("aifo-npm-cache:/home/coder/.npm".to_string());
            }
        }
        "python" => {
            if !no_cache {
                args.push("-v".to_string());
                args.push("aifo-pip-cache:/home/coder/.cache/pip".to_string());
            }
        }
        "c-cpp" => {
            if !no_cache {
                args.push("-v".to_string());
                args.push("aifo-ccache:/home/coder/.cache/ccache".to_string());
            }
            args.push("-e".to_string());
            args.push("CCACHE_DIR=/home/coder/.cache/ccache".to_string());
        }
        "go" => {
            if !no_cache {
                args.push("-v".to_string());
                args.push("aifo-go:/go".to_string());
            }
            args.push("-e".to_string());
            args.push("GOPATH=/go".to_string());
            args.push("-e".to_string());
            args.push("GOMODCACHE=/go/pkg/mod".to_string());
            args.push("-e".to_string());
            args.push("GOCACHE=/go/build-cache".to_string());
        }
        _ => {}
    }

    // base env and workdir
    args.push("-e".to_string());
    args.push("HOME=/home/coder".to_string());
    args.push("-e".to_string());
    args.push("GNUPGHOME=/home/coder/.gnupg".to_string());
    args.push("-w".to_string());
    args.push("/workspace".to_string());

    if let Some(profile) = apparmor {
        if docker_supports_apparmor() {
            args.push("--security-opt".to_string());
            args.push(format!("apparmor={profile}"));
        }
    }

    // Linux connectivity (host proxy via host-gateway) for sidecars as well
    #[cfg(target_os = "linux")]
    {
        if std::env::var("AIFO_TOOLEEXEC_ADD_HOST").ok().as_deref() == Some("1") {
            args.push("--add-host".to_string());
            args.push("host.docker.internal:host-gateway".to_string());
        }
    }

    args.push(image.to_string());
    args.push("sleep".to_string());
    args.push("infinity".to_string());
    args
}

#[cfg(any())]
fn build_sidecar_exec_preview(
    name: &str,
    uidgid: Option<(u32, u32)>,
    pwd: &Path,
    kind: &str,
    user_args: &[String],
) -> Vec<String> {
    let mut args: Vec<String> = vec!["docker".to_string(), "exec".to_string()];
    if let Some((uid, gid)) = uidgid {
        args.push("-u".to_string());
        args.push(format!("{uid}:{gid}"));
    }
    args.push("-w".to_string());
    args.push("/workspace".to_string());
    // base env
    args.push("-e".to_string());
    args.push("HOME=/home/coder".to_string());
    args.push("-e".to_string());
    args.push("GNUPGHOME=/home/coder/.gnupg".to_string());

    match kind {
        "rust" => {
            args.push("-e".to_string());
            args.push("CARGO_HOME=/usr/local/cargo".to_string());
        }
        "python" => {
            let venv_bin = pwd.join(".venv").join("bin");
            if venv_bin.exists() {
                args.push("-e".to_string());
                args.push("VIRTUAL_ENV=/workspace/.venv".to_string());
                args.push("-e".to_string());
                args.push("PATH=/workspace/.venv/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin".to_string());
            }
        }
        "c-cpp" => {
            args.push("-e".to_string());
            args.push("CCACHE_DIR=/home/coder/.cache/ccache".to_string());
        }
        "go" => {
            args.push("-e".to_string());
            args.push("GOPATH=/go".to_string());
            args.push("-e".to_string());
            args.push("GOMODCACHE=/go/pkg/mod".to_string());
            args.push("-e".to_string());
            args.push("GOCACHE=/go/build-cache".to_string());
        }
        _ => {}
    }

    args.push(name.to_string());
    // user command
    for a in user_args {
        args.push(a.clone());
    }
    // include pwd to silence unused warning; it's already used for run mount
    let _ = pwd;
    args
}

/// Rollout Phase 1: start a toolchain sidecar and run the provided command inside it.
/// Returns the exit code of the executed command.
#[cfg(any())]
pub fn toolchain_run(
    kind_in: &str,
    args: &[String],
    image_override: Option<&str>,
    no_cache: bool,
    verbose: bool,
    dry_run: bool,
) -> io::Result<i32> {
    let runtime = container_runtime_path()?;
    let pwd = {
        let p = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        fs::canonicalize(&p).unwrap_or(p)
    };

    #[cfg(unix)]
    let uid: u32 = u32::from(getuid());
    #[cfg(unix)]
    let gid: u32 = u32::from(getgid());

    #[cfg(not(unix))]
    let (uid, gid) = (0u32, 0u32);

    let sidecar_kind = normalize_toolchain_kind(kind_in);
    let image = match image_override {
        Some(s) if !s.trim().is_empty() => s.to_string(),
        _ => default_toolchain_image(sidecar_kind.as_str()),
    };
    let session_id = env::var("AIFO_CODER_FORK_SESSION")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| create_session_id());
    let net_name = sidecar_network_name(&session_id);
    let name = sidecar_container_name(sidecar_kind.as_str(), &session_id);

    // Ensure network exists before starting sidecar
    if !dry_run {
        if !ensure_network_exists(&runtime, &net_name, verbose) {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("failed to create or verify network {}", net_name),
            ));
        }
    }

    let apparmor_profile = desired_apparmor_profile();

    // Build and optionally run sidecar
    let run_preview_args = build_sidecar_run_preview(
        &name,
        Some(&net_name),
        if cfg!(unix) { Some((uid, gid)) } else { None },
        sidecar_kind.as_str(),
        &image,
        no_cache,
        &pwd,
        apparmor_profile.as_deref(),
    );
    let run_preview = shell_join(&run_preview_args);

    if verbose || dry_run {
        eprintln!("aifo-coder: docker: {}", run_preview);
    }

    if !dry_run {
        // If a sidecar with this name already exists, reuse it (another pane may have started it)
        let exists = Command::new(&runtime)
            .arg("inspect")
            .arg(&name)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !exists {
            let mut run_cmd = Command::new(&runtime);
            for a in &run_preview_args[1..] {
                run_cmd.arg(a);
            }
            if !verbose {
                run_cmd.stdout(Stdio::null()).stderr(Stdio::null());
            }
            let status = run_cmd
                .status()
                .map_err(|e| io::Error::new(e.kind(), format!("failed to start sidecar: {e}")))?;
            if !status.success() {
                // Race-safe fallback: consider success if the container exists now (started by a peer)
                let mut exists_after = false;
                for _ in 0..5 {
                    exists_after = Command::new(&runtime)
                        .arg("inspect")
                        .arg(&name)
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .status()
                        .map(|s| s.success())
                        .unwrap_or(false);
                    if exists_after {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                if !exists_after {
                    return Err(io::Error::new(
                        io::ErrorKind::Other,
                        format!(
                            "sidecar container failed to start (exit: {:?})",
                            status.code()
                        ),
                    ));
                }
            }
        }
    }

    // docker exec
    let exec_preview_args = build_sidecar_exec_preview(
        &name,
        if cfg!(unix) { Some((uid, gid)) } else { None },
        &pwd,
        sidecar_kind.as_str(),
        args,
    );
    let exec_preview = shell_join(&exec_preview_args);

    if verbose || dry_run {
        eprintln!("aifo-coder: docker: {}", exec_preview);
    }

    let mut exit_code: i32 = 0;

    if !dry_run {
        let mut exec_cmd = Command::new(&runtime);
        for a in &exec_preview_args[1..] {
            exec_cmd.arg(a);
        }
        let status = exec_cmd
            .status()
            .map_err(|e| io::Error::new(e.kind(), format!("failed to exec in sidecar: {e}")))?;
        exit_code = status.code().unwrap_or(1);
    }

    // Cleanup: stop sidecar and remove network (best-effort)
    if !dry_run {
        let mut stop_cmd = Command::new(&runtime);
        stop_cmd.arg("stop").arg(&name);
        if !verbose {
            stop_cmd.stdout(Stdio::null()).stderr(Stdio::null());
        }
        let _ = stop_cmd.status();

        remove_network(&runtime, &net_name, verbose);
    }

    Ok(exit_code)
}

#[cfg(any())]
fn sidecar_allowlist(kind: &str) -> &'static [&'static str] {
    match kind {
        "rust" => &["cargo", "rustc"],
        "node" => &["node", "npm", "npx", "tsc", "ts-node"],
        "python" => &["python", "python3", "pip", "pip3"],
        "c-cpp" => &[
            "gcc",
            "g++",
            "clang",
            "clang++",
            "make",
            "cmake",
            "ninja",
            "pkg-config",
        ],
        "go" => &["go", "gofmt"],
        _ => &[],
    }
}

/// Map a tool name to the sidecar kind.
#[cfg(any())]
pub fn route_tool_to_sidecar(tool: &str) -> &'static str {
    let t = tool.to_ascii_lowercase();
    match t.as_str() {
        // rust
        "cargo" | "rustc" => "rust",
        // node/typescript
        "node" | "npm" | "npx" | "tsc" | "ts-node" => "node",
        // python
        "python" | "python3" | "pip" | "pip3" => "python",
        // c/c++
        "gcc" | "g++" | "clang" | "clang++" | "make" | "cmake" | "ninja" | "pkg-config" => "c-cpp",
        // go
        "go" | "gofmt" => "go",
        _ => "node",
    }
}




#[cfg(any())]
fn parse_form_urlencoded(body: &str) -> Vec<(String, String)> {
    let mut res = Vec::new();
    for pair in body.split('&') {
        if pair.is_empty() {
            continue;
        }
        let mut it = pair.splitn(2, '=');
        let k = it.next().unwrap_or_default();
        let v = it.next().unwrap_or_default();
        res.push((url_decode(k), url_decode(v)));
    }
    res
}



/// Parse ~/.aider.conf.yml and extract notifications-command as argv tokens.
#[cfg(any())]
fn parse_notifications_command_config() -> Result<Vec<String>, String> {
    // Allow tests (and power users) to override config path explicitly
    let path = if let Ok(p) = env::var("AIFO_NOTIFICATIONS_CONFIG") {
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

//// Validate and, if allowed, execute the host 'say' command with provided args.
/// Returns (exit_code, output_bytes) on success, or Err(reason) if rejected.
#[cfg(any())]
fn notifications_handle_request(
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
    match rx.recv_timeout(std::time::Duration::from_secs(timeout_secs)) {
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

/// Write aifo-shim and tool wrappers into the given directory.
#[cfg(any())]
pub fn toolchain_write_shims(dir: &Path) -> io::Result<()> {
    let tools = [
        "cargo",
        "rustc",
        "node",
        "npm",
        "npx",
        "tsc",
        "ts-node",
        "python",
        "pip",
        "pip3",
        "gcc",
        "g++",
        "clang",
        "clang++",
        "make",
        "cmake",
        "ninja",
        "pkg-config",
        "go",
        "gofmt",
        "notifications-cmd",
    ];
    fs::create_dir_all(dir)?;
    let shim_path = dir.join("aifo-shim");
    let shim = r#"#!/bin/sh
set -e
if [ -z "$AIFO_TOOLEEXEC_URL" ] || [ -z "$AIFO_TOOLEEXEC_TOKEN" ]; then
  echo "aifo-shim: proxy not configured. Please launch agent with --toolchain." >&2
  exit 86
fi
tool="$(basename "$0")"
cwd="$(pwd)"
tmp="${TMPDIR:-/tmp}/aifo-shim.$$"
mkdir -p "$tmp"
# Build curl form payload (-d key=value supports urlencoding)
cmd=(curl -sS -D "$tmp/h" -o "$tmp/b" -X POST -H "Authorization: Bearer $AIFO_TOOLEEXEC_TOKEN")
cmd+=(-d "tool=$tool" -d "cwd=$cwd")
# Append args preserving order
for a in "$@"; do
  cmd+=(-d "arg=$a")
done
cmd+=("$AIFO_TOOLEEXEC_URL")
"${cmd[@]}"
ec="$(awk '/^X-Exit-Code:/{print $2}' "$tmp/h" | tr -d '\r' | tail -n1)"
cat "$tmp/b"
rm -rf "$tmp"
# Fallback to 1 if header missing
case "$ec" in '' ) ec=1 ;; esac
exit "$ec"
"#;
    fs::write(&shim_path, shim)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&shim_path, fs::Permissions::from_mode(0o755))?;
    }
    for t in tools {
        let path = dir.join(t);
        fs::write(
            &path,
            format!("#!/bin/sh\nexec \"$(dirname \"$0\")/aifo-shim\" \"$@\"\n"),
        )?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
        }
    }
    Ok(())
}

/// Start sidecar session for requested kinds; returns the session id.
#[cfg(any())]
pub fn toolchain_start_session(
    kinds: &[String],
    overrides: &[(String, String)],
    no_cache: bool,
    verbose: bool,
) -> io::Result<String> {
    let runtime = container_runtime_path()?;
    let pwd = {
        let p = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        fs::canonicalize(&p).unwrap_or(p)
    };

    #[cfg(unix)]
    let uid: u32 = u32::from(getuid());
    #[cfg(unix)]
    let gid: u32 = u32::from(getgid());
    #[cfg(not(unix))]
    let (_uid, _gid) = (0u32, 0u32);

    let session_id = env::var("AIFO_CODER_FORK_SESSION")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| create_session_id());
    let net_name = sidecar_network_name(&session_id);
    if !ensure_network_exists(&runtime, &net_name, verbose) {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "failed to create session network",
        ));
    }

    let apparmor_profile = desired_apparmor_profile();
    for k in kinds {
        let kind = normalize_toolchain_kind(k);
        // resolve image (override kind=image)
        let mut image = default_toolchain_image(kind.as_str());
        for (kk, vv) in overrides {
            if normalize_toolchain_kind(kk) == kind {
                image = vv.clone();
            }
        }
        let name = sidecar_container_name(kind.as_str(), &session_id);
        let args = build_sidecar_run_preview(
            &name,
            Some(&net_name),
            if cfg!(unix) { Some((uid, gid)) } else { None },
            kind.as_str(),
            &image,
            no_cache,
            &pwd,
            apparmor_profile.as_deref(),
        );
        if verbose {
            eprintln!("aifo-coder: docker: {}", shell_join(&args));
        }
        // If a sidecar with this name already exists, reuse it (another pane may have started it)
        let exists = Command::new(&runtime)
            .arg("inspect")
            .arg(&name)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !exists {
            let mut run_cmd = Command::new(&runtime);
            for a in &args[1..] {
                run_cmd.arg(a);
            }
            if !verbose {
                run_cmd.stdout(Stdio::null()).stderr(Stdio::null());
            }
            let st = run_cmd
                .status()
                .map_err(|e| io::Error::new(e.kind(), format!("failed to start sidecar: {e}")))?;
            if !st.success() {
                // Race-safe fallback: if the container exists now, proceed; otherwise fail
                let mut exists_after = false;
                for _ in 0..5 {
                    exists_after = Command::new(&runtime)
                        .arg("inspect")
                        .arg(&name)
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .status()
                        .map(|s| s.success())
                        .unwrap_or(false);
                    if exists_after {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                if !exists_after {
                    return Err(io::Error::new(
                        io::ErrorKind::Other,
                        "failed to start one or more sidecars",
                    ));
                }
            }
        }
    }
    Ok(session_id)
}

#[cfg(any())]
fn random_token() -> String {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_nanos();
    let pid = std::process::id() as u128;
    let v = now ^ pid;
    let alphabet = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut n = v;
    let mut s = String::new();
    if n == 0 {
        s.push('0');
    } else {
        while n > 0 {
            s.push(alphabet[(n % 36) as usize] as char);
            n /= 36;
        }
    }
    s.chars().rev().collect()
}

/// Start a minimal HTTP proxy to execute tools inside sidecars.
/// Returns (url, token, running_flag, thread_handle).
#[cfg(any())]
pub fn toolexec_start_proxy(
    session_id: &str,
    verbose: bool,
) -> io::Result<(
    String,
    String,
    std::sync::Arc<std::sync::atomic::AtomicBool>,
    std::thread::JoinHandle<()>,
)> {
    let runtime = container_runtime_path()?;

    #[cfg(unix)]
    let uid: u32 = u32::from(getuid());
    #[cfg(unix)]
    let gid: u32 = u32::from(getgid());
    #[cfg(not(unix))]
    let (uid, gid) = (0u32, 0u32);

    // Prepare shared proxy state (token, timeout, running flag, session id)
    let token = random_token();
    let token_for_thread = token.clone();
    // Per-request timeout (seconds); default 60
    let timeout_secs: u64 = env::var("AIFO_TOOLEEXEC_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|&v| v > 0)
        .unwrap_or(60);
    let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let session = session_id.to_string();

    // Optional unix socket transport on Linux, gated by AIFO_TOOLEEXEC_USE_UNIX=1
    let use_unix = cfg!(target_os = "linux")
        && env::var("AIFO_TOOLEEXEC_USE_UNIX").ok().as_deref() == Some("1");
    if use_unix {
        #[cfg(target_os = "linux")]
        {
            // Create host socket directory and bind UnixListener
            let base = "/tmp/aifo";
            let _ = fs::create_dir_all(base);
            let host_dir = format!("{}/aifo-{}", base, session);
            let _ = fs::create_dir_all(&host_dir);
            let sock_path = format!("{}/toolexec.sock", host_dir);
            let _ = fs::remove_file(&sock_path);
            let listener = UnixListener::bind(&sock_path)
                .map_err(|e| io::Error::new(e.kind(), format!("proxy unix bind failed: {e}")))?;
            let _ = listener.set_nonblocking(true);
            // Expose directory for agent mount
            env::set_var("AIFO_TOOLEEXEC_UNIX_DIR", &host_dir);
            let running_cl2 = running.clone();
            let token_for_thread2 = token_for_thread.clone();
            let handle = std::thread::spawn(move || {
                loop {
                    if !running_cl2.load(std::sync::atomic::Ordering::SeqCst) {
                        break;
                    }
                    let mut stream = match listener.accept() {
                        Ok((s, _addr)) => s,
                        Err(e) => {
                            if e.kind() == io::ErrorKind::WouldBlock {
                                std::thread::sleep(Duration::from_millis(50));
                                continue;
                            } else {
                                continue;
                            }
                        }
                    };
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(timeout_secs)));
                    let _ = stream.set_write_timeout(Some(Duration::from_secs(timeout_secs)));
                    // Read request (simple HTTP)
                    let mut buf = Vec::new();
                    let mut hdr = Vec::new();
                    let mut tmp = [0u8; 1024];
                    // Read until CRLF CRLF
                    let mut header_end = None;
                    while header_end.is_none() {
                        match stream.read(&mut tmp) {
                            Ok(0) => break,
                            Ok(n) => {
                                buf.extend_from_slice(&tmp[..n]);
                                if let Some(end) = find_header_end(&buf) {
                                    header_end = Some(end);
                                }
                                if buf.len() > 64 * 1024 {
                                    break;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                    let Some(hend) = header_end else {
                        let header = "HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                        let _ = stream.write_all(header.as_bytes());
                        let _ = stream.flush();
                        let _ = stream.shutdown(Shutdown::Both);
                        continue;
                    };
                    hdr.extend_from_slice(&buf[..hend]);
                    let header_str = String::from_utf8_lossy(&hdr);
                    let mut auth_ok = false;
                    let mut content_len: usize = 0;
                    let mut proto_ok = false;
                    for line in header_str.lines() {
                        let l = line.trim();
                        let lower = l.to_ascii_lowercase();
                        if lower.starts_with("authorization:") {
                            if let Some(v) = l.splitn(2, ':').nth(1) {
                                let value = v.trim();
                                // Accept either a bare token or a case-insensitive "Bearer <token>" scheme
                                if value == token_for_thread2 {
                                    auth_ok = true;
                                } else {
                                    let mut it = value.split_whitespace();
                                    let scheme = it.next().unwrap_or("");
                                    let cred = it.next().unwrap_or("");
                                    if scheme.eq_ignore_ascii_case("bearer")
                                        && cred == token_for_thread2
                                    {
                                        auth_ok = true;
                                    }
                                }
                            }
                        } else if lower.starts_with("content-length:") {
                            if let Some(v) = l.splitn(2, ':').nth(1) {
                                content_len = v.trim().parse().unwrap_or(0);
                            }
                        } else if lower.starts_with("x-aifo-proto:") {
                            if let Some(v) = l.splitn(2, ':').nth(1) {
                                proto_ok = v.trim() == "1";
                            }
                        }
                    }
                    // Read body
                    let mut body = buf[hend..].to_vec();
                    while body.len() < content_len {
                        match stream.read(&mut tmp) {
                            Ok(0) => break,
                            Ok(n) => body.extend_from_slice(&tmp[..n]),
                            Err(_) => break,
                        }
                    }
                    let form = String::from_utf8_lossy(&body).to_string();
                    let mut tool = String::new();
                    let mut cwd = "/workspace".to_string();
                    let mut argv: Vec<String> = Vec::new();
                    for (k, v) in parse_form_urlencoded(&form) {
                        match k.as_str() {
                            "tool" => tool = v,
                            "cwd" => cwd = v,
                            "arg" => argv.push(v),
                            _ => {}
                        }
                    }
                    if tool.is_empty() {
                        // If auth is missing/invalid, prefer 401; else if bad protocol, prefer 426; else 400 for malformed body
                        if !auth_ok {
                            let header = "HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                            let _ = stream.write_all(header.as_bytes());
                            let _ = stream.flush();
                            let _ = stream.shutdown(Shutdown::Both);
                            continue;
                        } else if !proto_ok {
                            let msg = b"Unsupported shim protocol; expected 1\n";
                            let header = format!(
                                "HTTP/1.1 426 Upgrade Required\r\nContent-Type: text/plain; charset=utf-8\r\nX-Exit-Code: 86\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                msg.len()
                            );
                            let _ = stream.write_all(header.as_bytes());
                            let _ = stream.write_all(msg);
                            let _ = stream.flush();
                            let _ = stream.shutdown(Shutdown::Both);
                            continue;
                        } else {
                            let header = "HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                            let _ = stream.write_all(header.as_bytes());
                            let _ = stream.flush();
                            let _ = stream.shutdown(Shutdown::Both);
                            continue;
                        }
                    }
                    if tool == "notifications-cmd" {
                        match notifications_handle_request(&argv, verbose, timeout_secs) {
                            Ok((status_code, body_out)) => {
                                let header = format!(
                                    "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nX-Exit-Code: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                    status_code,
                                    body_out.len()
                                );
                                let _ = stream.write_all(header.as_bytes());
                                let _ = stream.write_all(&body_out);
                                let _ = stream.flush();
                                let _ = stream.shutdown(Shutdown::Both);
                                continue;
                            }
                            Err(reason) => {
                                let mut body = reason.into_bytes();
                                body.push(b'\n');
                                let header = format!(
                                    "HTTP/1.1 403 Forbidden\r\nContent-Type: text/plain; charset=utf-8\r\nX-Exit-Code: 86\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                    body.len()
                                );
                                let _ = stream.write_all(header.as_bytes());
                                let _ = stream.write_all(&body);
                                let _ = stream.flush();
                                let _ = stream.shutdown(Shutdown::Both);
                                continue;
                            }
                        }
                    }
                    let kind = route_tool_to_sidecar(&tool);
                    let allow = sidecar_allowlist(kind);
                    if !allow.iter().any(|&t| t == tool.as_str()) {
                        let header = "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                        let _ = stream.write_all(header.as_bytes());
                        let _ = stream.flush();
                        let _ = stream.shutdown(Shutdown::Both);
                        continue;
                    }
                    // Now enforce Authorization for allowed tools
                    if !auth_ok {
                        let header = "HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                        let _ = stream.write_all(header.as_bytes());
                        let _ = stream.flush();
                        let _ = stream.shutdown(Shutdown::Both);
                        continue;
                    }
                    if !proto_ok {
                        let msg = b"Unsupported shim protocol; expected 1\n";
                        let header = format!(
                            "HTTP/1.1 426 Upgrade Required\r\nContent-Type: text/plain; charset=utf-8\r\nX-Exit-Code: 86\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            msg.len()
                        );
                        let _ = stream.write_all(header.as_bytes());
                        let _ = stream.write_all(msg);
                        let _ = stream.flush();
                        let _ = stream.shutdown(Shutdown::Both);
                        continue;
                    }
                    let name = sidecar_container_name(kind, &session);
                    let pwd = PathBuf::from(cwd);
                    if verbose {
                        eprintln!(
                            "aifo-coder: proxy exec: tool={} args={:?} cwd={}",
                            tool,
                            argv,
                            pwd.display()
                        );
                    }
                    let mut full_args: Vec<String>;
                    if tool == "tsc" {
                        let nm_tsc = pwd.join("node_modules").join(".bin").join("tsc");
                        if nm_tsc.exists() {
                            full_args = vec!["./node_modules/.bin/tsc".to_string()];
                            full_args.extend(argv.clone());
                        } else {
                            full_args = vec!["npx".to_string(), "tsc".to_string()];
                            full_args.extend(argv.clone());
                        }
                    } else {
                        full_args = vec![tool.clone()];
                        full_args.extend(argv.clone());
                    }
                    let exec_preview_args = build_sidecar_exec_preview(
                        &name,
                        if cfg!(unix) { Some((uid, gid)) } else { None },
                        &pwd,
                        kind,
                        &full_args,
                    );
                    if verbose {
                        eprintln!(
                            "aifo-coder: proxy docker: {}",
                            shell_join(&exec_preview_args)
                        );
                    }
                    let started = std::time::Instant::now();
                    let (status_code, body_out) = {
                        let (tx, rx) = std::sync::mpsc::channel();
                        let runtime_cl = runtime.clone();
                        let args_clone: Vec<String> = exec_preview_args[1..].to_vec();
                        std::thread::spawn(move || {
                            let mut cmd = Command::new(&runtime_cl);
                            for a in &args_clone {
                                cmd.arg(a);
                            }
                            let out = cmd.output();
                            let _ = tx.send(out);
                        });
                        match rx.recv_timeout(Duration::from_secs(timeout_secs)) {
                            Ok(Ok(o)) => {
                                let code = o.status.code().unwrap_or(1);
                                let mut b = o.stdout;
                                if !o.stderr.is_empty() {
                                    b.extend_from_slice(&o.stderr);
                                }
                                (code, b)
                            }
                            Ok(Err(e)) => {
                                let mut b = format!("aifo-coder proxy error: {}", e).into_bytes();
                                b.push(b'\n');
                                (1, b)
                            }
                            Err(_timeout) => {
                                let msg = b"aifo-coder proxy timeout\n";
                                let header = format!(
                                    "HTTP/1.1 504 Gateway Timeout\r\nContent-Type: text/plain; charset=utf-8\r\nX-Exit-Code: 124\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                    msg.len()
                                );
                                let _ = stream.write_all(header.as_bytes());
                                let _ = stream.write_all(msg);
                                let _ = stream.flush();
                                let _ = stream.shutdown(Shutdown::Both);
                                continue;
                            }
                        }
                    };
                    let dur_ms = started.elapsed().as_millis();
                    if verbose {
                        eprintln!(
                            "aifo-coder: proxy result tool={} kind={} code={} dur_ms={}",
                            tool, kind, status_code, dur_ms
                        );
                    }
                    let header = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nX-Exit-Code: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        status_code,
                        body_out.len()
                    );
                    let _ = stream.write_all(header.as_bytes());
                    let _ = stream.write_all(&body_out);
                    let _ = stream.flush();
                    let _ = stream.shutdown(Shutdown::Both);
                }
            });
            let url = "unix:///run/aifo/toolexec.sock".to_string();
            return Ok((url, token, running, handle));
        }
    }
    // Bind address by OS: 0.0.0.0 on Linux (containers connect), 127.0.0.1 on macOS/Windows
    let bind_host: &str = if cfg!(target_os = "linux") {
        "0.0.0.0"
    } else {
        "127.0.0.1"
    };
    let listener = TcpListener::bind((bind_host, 0))
        .map_err(|e| io::Error::new(e.kind(), format!("proxy bind failed: {e}")))?;
    let addr = listener
        .local_addr()
        .map_err(|e| io::Error::new(e.kind(), format!("proxy addr failed: {e}")))?;
    let port = addr.port();
    let _ = listener.set_nonblocking(true);
    let running_cl = running.clone();

    let handle = std::thread::spawn(move || {
        if verbose {
            eprintln!(
                "aifo-coder: toolexec proxy listening on {}:{port}",
                bind_host
            );
        }
        loop {
            if !running_cl.load(std::sync::atomic::Ordering::SeqCst) {
                break;
            }
            let mut stream = match listener.accept() {
                Ok((s, _addr)) => s,
                Err(e) => {
                    if e.kind() == io::ErrorKind::WouldBlock {
                        std::thread::sleep(Duration::from_millis(50));
                        continue;
                    } else {
                        continue;
                    }
                }
            };
            let _ = stream.set_read_timeout(Some(Duration::from_secs(timeout_secs)));
            let _ = stream.set_write_timeout(Some(Duration::from_secs(timeout_secs)));
            // Read request (simple HTTP)
            let mut buf = Vec::new();
            let mut hdr = Vec::new();
            let mut tmp = [0u8; 1024];
            // Read until CRLF CRLF
            let mut header_end = None;
            while header_end.is_none() {
                match stream.read(&mut tmp) {
                    Ok(0) => break,
                    Ok(n) => {
                        buf.extend_from_slice(&tmp[..n]);
                        if let Some(end) = find_header_end(&buf) {
                            header_end = Some(end);
                        }
                        // avoid overly large header
                        if buf.len() > 64 * 1024 {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            let Some(hend) = header_end else {
                let header =
                    "HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                let _ = stream.write_all(header.as_bytes());
                let _ = stream.flush();
                let _ = stream.shutdown(Shutdown::Both);
                continue;
            };
            hdr.extend_from_slice(&buf[..hend]);
            let header_str = String::from_utf8_lossy(&hdr);
            let mut auth_ok = false;
            let mut content_len: usize = 0;
            let mut proto_ok = false;
            for line in header_str.lines() {
                let l = line.trim();
                let lower = l.to_ascii_lowercase();
                if lower.starts_with("authorization:") {
                    if let Some(v) = l.splitn(2, ':').nth(1) {
                        let value = v.trim();
                        // Accept either a bare token or a case-insensitive "Bearer <token>" scheme
                        if value == token_for_thread {
                            auth_ok = true;
                        } else {
                            let mut it = value.split_whitespace();
                            let scheme = it.next().unwrap_or("");
                            let cred = it.next().unwrap_or("");
                            if scheme.eq_ignore_ascii_case("bearer") && cred == token_for_thread {
                                auth_ok = true;
                            }
                        }
                    }
                } else if lower.starts_with("content-length:") {
                    if let Some(v) = l.splitn(2, ':').nth(1) {
                        content_len = v.trim().parse().unwrap_or(0);
                    }
                } else if lower.starts_with("x-aifo-proto:") {
                    if let Some(v) = l.splitn(2, ':').nth(1) {
                        proto_ok = v.trim() == "1";
                    }
                }
            }
            // Read body
            let mut body = buf[hend..].to_vec();
            while body.len() < content_len {
                match stream.read(&mut tmp) {
                    Ok(0) => break,
                    Ok(n) => body.extend_from_slice(&tmp[..n]),
                    Err(_) => break,
                }
            }
            let form = String::from_utf8_lossy(&body).to_string();
            let mut tool = String::new();
            let mut cwd = "/workspace".to_string();
            let mut argv: Vec<String> = Vec::new();
            for (k, v) in parse_form_urlencoded(&form) {
                match k.as_str() {
                    "tool" => tool = v,
                    "cwd" => cwd = v,
                    "arg" => argv.push(v),
                    _ => {}
                }
            }
            if tool.is_empty() {
                // If auth is missing/invalid, prefer 401; else if bad protocol, prefer 426; else 400 for malformed body
                if !auth_ok {
                    let header = "HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                    let _ = stream.write_all(header.as_bytes());
                    let _ = stream.flush();
                    let _ = stream.shutdown(Shutdown::Both);
                    continue;
                } else if !proto_ok {
                    let msg = b"Unsupported shim protocol; expected 1\n";
                    let header = format!(
                        "HTTP/1.1 426 Upgrade Required\r\nContent-Type: text/plain; charset=utf-8\r\nX-Exit-Code: 86\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        msg.len()
                    );
                    let _ = stream.write_all(header.as_bytes());
                    let _ = stream.write_all(msg);
                    let _ = stream.flush();
                    let _ = stream.shutdown(Shutdown::Both);
                    continue;
                } else {
                    let header = "HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                    let _ = stream.write_all(header.as_bytes());
                    let _ = stream.flush();
                    let _ = stream.shutdown(Shutdown::Both);
                    continue;
                }
            }
            if tool == "notifications-cmd" {
                match notifications_handle_request(&argv, verbose, timeout_secs) {
                    Ok((status_code, body_out)) => {
                        let header = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nX-Exit-Code: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            status_code,
                            body_out.len()
                        );
                        let _ = stream.write_all(header.as_bytes());
                        let _ = stream.write_all(&body_out);
                        let _ = stream.flush();
                        let _ = stream.shutdown(Shutdown::Both);
                        continue;
                    }
                    Err(reason) => {
                        let mut body = reason.into_bytes();
                        body.push(b'\n');
                        let header = format!(
                            "HTTP/1.1 403 Forbidden\r\nContent-Type: text/plain; charset=utf-8\r\nX-Exit-Code: 86\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        );
                        let _ = stream.write_all(header.as_bytes());
                        let _ = stream.write_all(&body);
                        let _ = stream.flush();
                        let _ = stream.shutdown(Shutdown::Both);
                        continue;
                    }
                }
            }
            let kind = route_tool_to_sidecar(&tool);
            let allow = sidecar_allowlist(kind);
            if !allow.iter().any(|&t| t == tool.as_str()) {
                let header =
                    "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                let _ = stream.write_all(header.as_bytes());
                let _ = stream.flush();
                let _ = stream.shutdown(Shutdown::Both);
                continue;
            }
            // Now enforce Authorization for allowed tools, then protocol
            if !auth_ok {
                let header =
                    "HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                let _ = stream.write_all(header.as_bytes());
                let _ = stream.flush();
                let _ = stream.shutdown(Shutdown::Both);
                continue;
            }
            if !proto_ok {
                let msg = b"Unsupported shim protocol; expected 1\n";
                let header = format!(
                    "HTTP/1.1 426 Upgrade Required\r\nContent-Type: text/plain; charset=utf-8\r\nX-Exit-Code: 86\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    msg.len()
                );
                let _ = stream.write_all(header.as_bytes());
                let _ = stream.write_all(msg);
                let _ = stream.flush();
                let _ = stream.shutdown(Shutdown::Both);
                continue;
            }
            let name = sidecar_container_name(kind, &session);
            let pwd = PathBuf::from(cwd);
            if verbose {
                eprintln!(
                    "aifo-coder: proxy exec: tool={} args={:?} cwd={}",
                    tool,
                    argv,
                    pwd.display()
                );
            }
            let mut full_args: Vec<String>;
            if tool == "tsc" {
                let nm_tsc = pwd.join("node_modules").join(".bin").join("tsc");
                if nm_tsc.exists() {
                    full_args = vec!["./node_modules/.bin/tsc".to_string()];
                    full_args.extend(argv.clone());
                } else {
                    full_args = vec!["npx".to_string(), "tsc".to_string()];
                    full_args.extend(argv.clone());
                }
            } else {
                full_args = vec![tool.clone()];
                full_args.extend(argv.clone());
            }

            let exec_preview_args = build_sidecar_exec_preview(
                &name,
                if cfg!(unix) { Some((uid, gid)) } else { None },
                &pwd,
                kind,
                &full_args,
            );
            if verbose {
                eprintln!(
                    "aifo-coder: proxy docker: {}",
                    shell_join(&exec_preview_args)
                );
            }
            let started = std::time::Instant::now();
            let (status_code, body_out) = {
                let (tx, rx) = std::sync::mpsc::channel();
                let runtime_cl = runtime.clone();
                let args_clone: Vec<String> = exec_preview_args[1..].to_vec();
                std::thread::spawn(move || {
                    let mut cmd = Command::new(&runtime_cl);
                    for a in &args_clone {
                        cmd.arg(a);
                    }
                    let out = cmd.output();
                    let _ = tx.send(out);
                });
                match rx.recv_timeout(Duration::from_secs(timeout_secs)) {
                    Ok(Ok(o)) => {
                        let code = o.status.code().unwrap_or(1);
                        let mut b = o.stdout;
                        if !o.stderr.is_empty() {
                            b.extend_from_slice(&o.stderr);
                        }
                        (code, b)
                    }
                    Ok(Err(e)) => {
                        let mut b = format!("aifo-coder proxy error: {}", e).into_bytes();
                        b.push(b'\n');
                        (1, b)
                    }
                    Err(_timeout) => {
                        let msg = b"aifo-coder proxy timeout\n";
                        let header = format!(
                            "HTTP/1.1 504 Gateway Timeout\r\nContent-Type: text/plain; charset=utf-8\r\nX-Exit-Code: 124\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            msg.len()
                        );
                        let _ = stream.write_all(header.as_bytes());
                        let _ = stream.write_all(msg);
                        let _ = stream.flush();
                        let _ = stream.shutdown(Shutdown::Both);
                        continue;
                    }
                }
            };
            let dur_ms = started.elapsed().as_millis();
            if verbose {
                eprintln!(
                    "aifo-coder: proxy result tool={} kind={} code={} dur_ms={}",
                    tool, kind, status_code, dur_ms
                );
            }
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nX-Exit-Code: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                status_code,
                body_out.len()
            );
            let _ = stream.write_all(header.as_bytes());
            let _ = stream.write_all(&body_out);
            let _ = stream.flush();
            let _ = stream.shutdown(Shutdown::Both);
        }
        if verbose {
            eprintln!("aifo-coder: toolexec proxy stopped");
        }
    });
    // On macOS/Windows, host.docker.internal resolves; on Linux we add host-gateway and still use host.docker.internal
    let url = format!("http://host.docker.internal:{}/exec", port);
    Ok((url, token, running, handle))
}

/// Cleanup sidecars and network for a session id (best-effort).
#[cfg(any())]
pub fn toolchain_cleanup_session(session_id: &str, verbose: bool) {
    let runtime = match container_runtime_path() {
        Ok(p) => p,
        Err(_) => return,
    };
    let kinds = ["rust", "node", "python", "c-cpp", "go"];
    for k in kinds {
        let name = sidecar_container_name(k, session_id);
        // Only attempt stop when container exists to avoid noisy daemon errors
        let exists = Command::new(&runtime)
            .arg("inspect")
            .arg(&name)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if exists {
            if verbose {
                eprintln!("aifo-coder: docker: docker stop {}", name);
            }
            let _ = Command::new(&runtime)
                .arg("stop")
                .arg("--time")
                .arg("1")
                .arg(&name)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }
    let net = sidecar_network_name(session_id);
    remove_network(&runtime, &net, verbose);

    // Best-effort cleanup of unix socket directory (Linux, unix transport)
    if let Ok(dir) = env::var("AIFO_TOOLEEXEC_UNIX_DIR") {
        if !dir.trim().is_empty() {
            let p = PathBuf::from(dir);
            let _ = fs::remove_file(p.join("toolexec.sock"));
            let _ = fs::remove_dir_all(&p);
        }
    }
}

/// Purge all named Docker volumes used as toolchain caches (rust, node, python, c/cpp, go).
#[cfg(any())]
pub fn toolchain_purge_caches(verbose: bool) -> io::Result<()> {
    let runtime = container_runtime_path()?;
    let volumes = [
        "aifo-cargo-registry",
        "aifo-cargo-git",
        "aifo-npm-cache",
        "aifo-pip-cache",
        "aifo-ccache",
        "aifo-go",
    ];
    for v in volumes {
        if verbose {
            eprintln!("aifo-coder: docker: docker volume rm -f {}", v);
        }
        let _ = Command::new(&runtime)
            .arg("volume")
            .arg("rm")
            .arg("-f")
            .arg(v)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    Ok(())
}

//// Phase 2: Snapshot and cloning primitives (per spec v4)








// Phase 6: fork maintenance and stale-session notice













#[cfg(test)]
mod tests {
    use super::*;
    use crate as aifo_coder;
    use once_cell::sync::Lazy;
    use std::sync::Mutex;

    // Serialize tests that mutate HOME/AIFO_NOTIFICATIONS_CONFIG to avoid env races
    static NOTIF_ENV_GUARD: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

    #[test]
    fn test_url_decode_mixed() {
        assert_eq!(url_decode("a+b%20c%2F%3F%25"), "a b c/?%");
        assert_eq!(url_decode("%41%42%43"), "ABC");
        assert_eq!(url_decode("no-escapes_here~"), "no-escapes_here~");
    }

    #[test]
    fn test_parse_form_urlencoded_basic_and_repeated() {
        let pairs = parse_form_urlencoded("arg=a&arg=b&tool=cargo&cwd=.");
        let expected = vec![
            ("arg".to_string(), "a".to_string()),
            ("arg".to_string(), "b".to_string()),
            ("tool".to_string(), "cargo".to_string()),
            ("cwd".to_string(), ".".to_string()),
        ];
        assert_eq!(pairs, expected);
    }

    #[test]
    fn test_find_crlfcrlf_cases() {
        assert_eq!(find_crlfcrlf(b"\r\n\r\n"), Some(0));
        assert_eq!(find_crlfcrlf(b"abc\r\n\r\ndef"), Some(3));
        assert_eq!(find_crlfcrlf(b"abcdef"), None);
        assert_eq!(find_crlfcrlf(b"\r\n\r"), None);
    }

    #[test]
    fn test_strip_outer_quotes_variants() {
        assert_eq!(strip_outer_quotes("'abc'"), "abc");
        assert_eq!(strip_outer_quotes("\"abc\""), "abc");
        assert_eq!(strip_outer_quotes("'a b'"), "a b");
        assert_eq!(strip_outer_quotes("noquote"), "noquote");
        // Only strips if both ends match the same quote type
        assert_eq!(strip_outer_quotes("'mismatch\""), "'mismatch\"");
    }

    #[test]
    fn test_shell_like_split_args_quotes_and_spaces() {
        let args = shell_like_split_args("'a b' c \"d e\"");
        assert_eq!(
            args,
            vec!["a b".to_string(), "c".to_string(), "d e".to_string()]
        );

        let args2 = shell_like_split_args("  a   'b c'   d  ");
        assert_eq!(
            args2,
            vec!["a".to_string(), "b c".to_string(), "d".to_string()]
        );
    }

    #[test]
    fn test_parse_notifications_inline_array() {
        let _g = NOTIF_ENV_GUARD.lock().unwrap();
        // Isolate HOME to a temp dir with an inline-array notifications-command
        let td = tempfile::tempdir().expect("tmpdir");
        let home = td.path().to_path_buf();
        let old_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", &home);

        let cfg = r#"notifications-command: ["say", "--title", "AIFO"]\n"#;
        let cfg_path = home.join(".aider.conf.yml");
        std::fs::write(&cfg_path, cfg).expect("write config");
        // Force parser to use this exact file path to avoid HOME/env races
        let old_cfg = std::env::var("AIFO_NOTIFICATIONS_CONFIG").ok();
        std::env::set_var("AIFO_NOTIFICATIONS_CONFIG", &cfg_path);
        let argv = parse_notifications_command_config().expect("parse notifications array");
        assert_eq!(
            argv,
            vec!["say".to_string(), "--title".to_string(), "AIFO".to_string()]
        );
        // Restore AIFO_NOTIFICATIONS_CONFIG
        if let Some(v) = old_cfg {
            std::env::set_var("AIFO_NOTIFICATIONS_CONFIG", v);
        } else {
            std::env::remove_var("AIFO_NOTIFICATIONS_CONFIG");
        }

        // Restore HOME
        if let Some(v) = old_home {
            std::env::set_var("HOME", v);
        } else {
            std::env::remove_var("HOME");
        }
    }

    #[test]
    fn test_parse_notifications_nested_array_lines() {
        let _g = NOTIF_ENV_GUARD.lock().unwrap();
        // Isolate HOME to a temp dir with a nested array notifications-command
        let td = tempfile::tempdir().expect("tmpdir");
        let home = td.path().to_path_buf();
        let old_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", &home);

        let cfg = r#"notifications-command:
  - "say"
  - --title
  - AIFO
"#;
        let cfg_path = home.join(".aider.conf.yml");
        std::fs::write(&cfg_path, cfg).expect("write config");
        let old_cfg = std::env::var("AIFO_NOTIFICATIONS_CONFIG").ok();
        std::env::set_var("AIFO_NOTIFICATIONS_CONFIG", &cfg_path);
        let argv = parse_notifications_command_config().expect("parse notifications nested array");
        assert_eq!(
            argv,
            vec!["say".to_string(), "--title".to_string(), "AIFO".to_string()]
        );
        // Restore env
        if let Some(v) = old_cfg {
            std::env::set_var("AIFO_NOTIFICATIONS_CONFIG", v);
        } else {
            std::env::remove_var("AIFO_NOTIFICATIONS_CONFIG");
        }
        if let Some(v) = old_home {
            std::env::set_var("HOME", v);
        } else {
            std::env::remove_var("HOME");
        }
    }

    #[test]
    fn test_parse_notifications_block_scalar() {
        let _g = NOTIF_ENV_GUARD.lock().unwrap();
        // Isolate HOME to a temp dir with a block scalar notifications-command
        let td = tempfile::tempdir().expect("tmpdir");
        let home = td.path().to_path_buf();
        let old_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", &home);

        let cfg = r#"notifications-command: |
  say --title "AIFO"
"#;
        let cfg_path = home.join(".aider.conf.yml");
        std::fs::write(&cfg_path, cfg).expect("write config");
        let old_cfg = std::env::var("AIFO_NOTIFICATIONS_CONFIG").ok();
        std::env::set_var("AIFO_NOTIFICATIONS_CONFIG", &cfg_path);
        let argv = parse_notifications_command_config().expect("parse notifications block");
        assert_eq!(
            argv,
            vec!["say".to_string(), "--title".to_string(), "AIFO".to_string()]
        );
        // Restore env
        if let Some(v) = old_cfg {
            std::env::set_var("AIFO_NOTIFICATIONS_CONFIG", v);
        } else {
            std::env::remove_var("AIFO_NOTIFICATIONS_CONFIG");
        }
        if let Some(v) = old_home {
            std::env::set_var("HOME", v);
        } else {
            std::env::remove_var("HOME");
        }
    }

    #[test]
    fn test_parse_notifications_single_line_string() {
        let _g = NOTIF_ENV_GUARD.lock().unwrap();
        // Isolate HOME to a temp dir with a single-line string notifications-command
        let td = tempfile::tempdir().expect("tmpdir");
        let home = td.path().to_path_buf();
        let old_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", &home);

        let cfg = r#"notifications-command: "say --title AIFO"\n"#;
        let cfg_path = home.join(".aider.conf.yml");
        std::fs::write(&cfg_path, cfg).expect("write config");
        // Force parser to use this exact file path to avoid HOME/env races
        let old_cfg = std::env::var("AIFO_NOTIFICATIONS_CONFIG").ok();
        std::env::set_var("AIFO_NOTIFICATIONS_CONFIG", &cfg_path);
        let argv = parse_notifications_command_config().expect("parse notifications string");
        assert_eq!(
            argv,
            vec!["say".to_string(), "--title".to_string(), "AIFO".to_string()]
        );
        // Restore AIFO_NOTIFICATIONS_CONFIG
        if let Some(v) = old_cfg {
            std::env::set_var("AIFO_NOTIFICATIONS_CONFIG", v);
        } else {
            std::env::remove_var("AIFO_NOTIFICATIONS_CONFIG");
        }

        // Restore HOME
        if let Some(v) = old_home {
            std::env::set_var("HOME", v);
        } else {
            std::env::remove_var("HOME");
        }
    }

    #[test]
    fn test_build_sidecar_exec_preview_python_venv_env() {
        // Create a temp workspace with .venv/bin and ensure PATH/VIRTUAL_ENV are injected
        let td = tempfile::tempdir().expect("tmpdir");
        let pwd = td.path();
        std::fs::create_dir_all(pwd.join(".venv").join("bin")).expect("create venv/bin");
        let user_args = vec!["python".to_string(), "--version".to_string()];
        let args = build_sidecar_exec_preview("tc-python", None, pwd, "python", &user_args);

        let has_virtual_env = args.iter().any(|s| s == "VIRTUAL_ENV=/workspace/.venv");
        let has_path_prefix = args
            .iter()
            .any(|s| s.contains("PATH=/workspace/.venv/bin:"));
        assert!(
            has_virtual_env,
            "exec preview missing VIRTUAL_ENV: {:?}",
            args
        );
        assert!(
            has_path_prefix,
            "exec preview missing PATH venv prefix: {:?}",
            args
        );
    }

    #[test]
    fn test_notifications_config_rejects_non_say() {
        let _g = NOTIF_ENV_GUARD.lock().unwrap();
        // Isolate HOME to a temp dir with a non-say notifications-command
        let td = tempfile::tempdir().expect("tmpdir");
        let home = td.path().to_path_buf();
        let old_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", &home);

        let cfg = r#"notifications-command: ["notify", "--title", "AIFO"]\n"#;
        let cfg_path = home.join(".aider.conf.yml");
        std::fs::write(&cfg_path, cfg).expect("write config");
        // Force parser to use this exact file path to avoid platform-specific HOME quirks
        let old_cfg = std::env::var("AIFO_NOTIFICATIONS_CONFIG").ok();
        std::env::set_var("AIFO_NOTIFICATIONS_CONFIG", &cfg_path);
        let res = notifications_handle_request(&["--title".into(), "AIFO".into()], false, 1);
        assert!(res.is_err(), "expected error when executable is not 'say'");
        let msg = res.err().unwrap();
        assert!(
            msg.contains("only 'say' is allowed"),
            "unexpected error: {}",
            msg
        );

        // Restore AIFO_NOTIFICATIONS_CONFIG
        if let Some(v) = old_cfg {
            std::env::set_var("AIFO_NOTIFICATIONS_CONFIG", v);
        } else {
            std::env::remove_var("AIFO_NOTIFICATIONS_CONFIG");
        }

        // Restore HOME
        if let Some(v) = old_home {
            std::env::set_var("HOME", v);
        } else {
            std::env::remove_var("HOME");
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_sidecar_run_preview_add_host_flag_linux() {
        // Ensure add-host is injected for sidecars when env flag is set
        let td = tempfile::tempdir().expect("tmpdir");
        let pwd = td.path();
        let old = std::env::var("AIFO_TOOLEEXEC_ADD_HOST").ok();
        std::env::set_var("AIFO_TOOLEEXEC_ADD_HOST", "1");

        let args = build_sidecar_run_preview(
            "tc-rust-test",
            Some("aifo-net-test"),
            None,
            "rust",
            "rust:1.80-slim",
            true,
            pwd,
            Some("docker-default"),
        );
        let joined = shell_join(&args);
        assert!(
            joined.contains("--add-host host.docker.internal:host-gateway"),
            "sidecar run preview missing --add-host: {}",
            joined
        );

        // Restore env
        if let Some(v) = old {
            std::env::set_var("AIFO_TOOLEEXEC_ADD_HOST", v);
        } else {
            std::env::remove_var("AIFO_TOOLEEXEC_ADD_HOST");
        }
    }

    #[test]
    fn test_sidecar_run_preview_rust_caches_env() {
        // Ensure rust sidecar gets cargo cache mounts and CARGO_HOME
        let td = tempfile::tempdir().expect("tmpdir");
        let pwd = td.path();
        let args = build_sidecar_run_preview(
            "tc-rust-cache",
            Some("aifo-net-x"),
            None,
            "rust",
            "rust:1.80-slim",
            false, // no_cache = false -> caches enabled
            pwd,
            Some("docker-default"),
        );
        let joined = shell_join(&args);
        assert!(
            joined.contains("aifo-cargo-registry:/usr/local/cargo/registry"),
            "missing cargo registry mount: {}",
            joined
        );
        assert!(
            joined.contains("aifo-cargo-git:/usr/local/cargo/git"),
            "missing cargo git mount: {}",
            joined
        );
        assert!(
            joined.contains("CARGO_HOME=/usr/local/cargo"),
            "missing CARGO_HOME env: {}",
            joined
        );
    }

    #[test]
    fn test_sidecar_run_preview_caches_for_node_python_cpp_go() {
        let td = tempfile::tempdir().expect("tmpdir");
        let pwd = td.path();

        // node: npm cache
        let node = build_sidecar_run_preview(
            "tc-node-cache",
            Some("aifo-net-x"),
            None,
            "node",
            "node:20-bookworm-slim",
            false,
            pwd,
            Some("docker-default"),
        );
        let node_joined = shell_join(&node);
        assert!(
            node_joined.contains("aifo-npm-cache:/home/coder/.npm"),
            "missing npm cache mount: {}",
            node_joined
        );

        // python: pip cache
        let py = build_sidecar_run_preview(
            "tc-python-cache",
            Some("aifo-net-x"),
            None,
            "python",
            "python:3.12-slim",
            false,
            pwd,
            Some("docker-default"),
        );
        let py_joined = shell_join(&py);
        assert!(
            py_joined.contains("aifo-pip-cache:/home/coder/.cache/pip"),
            "missing pip cache mount: {}",
            py_joined
        );

        // c-cpp: ccache dir and env
        let cpp = build_sidecar_run_preview(
            "tc-cpp-cache",
            Some("aifo-net-x"),
            None,
            "c-cpp",
            "aifo-cpp-toolchain:latest",
            false,
            pwd,
            Some("docker-default"),
        );
        let cpp_joined = shell_join(&cpp);
        assert!(
            cpp_joined.contains("aifo-ccache:/home/coder/.cache/ccache"),
            "missing ccache volume: {}",
            cpp_joined
        );
        assert!(
            cpp_joined.contains("CCACHE_DIR=/home/coder/.cache/ccache"),
            "missing CCACHE_DIR env: {}",
            cpp_joined
        );

        // go: GOPATH/GOMODCACHE/GOCACHE and volume
        let go = build_sidecar_run_preview(
            "tc-go-cache",
            Some("aifo-net-x"),
            None,
            "go",
            "golang:1.22-bookworm",
            false,
            pwd,
            Some("docker-default"),
        );
        let go_joined = shell_join(&go);
        assert!(
            go_joined.contains("aifo-go:/go"),
            "missing go volume: {}",
            go_joined
        );
        assert!(
            go_joined.contains("GOPATH=/go"),
            "missing GOPATH env: {}",
            go_joined
        );
        assert!(
            go_joined.contains("GOMODCACHE=/go/pkg/mod"),
            "missing GOMODCACHE env: {}",
            go_joined
        );
        assert!(
            go_joined.contains("GOCACHE=/go/build-cache"),
            "missing GOCACHE env: {}",
            go_joined
        );
    }

    #[test]
    fn test_build_sidecar_exec_preview_cpp_ccache_env() {
        // c/cpp exec should include CCACHE_DIR env
        let td = tempfile::tempdir().expect("tmpdir");
        let pwd = td.path();
        let user_args = vec!["cmake".to_string(), "--version".to_string()];
        let args = build_sidecar_exec_preview("tc-cpp", None, pwd, "c-cpp", &user_args);
        let has_ccache = args
            .iter()
            .any(|s| s == "CCACHE_DIR=/home/coder/.cache/ccache");
        assert!(has_ccache, "exec preview missing CCACHE_DIR: {:?}", args);
    }

    #[test]
    fn test_build_sidecar_exec_preview_go_envs() {
        // go exec should include GOPATH/GOMODCACHE/GOCACHE envs
        let td = tempfile::tempdir().expect("tmpdir");
        let pwd = td.path();
        let user_args = vec!["go".to_string(), "version".to_string()];
        let args = build_sidecar_exec_preview("tc-go", None, pwd, "go", &user_args);
        let has_gopath = args.iter().any(|s| s == "GOPATH=/go");
        let has_mod = args.iter().any(|s| s == "GOMODCACHE=/go/pkg/mod");
        let has_cache = args.iter().any(|s| s == "GOCACHE=/go/build-cache");
        assert!(
            has_gopath && has_mod && has_cache,
            "exec preview missing go envs: {:?}",
            args
        );
    }

    #[test]
    fn test_notifications_args_mismatch_error() {
        let _g = NOTIF_ENV_GUARD.lock().unwrap();
        // Prepare config allowing only ["--title", "AIFO"]
        let td = tempfile::tempdir().expect("tmpdir");
        let home = td.path().to_path_buf();
        let old_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", &home);

        let cfg = r#"notifications-command: ["say", "--title", "AIFO"]\n"#;
        let cfg_path = home.join(".aider.conf.yml");
        std::fs::write(&cfg_path, cfg).expect("write config");
        let old_cfg = std::env::var("AIFO_NOTIFICATIONS_CONFIG").ok();
        std::env::set_var("AIFO_NOTIFICATIONS_CONFIG", &cfg_path);

        // Request with mismatching args
        let res = notifications_handle_request(&["--title".into(), "Other".into()], false, 1);
        assert!(res.is_err(), "expected mismatch error, got: {:?}", res);
        let msg = res.err().unwrap();
        assert!(
            msg.contains("arguments mismatch"),
            "unexpected error message: {}",
            msg
        );

        // Restore env
        if let Some(v) = old_cfg {
            std::env::set_var("AIFO_NOTIFICATIONS_CONFIG", v);
        } else {
            std::env::remove_var("AIFO_NOTIFICATIONS_CONFIG");
        }
        if let Some(v) = old_home {
            std::env::set_var("HOME", v);
        } else {
            std::env::remove_var("HOME");
        }
    }

    #[test]
    fn test_candidate_lock_paths_includes_xdg_runtime_dir() {
        // Use a non-repo temp directory to exercise legacy fallback candidates
        let td = tempfile::tempdir().expect("tmpdir");
        let old = std::env::var("XDG_RUNTIME_DIR").ok();
        let old_cwd = std::env::current_dir().expect("cwd");
        std::env::set_var("XDG_RUNTIME_DIR", td.path());
        std::env::set_current_dir(td.path()).expect("chdir");

        let paths = candidate_lock_paths();
        let expected = td.path().join("aifo-coder.lock");
        assert!(
            paths.iter().any(|p| p == &expected),
            "candidate_lock_paths missing expected XDG_RUNTIME_DIR path: {:?}",
            expected
        );

        // Restore env and cwd
        if let Some(v) = old {
            std::env::set_var("XDG_RUNTIME_DIR", v);
        } else {
            std::env::remove_var("XDG_RUNTIME_DIR");
        }
        std::env::set_current_dir(old_cwd).ok();
    }

    #[test]
    fn test_candidate_lock_paths_includes_cwd_lock_outside_repo() {
        // In a non-repo directory, ensure CWD/.aifo-coder.lock appears among legacy candidates
        let td = tempfile::tempdir().expect("tmpdir");
        let old_cwd = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(td.path()).expect("chdir");
        // Unset repo-related envs to avoid confusing repo detection
        let paths = candidate_lock_paths();
        let expected = td.path().join(".aifo-coder.lock");
        // On macOS, /var is often a symlink to /private/var. Canonicalize parent dirs for comparison.
        let expected_dir_canon =
            std::fs::canonicalize(td.path()).unwrap_or_else(|_| td.path().to_path_buf());
        let found = paths.iter().any(|p| {
            p.file_name()
                .map(|n| n == ".aifo-coder.lock")
                .unwrap_or(false)
                && p.parent()
                    .and_then(|d| std::fs::canonicalize(d).ok())
                    .map(|d| d == expected_dir_canon)
                    .unwrap_or(false)
        });
        assert!(
            found,
            "candidate_lock_paths missing expected CWD lock path: {:?} in {:?}",
            expected, paths
        );
        std::env::set_current_dir(old_cwd).ok();
    }

    #[test]
    fn test_parse_form_urlencoded_empty_and_missing_values() {
        let pairs = parse_form_urlencoded("a=1&b=&c");
        assert!(
            pairs.contains(&(String::from("a"), String::from("1"))),
            "missing a=1 in {:?}",
            pairs
        );
        assert!(
            pairs.contains(&(String::from("b"), String::from(""))),
            "missing b= in {:?}",
            pairs
        );
        assert!(
            pairs.contains(&(String::from("c"), String::from(""))),
            "missing c (no '=') in {:?}",
            pairs
        );
    }

    #[test]
    fn test_sidecar_run_preview_no_cache_removes_cache_mounts() {
        let td = tempfile::tempdir().expect("tmpdir");
        let pwd = td.path();

        // rust: no aifo-cargo-* mounts when no_cache=true
        let rust = build_sidecar_run_preview(
            "tc-rust-nocache",
            Some("aifo-net-x"),
            None,
            "rust",
            "rust:1.80-slim",
            true,
            pwd,
            Some("docker-default"),
        );
        let r = shell_join(&rust);
        assert!(
            !r.contains("aifo-cargo-registry:/usr/local/cargo/registry"),
            "unexpected cargo registry mount: {}",
            r
        );
        assert!(
            !r.contains("aifo-cargo-git:/usr/local/cargo/git"),
            "unexpected cargo git mount: {}",
            r
        );

        // node: no npm cache mount
        let node = build_sidecar_run_preview(
            "tc-node-nocache",
            Some("aifo-net-x"),
            None,
            "node",
            "node:20-bookworm-slim",
            true,
            pwd,
            Some("docker-default"),
        );
        let n = shell_join(&node);
        assert!(
            !n.contains("aifo-npm-cache:/home/coder/.npm"),
            "unexpected npm cache mount: {}",
            n
        );

        // python: no pip cache mount
        let py = build_sidecar_run_preview(
            "tc-python-nocache",
            Some("aifo-net-x"),
            None,
            "python",
            "python:3.12-slim",
            true,
            pwd,
            Some("docker-default"),
        );
        let p = shell_join(&py);
        assert!(
            !p.contains("aifo-pip-cache:/home/coder/.cache/pip"),
            "unexpected pip cache mount: {}",
            p
        );

        // c-cpp: no ccache volume
        let cpp = build_sidecar_run_preview(
            "tc-cpp-nocache",
            Some("aifo-net-x"),
            None,
            "c-cpp",
            "aifo-cpp-toolchain:latest",
            true,
            pwd,
            Some("docker-default"),
        );
        let c = shell_join(&cpp);
        assert!(
            !c.contains("aifo-ccache:/home/coder/.cache/ccache"),
            "unexpected ccache volume: {}",
            c
        );

        // go: no /go volume
        let go = build_sidecar_run_preview(
            "tc-go-nocache",
            Some("aifo-net-x"),
            None,
            "go",
            "golang:1.22-bookworm",
            true,
            pwd,
            Some("docker-default"),
        );
        let g = shell_join(&go);
        assert!(!g.contains("aifo-go:/go"), "unexpected go volume: {}", g);
    }

    #[test]
    fn test_should_acquire_lock_env() {
        // Default: acquire
        std::env::remove_var("AIFO_CODER_SKIP_LOCK");
        assert!(should_acquire_lock(), "should acquire lock by default");
        // Skip when set to "1"
        std::env::set_var("AIFO_CODER_SKIP_LOCK", "1");
        assert!(
            !should_acquire_lock(),
            "should not acquire lock when AIFO_CODER_SKIP_LOCK=1"
        );
        std::env::remove_var("AIFO_CODER_SKIP_LOCK");
    }

    #[cfg(not(windows))]
    #[test]
    fn test_hashed_lock_path_diff_for_two_repos() {
        // Create two separate repos and ensure their hashed XDG lock paths differ
        let td = tempfile::tempdir().expect("tmpdir");
        let ws = td.path().to_path_buf();
        let old_xdg = std::env::var("XDG_RUNTIME_DIR").ok();
        std::env::set_var("XDG_RUNTIME_DIR", &ws);

        // repo A
        let repo_a = ws.join("repo-a");
        std::fs::create_dir_all(&repo_a).unwrap();
        let _ = std::process::Command::new("git")
            .arg("init")
            .current_dir(&repo_a)
            .status();
        std::env::set_current_dir(&repo_a).unwrap();
        let paths_a = candidate_lock_paths();
        assert!(
            paths_a.len() >= 2,
            "expected at least two candidates for repo A"
        );
        let hashed_a = paths_a[1].clone();

        // repo B
        let repo_b = ws.join("repo-b");
        std::fs::create_dir_all(&repo_b).unwrap();
        let _ = std::process::Command::new("git")
            .arg("init")
            .current_dir(&repo_b)
            .status();
        std::env::set_current_dir(&repo_b).unwrap();
        let paths_b = candidate_lock_paths();
        assert!(
            paths_b.len() >= 2,
            "expected at least two candidates for repo B"
        );
        let hashed_b = paths_b[1].clone();

        assert_ne!(
            hashed_a,
            hashed_b,
            "hashed runtime lock path should differ across repos: A={} B={}",
            hashed_a.display(),
            hashed_b.display()
        );

        // restore env/cwd
        if let Some(v) = old_xdg {
            std::env::set_var("XDG_RUNTIME_DIR", v);
        } else {
            std::env::remove_var("XDG_RUNTIME_DIR");
        }
    }

    #[test]
    fn test_candidate_lock_paths_repo_scoped() {
        // Create a temporary git repository and ensure repo-scoped lock paths are preferred
        let td = tempfile::tempdir().expect("tmpdir");
        let old_cwd = std::env::current_dir().expect("cwd");
        let old_xdg = std::env::var("XDG_RUNTIME_DIR").ok();

        // Use a temp runtime dir to make the hashed path predictable and writable
        std::env::set_var("XDG_RUNTIME_DIR", td.path());
        std::env::set_current_dir(td.path()).expect("chdir");

        // Initialize a git repo
        let _ = std::fs::create_dir_all(td.path().join(".git"));
        // Prefer actual git init if available (more realistic)
        let _ = std::process::Command::new("git")
            .arg("init")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();

        // Resolve repo root (should be Some for initialized repo)
        let root = repo_root().unwrap_or_else(|| td.path().to_path_buf());

        // Compute expected candidates
        let first = root.join(".aifo-coder.lock");
        let key = normalized_repo_key_for_hash(&root);
        let mut second_base = std::env::var("XDG_RUNTIME_DIR")
            .ok()
            .filter(|s| !s.is_empty())
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::env::temp_dir());
        second_base.push(format!(
            "aifo-coder.{}.lock",
            crate::hash_repo_key_hex(&key)
        ));

        let paths = candidate_lock_paths();
        assert_eq!(
            paths.get(0),
            Some(&first),
            "first candidate must be in-repo lock path"
        );
        assert_eq!(
            paths.get(1),
            Some(&second_base),
            "second candidate must be hashed runtime-scoped lock path"
        );

        // Restore env and cwd
        if let Some(v) = old_xdg {
            std::env::set_var("XDG_RUNTIME_DIR", v);
        } else {
            std::env::remove_var("XDG_RUNTIME_DIR");
        }
        std::env::set_current_dir(old_cwd).ok();
    }

    #[cfg(windows)]
    #[test]
    fn test_normalized_repo_key_windows_drive_uppercase_and_backslashes() {
        // Create a temp dir and verify normalization rules:
        // - case-fold whole path
        // - separators are backslashes
        // - drive letter uppercased
        let td = tempfile::tempdir().expect("tmpdir");
        let canon = std::fs::canonicalize(td.path())
            .expect("canon")
            .to_string_lossy()
            .to_string();

        let norm = normalized_repo_key_for_hash(td.path());
        // Build expected normalization from canonical path
        let lower = canon.replace('/', "\\").to_ascii_lowercase();
        let mut expected = lower.into_bytes();
        if expected.len() >= 2 && expected[1] == b':' {
            expected[0] = (expected[0] as char).to_ascii_uppercase() as u8;
        }
        let expected = String::from_utf8(expected).unwrap();
        assert_eq!(norm, expected, "normalized repo key mismatch on Windows");
    }

    #[test]
    fn test_build_docker_cmd_uses_per_pane_state_mounts() {
        // Skip if docker isn't available on this host
        if crate::container_runtime_path().is_err() {
            eprintln!("skipping: docker not found in PATH");
            return;
        }

        let td = tempfile::tempdir().expect("tmpdir");
        let state_dir = td.path().to_path_buf();

        // Save and set env
        let old = std::env::var("AIFO_CODER_FORK_STATE_DIR").ok();
        std::env::set_var("AIFO_CODER_FORK_STATE_DIR", &state_dir);

        let args = vec!["--help".to_string()];
        let (_cmd, preview) =
            crate::build_docker_cmd("aider", &args, "alpine:3.20", None).expect("build_docker_cmd");

        let sd_aider = format!("{}:/home/coder/.aider", state_dir.join(".aider").display());
        let sd_codex = format!("{}:/home/coder/.codex", state_dir.join(".codex").display());
        let sd_crush = format!("{}:/home/coder/.crush", state_dir.join(".crush").display());

        assert!(
            preview.contains(&sd_aider),
            "preview missing per-pane .aider mount: {}",
            preview
        );
        assert!(
            preview.contains(&sd_codex),
            "preview missing per-pane .codex mount: {}",
            preview
        );
        assert!(
            preview.contains(&sd_crush),
            "preview missing per-pane .crush mount: {}",
            preview
        );

        // Ensure home-based mounts for these dirs are not present when per-pane state is set
        if let Some(home) = home::home_dir() {
            let home_aider = format!("{}:/home/coder/.aider", home.join(".aider").display());
            let home_codex = format!("{}:/home/coder/.codex", home.join(".codex").display());
            let home_crush1 = format!(
                "{}:/home/coder/.local/share/crush",
                home.join(".local").join("share").join("crush").display()
            );
            let home_crush2 = format!("{}:/home/coder/.crush", home.join(".crush").display());
            assert!(
                !preview.contains(&home_aider),
                "preview should not include HOME .aider when per-pane state is set: {}",
                preview
            );
            assert!(
                !preview.contains(&home_codex),
                "preview should not include HOME .codex when per-pane state is set: {}",
                preview
            );
            assert!(
                !preview.contains(&home_crush1),
                "preview should not include HOME .local/share/crush when per-pane state is set: {}",
                preview
            );
            assert!(
                !preview.contains(&home_crush2),
                "preview should not include HOME .crush when per-pane state is set: {}",
                preview
            );
        }

        // Restore env
        if let Some(v) = old {
            std::env::set_var("AIFO_CODER_FORK_STATE_DIR", v);
        } else {
            std::env::remove_var("AIFO_CODER_FORK_STATE_DIR");
        }
    }

    // -------------------------
    // Phase 2 unit tests
    // -------------------------

    #[test]
    fn test_fork_sanitize_base_label_rules() {
        assert_eq!(fork_sanitize_base_label("Main Feature"), "main-feature");
        assert_eq!(
            fork_sanitize_base_label("Release/2025.09"),
            "release-2025-09"
        );
        assert_eq!(fork_sanitize_base_label("...Weird__Name///"), "weird-name");
        // Length trimming and trailing cleanup
        let long = "A".repeat(200);
        let s = fork_sanitize_base_label(&long);
        assert!(
            !s.is_empty() && s.len() <= 48,
            "sanitized too long: {}",
            s.len()
        );
    }

    fn have_git() -> bool {
        std::process::Command::new("git")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    // Minimal helper to initialize a git repository with one commit
    fn init_repo(dir: &std::path::Path) {
        let _ = std::process::Command::new("git")
            .arg("init")
            .current_dir(dir)
            .status();
        let _ = std::process::Command::new("git")
            .args(["config", "user.name", "UT"])
            .current_dir(dir)
            .status();
        let _ = std::process::Command::new("git")
            .args(["config", "user.email", "ut@example.com"])
            .current_dir(dir)
            .status();
        let _ = std::fs::write(dir.join("init.txt"), "x\n");
        let _ = std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(dir)
            .status();
        let _ = std::process::Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(dir)
            .status();
    }

    #[test]
    fn test_fork_base_info_branch_and_detached() {
        if !have_git() {
            eprintln!("skipping: git not found in PATH");
            return;
        }
        let td = tempfile::tempdir().expect("tmpdir");
        let repo = td.path();

        // init repo
        assert!(std::process::Command::new("git")
            .args(["init"])
            .current_dir(repo)
            .status()
            .expect("git init")
            .success());

        // configure identity (commit-tree does not need it, but normal commit may)
        let _ = std::process::Command::new("git")
            .args(["config", "user.name", "AIFO Test"])
            .current_dir(repo)
            .status();
        let _ = std::process::Command::new("git")
            .args(["config", "user.email", "aifo@example.com"])
            .current_dir(repo)
            .status();

        // make initial commit
        std::fs::write(repo.join("README.md"), "hello\n").expect("write");
        assert!(std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());
        assert!(std::process::Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());

        // verify base info on branch
        let (label, base, head) = fork_base_info(repo).expect("base info");
        assert!(!head.is_empty(), "HEAD sha must be non-empty");
        // Default branch could be 'master' or 'main' depending on git config; accept either
        assert!(
            base == "master" || base == "main",
            "expected base to be current branch name, got {}",
            base
        );
        assert!(
            label == "master" || label == "main",
            "expected label to match sanitized branch name, got {}",
            label
        );

        // detached
        assert!(std::process::Command::new("git")
            .args(["checkout", "--detach", "HEAD"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());
        let (label2, base2, head2) = fork_base_info(repo).expect("base info detached");
        assert_eq!(label2, "detached");
        assert_eq!(base2, head2);
    }

    #[test]
    fn test_fork_create_snapshot_commit_exists() {
        if !have_git() {
            eprintln!("skipping: git not found in PATH");
            return;
        }
        let td = tempfile::tempdir().expect("tmpdir");
        let repo = td.path();

        // init repo with one commit
        assert!(std::process::Command::new("git")
            .args(["init"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());
        let _ = std::process::Command::new("git")
            .args(["config", "user.name", "AIFO Test"])
            .current_dir(repo)
            .status();
        let _ = std::process::Command::new("git")
            .args(["config", "user.email", "aifo@example.com"])
            .current_dir(repo)
            .status();
        std::fs::write(repo.join("a.txt"), "a\n").unwrap();
        assert!(std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());
        assert!(std::process::Command::new("git")
            .args(["commit", "-m", "c1"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());

        // dirty change (unstaged or staged)
        std::fs::write(repo.join("b.txt"), "b\n").unwrap();

        // create snapshot
        let sid = "ut";
        let snap = fork_create_snapshot(repo, sid).expect("snapshot");
        assert_eq!(snap.len(), 40, "snapshot should be a 40-hex sha: {}", snap);

        // verify it's a commit object
        let out = std::process::Command::new("git")
            .arg("cat-file")
            .arg("-t")
            .arg(&snap)
            .current_dir(repo)
            .output()
            .expect("git cat-file");
        let t = String::from_utf8_lossy(&out.stdout).trim().to_string();
        assert_eq!(
            t, "commit",
            "snapshot object type must be commit, got {}",
            t
        );
    }

    #[test]
    fn test_fork_clone_and_checkout_panes_creates_branches() {
        if !have_git() {
            eprintln!("skipping: git not found in PATH");
            return;
        }
        let td = tempfile::tempdir().expect("tmpdir");
        let repo = td.path();

        // init repo with one commit on default branch
        assert!(std::process::Command::new("git")
            .args(["init"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());
        let _ = std::process::Command::new("git")
            .args(["config", "user.name", "AIFO Test"])
            .current_dir(repo)
            .status();
        let _ = std::process::Command::new("git")
            .args(["config", "user.email", "aifo@example.com"])
            .current_dir(repo)
            .status();
        std::fs::write(repo.join("file.txt"), "x\n").unwrap();
        assert!(std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());
        assert!(std::process::Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());

        // Determine current branch name
        let out = std::process::Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(repo)
            .output()
            .unwrap();
        let cur_branch = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let base_label = fork_sanitize_base_label(&cur_branch);

        let sid = "forksid";
        let res = fork_clone_and_checkout_panes(repo, sid, 2, &cur_branch, &base_label, false)
            .expect("clone panes");
        assert_eq!(res.len(), 2, "expected two panes");

        // Verify branches are checked out in panes
        for (idx, (pane_dir, branch)) in res.iter().enumerate() {
            assert!(
                pane_dir.exists(),
                "pane dir must exist: {}",
                pane_dir.display()
            );
            let out = std::process::Command::new("git")
                .args(["rev-parse", "--abbrev-ref", "HEAD"])
                .current_dir(pane_dir)
                .output()
                .unwrap();
            let head_branch = String::from_utf8_lossy(&out.stdout).trim().to_string();
            assert_eq!(
                &head_branch,
                branch,
                "pane {} HEAD should be {}",
                idx + 1,
                branch
            );
        }
    }

    #[test]
    fn test_fork_merge_fetch_creates_branches_and_meta() {
        if !have_git() {
            eprintln!("skipping: git not found in PATH");
            return;
        }
        let td = tempfile::tempdir().expect("tmpdir");
        let repo = td.path();

        // init repo with one commit on default branch
        assert!(std::process::Command::new("git")
            .args(["init"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());
        let _ = std::process::Command::new("git")
            .args(["config", "user.name", "AIFO Test"])
            .current_dir(repo)
            .status();
        let _ = std::process::Command::new("git")
            .args(["config", "user.email", "aifo@example.com"])
            .current_dir(repo)
            .status();
        std::fs::write(repo.join("seed.txt"), "seed\n").unwrap();
        assert!(std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());
        assert!(std::process::Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());

        // Determine current branch name and label
        let out = std::process::Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(repo)
            .output()
            .unwrap();
        let cur_branch = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let base_label = fork_sanitize_base_label(&cur_branch);

        // Create a fork session with two panes
        let sid = "sid-merge-fetch";
        let clones = fork_clone_and_checkout_panes(repo, sid, 2, &cur_branch, &base_label, false)
            .expect("clone panes");
        assert_eq!(clones.len(), 2, "expected two panes");

        // Make independent commits in each pane
        for (idx, (pane_dir, _br)) in clones.iter().enumerate() {
            let fname = format!("pane-{}.txt", idx + 1);
            std::fs::write(pane_dir.join(&fname), format!("pane {}\n", idx + 1)).unwrap();
            let _ = std::process::Command::new("git")
                .args(["config", "user.name", "AIFO Test"])
                .current_dir(pane_dir)
                .status();
            let _ = std::process::Command::new("git")
                .args(["config", "user.email", "aifo@example.com"])
                .current_dir(pane_dir)
                .status();
            assert!(std::process::Command::new("git")
                .args(["add", "-A"])
                .current_dir(pane_dir)
                .status()
                .unwrap()
                .success());
            assert!(std::process::Command::new("git")
                .args(["commit", "-m", &format!("pane {}", idx + 1)])
                .current_dir(pane_dir)
                .status()
                .unwrap()
                .success());
        }

        // Perform fetch merge strategy
        let res = fork_merge_branches_by_session(repo, sid, MergingStrategy::Fetch, true, false);
        assert!(
            res.is_ok(),
            "fetch merge strategy should succeed: {:?}",
            res.err()
        );

        // Verify branches exist in the original repo
        for (_pane_dir, branch) in &clones {
            let ok = std::process::Command::new("git")
                .args(["rev-parse", "--verify", branch])
                .current_dir(repo)
                .status()
                .unwrap()
                .success();
            assert!(ok, "expected branch '{}' to exist in original repo", branch);
        }

        // Verify metadata contains merge_strategy=fetch
        let meta_path = repo
            .join(".aifo-coder")
            .join("forks")
            .join(sid)
            .join(".meta.json");
        let meta = std::fs::read_to_string(&meta_path).expect("read meta");
        assert!(
            meta.contains("\"merge_strategy\"") && meta.contains("fetch"),
            "meta should include merge_strategy=fetch, got: {}",
            meta
        );
    }

    #[test]
    fn test_fork_merge_octopus_success_creates_merge_branch_and_deletes_pane_branches() {
        if !have_git() {
            eprintln!("skipping: git not found in PATH");
            return;
        }
        let td = tempfile::tempdir().expect("tmpdir");
        let repo = td.path();

        // init repo with one commit on default branch
        assert!(std::process::Command::new("git")
            .args(["init"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());
        let _ = std::process::Command::new("git")
            .args(["config", "user.name", "AIFO Test"])
            .current_dir(repo)
            .status();
        let _ = std::process::Command::new("git")
            .args(["config", "user.email", "aifo@example.com"])
            .current_dir(repo)
            .status();
        std::fs::write(repo.join("seed.txt"), "seed\n").unwrap();
        assert!(std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());
        assert!(std::process::Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());

        // Determine current branch name and base label
        let out = std::process::Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(repo)
            .output()
            .unwrap();
        let cur_branch = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let base_label = fork_sanitize_base_label(&cur_branch);

        // Create a fork session with two panes and make non-conflicting commits
        let sid = "sid-merge-oct-success";
        let clones = fork_clone_and_checkout_panes(repo, sid, 2, &cur_branch, &base_label, false)
            .expect("clone panes");
        assert_eq!(clones.len(), 2, "expected two panes");
        for (idx, (pane_dir, _)) in clones.iter().enumerate() {
            let fname = format!("pane-success-{}.txt", idx + 1);
            std::fs::write(pane_dir.join(&fname), format!("ok {}\n", idx + 1)).unwrap();
            let _ = std::process::Command::new("git")
                .args(["config", "user.name", "AIFO Test"])
                .current_dir(pane_dir)
                .status();
            let _ = std::process::Command::new("git")
                .args(["config", "user.email", "aifo@example.com"])
                .current_dir(pane_dir)
                .status();
            assert!(std::process::Command::new("git")
                .args(["add", "-A"])
                .current_dir(pane_dir)
                .status()
                .unwrap()
                .success());
            assert!(std::process::Command::new("git")
                .args(["commit", "-m", &format!("pane ok {}", idx + 1)])
                .current_dir(pane_dir)
                .status()
                .unwrap()
                .success());
        }

        // Perform octopus merge
        let res = fork_merge_branches_by_session(repo, sid, MergingStrategy::Octopus, true, false);
        assert!(res.is_ok(), "octopus merge should succeed: {:?}", res.err());

        // Verify we are on merge/<sid>
        let out2 = std::process::Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(repo)
            .output()
            .unwrap();
        let head_branch = String::from_utf8_lossy(&out2.stdout).trim().to_string();
        assert_eq!(
            head_branch,
            format!("merge/{}", sid),
            "expected HEAD to be merge/<sid>"
        );

        // Verify pane branches are deleted from original repo
        for (_pane_dir, branch) in &clones {
            let ok = std::process::Command::new("git")
                .args(["show-ref", "--verify", &format!("refs/heads/{}", branch)])
                .current_dir(repo)
                .status()
                .unwrap()
                .success();
            assert!(
                !ok,
                "pane branch '{}' should be deleted after octopus merge",
                branch
            );
        }

        // Verify metadata contains merge_target and merge_commit_sha
        let meta_path = repo
            .join(".aifo-coder")
            .join("forks")
            .join(sid)
            .join(".meta.json");
        let meta2 = std::fs::read_to_string(&meta_path).expect("read meta2");
        assert!(
            meta2.contains("\"merge_target\"") && meta2.contains(&format!("merge/{}", sid)),
            "meta should include merge_target=merge/<sid>: {}",
            meta2
        );
        assert!(
            meta2.contains("\"merge_commit_sha\""),
            "meta should include merge_commit_sha: {}",
            meta2
        );
    }

    #[test]
    fn test_fork_merge_octopus_conflict_sets_meta_and_leaves_branches() {
        if !have_git() {
            eprintln!("skipping: git not found in PATH");
            return;
        }
        let td = tempfile::tempdir().expect("tmpdir");
        let repo = td.path();

        // init repo with one commit on default branch
        assert!(std::process::Command::new("git")
            .args(["init"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());
        let _ = std::process::Command::new("git")
            .args(["config", "user.name", "AIFO Test"])
            .current_dir(repo)
            .status();
        let _ = std::process::Command::new("git")
            .args(["config", "user.email", "aifo@example.com"])
            .current_dir(repo)
            .status();
        std::fs::write(repo.join("seed.txt"), "seed\n").unwrap();
        assert!(std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());
        assert!(std::process::Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());

        // Determine current branch
        let out = std::process::Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(repo)
            .output()
            .unwrap();
        let cur_branch = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let base_label = fork_sanitize_base_label(&cur_branch);

        // Create two panes with conflicting changes to the same file
        let sid = "sid-merge-oct-conflict";
        let clones = fork_clone_and_checkout_panes(repo, sid, 2, &cur_branch, &base_label, false)
            .expect("clone panes");
        assert_eq!(clones.len(), 2, "expected two panes");

        // Pane 1 writes conflict.txt
        {
            let (pane_dir, _) = &clones[0];
            std::fs::write(pane_dir.join("conflict.txt"), "A\n").unwrap();
            let _ = std::process::Command::new("git")
                .args(["config", "user.name", "AIFO Test"])
                .current_dir(pane_dir)
                .status();
            let _ = std::process::Command::new("git")
                .args(["config", "user.email", "aifo@example.com"])
                .current_dir(pane_dir)
                .status();
            assert!(std::process::Command::new("git")
                .args(["add", "-A"])
                .current_dir(pane_dir)
                .status()
                .unwrap()
                .success());
            assert!(std::process::Command::new("git")
                .args(["commit", "-m", "pane1"])
                .current_dir(pane_dir)
                .status()
                .unwrap()
                .success());
        }
        // Pane 2 writes conflicting content
        {
            let (pane_dir, _) = &clones[1];
            std::fs::write(pane_dir.join("conflict.txt"), "B\n").unwrap();
            let _ = std::process::Command::new("git")
                .args(["config", "user.name", "AIFO Test"])
                .current_dir(pane_dir)
                .status();
            let _ = std::process::Command::new("git")
                .args(["config", "user.email", "aifo@example.com"])
                .current_dir(pane_dir)
                .status();
            assert!(std::process::Command::new("git")
                .args(["add", "-A"])
                .current_dir(pane_dir)
                .status()
                .unwrap()
                .success());
            assert!(std::process::Command::new("git")
                .args(["commit", "-m", "pane2"])
                .current_dir(pane_dir)
                .status()
                .unwrap()
                .success());
        }

        // Attempt octopus merge (should fail due to conflict)
        let res = fork_merge_branches_by_session(repo, sid, MergingStrategy::Octopus, true, false);
        assert!(res.is_err(), "octopus merge should fail due to conflicts");

        // Metadata should record merge_failed: true
        let meta_path = repo
            .join(".aifo-coder")
            .join("forks")
            .join(sid)
            .join(".meta.json");
        let meta = std::fs::read_to_string(&meta_path).expect("read meta");
        assert!(
            meta.contains("\"merge_failed\":true"),
            "meta should include merge_failed:true, got: {}",
            meta
        );

        // Fetched pane branches should exist in original repo (not deleted)
        for (_pane_dir, branch) in &clones {
            let ok = std::process::Command::new("git")
                .args(["show-ref", "--verify", &format!("refs/heads/{}", branch)])
                .current_dir(repo)
                .status()
                .unwrap()
                .success();
            assert!(
                ok,
                "pane branch '{}' should exist after failed merge",
                branch
            );
        }

        // Repo should be in conflict state (has unmerged paths)
        let out2 = std::process::Command::new("git")
            .args(["ls-files", "-u"])
            .current_dir(repo)
            .output()
            .unwrap();
        assert!(
            !out2.stdout.is_empty(),
            "expected unmerged paths after failed octopus merge"
        );
    }

    #[test]
    fn test_fork_clone_and_checkout_panes_inits_submodules() {
        if !have_git() {
            eprintln!("skipping: git not found in PATH");
            return;
        }
        let td = tempfile::tempdir().expect("tmpdir");

        // Create submodule repository
        let sub = td.path().join("sm");
        std::fs::create_dir_all(&sub).expect("mkdir sm");
        assert!(std::process::Command::new("git")
            .args(["init"])
            .current_dir(&sub)
            .status()
            .unwrap()
            .success());
        let _ = std::process::Command::new("git")
            .args(["config", "user.name", "AIFO Test"])
            .current_dir(&sub)
            .status();
        let _ = std::process::Command::new("git")
            .args(["config", "user.email", "aifo@example.com"])
            .current_dir(&sub)
            .status();
        std::fs::write(sub.join("sub.txt"), "sub\n").unwrap();
        assert!(std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(&sub)
            .status()
            .unwrap()
            .success());
        assert!(std::process::Command::new("git")
            .args(["commit", "-m", "sub init"])
            .current_dir(&sub)
            .status()
            .unwrap()
            .success());

        // Create base repository and add submodule
        let base = td.path().join("base");
        std::fs::create_dir_all(&base).expect("mkdir base");
        assert!(std::process::Command::new("git")
            .args(["init"])
            .current_dir(&base)
            .status()
            .unwrap()
            .success());
        let _ = std::process::Command::new("git")
            .args(["config", "user.name", "AIFO Test"])
            .current_dir(&base)
            .status();
        let _ = std::process::Command::new("git")
            .args(["config", "user.email", "aifo@example.com"])
            .current_dir(&base)
            .status();
        std::fs::write(base.join("file.txt"), "x\n").unwrap();
        assert!(std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(&base)
            .status()
            .unwrap()
            .success());
        assert!(std::process::Command::new("git")
            .args(["commit", "-m", "base init"])
            .current_dir(&base)
            .status()
            .unwrap()
            .success());

        // Add submodule pointing to local path; allow file transport explicitly for modern Git
        let sub_path = sub.display().to_string();
        assert!(std::process::Command::new("git")
            .args([
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                &sub_path,
                "submod"
            ])
            .current_dir(&base)
            .status()
            .unwrap()
            .success());
        assert!(std::process::Command::new("git")
            .args(["commit", "-m", "add submodule"])
            .current_dir(&base)
            .status()
            .unwrap()
            .success());

        // Determine current branch name
        let out = std::process::Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(&base)
            .output()
            .unwrap();
        let cur_branch = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let base_label = fork_sanitize_base_label(&cur_branch);

        // Clone panes and ensure submodule is initialized in clone
        let res =
            fork_clone_and_checkout_panes(&base, "sid-sub", 1, &cur_branch, &base_label, false)
                .expect("clone panes with submodule");
        assert_eq!(res.len(), 1);
        let pane_dir = &res[0].0;
        let sub_file = pane_dir.join("submod").join("sub.txt");
        assert!(
            sub_file.exists(),
            "expected submodule file to exist in clone: {}",
            sub_file.display()
        );
    }

    #[test]
    fn test_fork_clone_and_checkout_panes_lfs_marker_does_not_fail_without_lfs() {
        if !have_git() {
            eprintln!("skipping: git not found in PATH");
            return;
        }
        let td = tempfile::tempdir().expect("tmpdir");
        let repo = td.path();

        // init repo with a .gitattributes marking LFS filters (may or may not have git-lfs installed)
        assert!(std::process::Command::new("git")
            .args(["init"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());
        let _ = std::process::Command::new("git")
            .args(["config", "user.name", "AIFO Test"])
            .current_dir(repo)
            .status();
        let _ = std::process::Command::new("git")
            .args(["config", "user.email", "aifo@example.com"])
            .current_dir(repo)
            .status();

        std::fs::write(
            repo.join(".gitattributes"),
            "*.bin filter=lfs diff=lfs merge=lfs -text\n",
        )
        .unwrap();
        std::fs::write(repo.join("a.bin"), b"\x00\x01\x02").unwrap();
        assert!(std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());
        // Commit even if lfs not installed; the filter may be ignored, but commit should succeed
        assert!(std::process::Command::new("git")
            .args(["commit", "-m", "add lfs marker"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());

        let out = std::process::Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(repo)
            .output()
            .unwrap();
        let cur_branch = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let base_label = fork_sanitize_base_label(&cur_branch);

        // Should not fail regardless of git-lfs availability
        let res =
            fork_clone_and_checkout_panes(repo, "sid-lfs", 1, &cur_branch, &base_label, false)
                .expect("clone panes with lfs marker");
        assert_eq!(res.len(), 1);
    }

    #[test]
    fn test_repo_uses_lfs_quick_top_level_gitattributes() {
        let td = tempfile::tempdir().expect("tmpdir");
        let repo = td.path();
        // Create top-level .gitattributes with lfs filter
        std::fs::write(
            repo.join(".gitattributes"),
            "*.bin filter=lfs diff=lfs merge=lfs -text\n",
        )
        .unwrap();
        assert!(
            repo_uses_lfs_quick(repo),
            "expected repo_uses_lfs_quick to detect top-level filter=lfs"
        );
    }

    #[test]
    fn test_repo_uses_lfs_quick_nested_gitattributes() {
        let td = tempfile::tempdir().expect("tmpdir");
        let repo = td.path();
        let nested = repo.join("assets").join("media");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(
            nested.join(".gitattributes"),
            "*.png filter=lfs diff=lfs merge=lfs -text\n",
        )
        .unwrap();
        assert!(
            repo_uses_lfs_quick(repo),
            "expected repo_uses_lfs_quick to detect nested filter=lfs"
        );
    }

    #[test]
    fn test_repo_uses_lfs_quick_lfsconfig_present() {
        let td = tempfile::tempdir().expect("tmpdir");
        let repo = td.path();
        std::fs::write(
            repo.join(".lfsconfig"),
            "[lfs]\nurl = https://example.com/lfs\n",
        )
        .unwrap();
        assert!(
            repo_uses_lfs_quick(repo),
            "expected repo_uses_lfs_quick to detect .lfsconfig presence"
        );
    }

    #[test]
    fn test_fork_clean_protects_ahead_and_force_deletes() {
        if !have_git() {
            eprintln!("skipping: git not found in PATH");
            return;
        }
        let td = tempfile::tempdir().expect("tmpdir");
        let root = td.path().to_path_buf();

        // Session/pane setup
        let sid = "sid-ahead";
        let base = root.join(".aifo-coder").join("forks").join(sid);
        let pane = base.join("pane-1");
        std::fs::create_dir_all(&pane).unwrap();
        init_repo(&pane);

        // Record base_commit_sha as current HEAD
        let head = std::process::Command::new("git")
            .args(["rev-parse", "--verify", "HEAD"])
            .current_dir(&pane)
            .output()
            .unwrap();
        let head_sha = String::from_utf8_lossy(&head.stdout).trim().to_string();

        // Write minimal meta.json
        std::fs::create_dir_all(&base).unwrap();
        let meta = format!(
            "{{ \"created_at\": {}, \"base_label\": \"main\", \"base_ref_or_sha\": \"main\", \"base_commit_sha\": \"{}\", \"panes\": 1, \"pane_dirs\": [\"{}\"], \"branches\": [\"fork/main/{sid}-1\"], \"layout\": \"tiled\" }}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(),
            head_sha,
            pane.display()
        );
        std::fs::write(base.join(".meta.json"), meta).unwrap();

        // Create an extra commit in the pane to make it "ahead" of base_commit_sha
        std::fs::write(pane.join("new.txt"), "y\n").unwrap();
        assert!(std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(&pane)
            .status()
            .unwrap()
            .success());
        assert!(std::process::Command::new("git")
            .args(["commit", "-m", "advance pane"])
            .current_dir(&pane)
            .status()
            .unwrap()
            .success());

        // Default clean should REFUSE because pane is ahead
        let opts_refuse = aifo_coder::ForkCleanOpts {
            session: Some(sid.to_string()),
            older_than_days: None,
            all: false,
            dry_run: false,
            yes: false,
            force: false,
            keep_dirty: false,
            json: false,
        };
        let code = aifo_coder::fork_clean(&root, &opts_refuse).expect("fork_clean refuse");
        assert_eq!(code, 1, "expected refusal when pane is ahead");
        assert!(base.exists(), "session dir must remain after refusal");

        // keep-dirty should succeed, keep the ahead pane and update meta
        let opts_keep = aifo_coder::ForkCleanOpts {
            session: Some(sid.to_string()),
            older_than_days: None,
            all: false,
            dry_run: false,
            yes: true, // skip prompt
            force: false,
            keep_dirty: true,
            json: false,
        };
        let code2 = aifo_coder::fork_clean(&root, &opts_keep).expect("fork_clean keep-dirty");
        assert_eq!(
            code2, 0,
            "keep-dirty should succeed (no deletions if all panes protected)"
        );
        assert!(pane.exists(), "ahead pane should remain");
        let meta2 = std::fs::read_to_string(base.join(".meta.json")).expect("read meta2");
        assert!(
            meta2.contains("\"panes_remaining\""),
            "meta should be updated to include panes_remaining"
        );

        // force should delete the session
        let opts_force = aifo_coder::ForkCleanOpts {
            session: Some(sid.to_string()),
            older_than_days: None,
            all: false,
            dry_run: false,
            yes: true,
            force: true,
            keep_dirty: false,
            json: false,
        };
        let code3 = aifo_coder::fork_clean(&root, &opts_force).expect("fork_clean force");
        assert_eq!(code3, 0, "force should succeed");
        assert!(!base.exists(), "session dir should be removed by force");
    }

    #[test]
    fn test_fork_clean_protects_submodule_dirty() {
        if !have_git() {
            eprintln!("skipping: git not found in PATH");
            return;
        }
        let td = tempfile::tempdir().expect("tmpdir");
        let root = td.path().to_path_buf();

        // Prepare submodule upstream repo
        let upstream = td.path().join("upstream-sm");
        std::fs::create_dir_all(&upstream).unwrap();
        assert!(std::process::Command::new("git")
            .args(["init"])
            .current_dir(&upstream)
            .status()
            .unwrap()
            .success());
        let _ = std::process::Command::new("git")
            .args(["config", "user.name", "UT"])
            .current_dir(&upstream)
            .status();
        let _ = std::process::Command::new("git")
            .args(["config", "user.email", "ut@example.com"])
            .current_dir(&upstream)
            .status();
        std::fs::write(upstream.join("a.txt"), "a\n").unwrap();
        assert!(std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(&upstream)
            .status()
            .unwrap()
            .success());
        assert!(std::process::Command::new("git")
            .args(["commit", "-m", "sm init"])
            .current_dir(&upstream)
            .status()
            .unwrap()
            .success());

        // Create pane repo and add submodule
        let sid = "sid-subdirty";
        let base = root.join(".aifo-coder").join("forks").join(sid);
        let pane = base.join("pane-1");
        std::fs::create_dir_all(&pane).unwrap();
        assert!(std::process::Command::new("git")
            .args(["init"])
            .current_dir(&pane)
            .status()
            .unwrap()
            .success());
        let _ = std::process::Command::new("git")
            .args(["config", "user.name", "UT"])
            .current_dir(&pane)
            .status();
        let _ = std::process::Command::new("git")
            .args(["config", "user.email", "ut@example.com"])
            .current_dir(&pane)
            .status();
        // Commit initial file
        std::fs::write(&pane.join("root.txt"), "r\n").unwrap();
        assert!(std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(&pane)
            .status()
            .unwrap()
            .success());
        assert!(std::process::Command::new("git")
            .args(["commit", "-m", "root"])
            .current_dir(&pane)
            .status()
            .unwrap()
            .success());
        // Add submodule (allow file protocol)
        let up_path = upstream.display().to_string();
        assert!(std::process::Command::new("git")
            .args([
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                &up_path,
                "sub"
            ])
            .current_dir(&pane)
            .status()
            .unwrap()
            .success());
        assert!(std::process::Command::new("git")
            .args(["commit", "-m", "add submodule"])
            .current_dir(&pane)
            .status()
            .unwrap()
            .success());

        // Record base_commit_sha as current HEAD in pane
        let head = std::process::Command::new("git")
            .args(["rev-parse", "--verify", "HEAD"])
            .current_dir(&pane)
            .output()
            .unwrap();
        let head_sha = String::from_utf8_lossy(&head.stdout).trim().to_string();
        std::fs::create_dir_all(&base).unwrap();
        let meta = format!(
            "{{ \"created_at\": {}, \"base_label\": \"main\", \"base_ref_or_sha\": \"main\", \"base_commit_sha\": \"{}\", \"panes\": 1, \"pane_dirs\": [\"{}\"], \"branches\": [\"fork/main/{sid}-1\"], \"layout\": \"tiled\" }}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(),
            head_sha,
            pane.display()
        );
        std::fs::write(base.join(".meta.json"), meta).unwrap();

        // Make submodule dirty relative to recorded commit: commit new change inside pane/sub (the submodule checkout)
        let subdir = pane.join("sub");
        let _ = std::process::Command::new("git")
            .args(["config", "user.name", "UT"])
            .current_dir(&subdir)
            .status();
        let _ = std::process::Command::new("git")
            .args(["config", "user.email", "ut@example.com"])
            .current_dir(&subdir)
            .status();
        std::fs::write(subdir.join("b.txt"), "b\n").unwrap();
        assert!(std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(&subdir)
            .status()
            .unwrap()
            .success());
        assert!(std::process::Command::new("git")
            .args(["commit", "-m", "advance sub"])
            .current_dir(&subdir)
            .status()
            .unwrap()
            .success());

        // Default clean should refuse due to submodules-dirty
        let opts_refuse = aifo_coder::ForkCleanOpts {
            session: Some(sid.to_string()),
            older_than_days: None,
            all: false,
            dry_run: false,
            yes: false,
            force: false,
            keep_dirty: false,
            json: false,
        };
        let code =
            aifo_coder::fork_clean(&root, &opts_refuse).expect("fork_clean refuse submodule-dirty");
        assert_eq!(code, 1, "expected refusal when submodule is dirty");
        assert!(
            base.exists(),
            "session dir must remain after refusal on submodule-dirty"
        );

        // keep-dirty should keep the pane and succeed
        let opts_keep = aifo_coder::ForkCleanOpts {
            session: Some(sid.to_string()),
            older_than_days: None,
            all: false,
            dry_run: false,
            yes: true,
            force: false,
            keep_dirty: true,
            json: false,
        };
        let code2 =
            aifo_coder::fork_clean(&root, &opts_keep).expect("fork_clean keep-dirty submodule");
        assert_eq!(
            code2, 0,
            "keep-dirty should succeed (no deletions if pane protected)"
        );
        assert!(pane.exists(), "pane with dirty submodule should remain");
    }

    #[test]
    fn test_fork_clean_older_than_deletes_only_old_sessions() {
        if !have_git() {
            eprintln!("skipping: git not found in PATH");
            return;
        }
        let td = tempfile::tempdir().expect("tmpdir");
        let root = td.path().to_path_buf();
        init_repo(&root);

        // Old clean session (older than threshold)
        let sid_old = "sid-old2";
        let base_old = root.join(".aifo-coder").join("forks").join(sid_old);
        let pane_old = base_old.join("pane-1");
        std::fs::create_dir_all(&pane_old).unwrap();
        init_repo(&pane_old);
        let head_old = String::from_utf8_lossy(
            &std::process::Command::new("git")
                .args(["rev-parse", "--verify", "HEAD"])
                .current_dir(&pane_old)
                .output()
                .unwrap()
                .stdout,
        )
        .trim()
        .to_string();
        let old_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            - 20 * 86400;
        let meta_old = format!(
            "{{ \"created_at\": {}, \"base_label\": \"main\", \"base_ref_or_sha\": \"main\", \"base_commit_sha\": \"{}\", \"panes\": 1, \"pane_dirs\": [\"{}\"], \"branches\": [\"fork/main/{sid}-1\"], \"layout\": \"tiled\" }}",
            old_secs, head_old, pane_old.display(), sid = sid_old
        );
        std::fs::create_dir_all(&base_old).unwrap();
        std::fs::write(base_old.join(".meta.json"), meta_old).unwrap();

        // Recent clean session (younger than threshold)
        let sid_new = "sid-new2";
        let base_new = root.join(".aifo-coder").join("forks").join(sid_new);
        let pane_new = base_new.join("pane-1");
        std::fs::create_dir_all(&pane_new).unwrap();
        init_repo(&pane_new);
        let head_new = String::from_utf8_lossy(
            &std::process::Command::new("git")
                .args(["rev-parse", "--verify", "HEAD"])
                .current_dir(&pane_new)
                .output()
                .unwrap()
                .stdout,
        )
        .trim()
        .to_string();
        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let meta_new = format!(
            "{{ \"created_at\": {}, \"base_label\": \"main\", \"base_ref_or_sha\": \"main\", \"base_commit_sha\": \"{}\", \"panes\": 1, \"pane_dirs\": [\"{}\"], \"branches\": [\"fork/main/{sid}-1\"], \"layout\": \"tiled\" }}",
            now_secs, head_new, pane_new.display(), sid = sid_new
        );
        std::fs::create_dir_all(&base_new).unwrap();
        std::fs::write(base_new.join(".meta.json"), meta_new).unwrap();

        // Clean with older-than=10 days should delete only sid-old2
        let opts = aifo_coder::ForkCleanOpts {
            session: None,
            older_than_days: Some(10),
            all: false,
            dry_run: false,
            yes: true,
            force: false,
            keep_dirty: false,
            json: false,
        };
        let code = aifo_coder::fork_clean(&root, &opts).expect("fork_clean older-than");
        assert_eq!(code, 0, "older-than clean should succeed");
        assert!(!base_old.exists(), "old session should be deleted");
        assert!(base_new.exists(), "recent session should remain");
    }

    #[test]
    fn test_fork_create_snapshot_on_empty_repo() {
        if !have_git() {
            eprintln!("skipping: git not found in PATH");
            return;
        }
        let td = tempfile::tempdir().expect("tmpdir");
        let repo = td.path();
        assert!(std::process::Command::new("git")
            .args(["init"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());
        // Create an untracked file; snapshot should still succeed by indexing working tree
        std::fs::write(repo.join("a.txt"), "a\n").unwrap();
        let sid = "empty";
        let snap = fork_create_snapshot(repo, sid).expect("snapshot on empty repo");
        assert_eq!(snap.len(), 40, "snapshot sha length");
        let out = std::process::Command::new("git")
            .args(["cat-file", "-t", &snap])
            .current_dir(repo)
            .output()
            .unwrap();
        let t = String::from_utf8_lossy(&out.stdout).trim().to_string();
        assert_eq!(t, "commit", "snapshot object must be a commit");
    }

    #[test]
    fn test_fork_clone_with_dissociate() {
        if !have_git() {
            eprintln!("skipping: git not found in PATH");
            return;
        }
        let td = tempfile::tempdir().expect("tmpdir");
        let repo = td.path();
        // init repo with one commit
        assert!(std::process::Command::new("git")
            .args(["init"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());
        let _ = std::process::Command::new("git")
            .args(["config", "user.name", "AIFO Test"])
            .current_dir(repo)
            .status();
        let _ = std::process::Command::new("git")
            .args(["config", "user.email", "aifo@example.com"])
            .current_dir(repo)
            .status();
        std::fs::write(repo.join("f.txt"), "x\n").unwrap();
        assert!(std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());
        assert!(std::process::Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());
        // Determine branch
        let out = std::process::Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(repo)
            .output()
            .unwrap();
        let cur_branch = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let base_label = fork_sanitize_base_label(&cur_branch);
        // Clone with dissociate should succeed
        let res =
            fork_clone_and_checkout_panes(repo, "sid-dissoc", 1, &cur_branch, &base_label, true)
                .expect("clone with --dissociate");
        assert_eq!(res.len(), 1);
        assert!(res[0].0.exists());
    }

    #[test]
    fn test_fork_autoclean_removes_only_clean_sessions() {
        if !have_git() {
            eprintln!("skipping: git not found in PATH");
            return;
        }
        let td = tempfile::tempdir().expect("tmpdir");
        let root = td.path().to_path_buf();
        // Initialize base repo to ensure repo_root() detects it
        init_repo(&root);

        // Old clean session
        let sid_clean = "sid-clean-old";
        let base_clean = root.join(".aifo-coder").join("forks").join(sid_clean);
        let pane_clean = base_clean.join("pane-1");
        std::fs::create_dir_all(&pane_clean).unwrap();
        init_repo(&pane_clean);
        // Record base_commit_sha as current HEAD
        let head_clean = String::from_utf8_lossy(
            &std::process::Command::new("git")
                .args(["rev-parse", "--verify", "HEAD"])
                .current_dir(&pane_clean)
                .output()
                .unwrap()
                .stdout,
        )
        .trim()
        .to_string();
        let old_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            - 40 * 86400;
        std::fs::create_dir_all(&base_clean).unwrap();
        let meta_clean = format!(
            "{{ \"created_at\": {}, \"base_label\": \"main\", \"base_ref_or_sha\": \"main\", \"base_commit_sha\": \"{}\", \"panes\": 1, \"pane_dirs\": [\"{}\"], \"branches\": [\"fork/main/{sid}-1\"], \"layout\": \"tiled\" }}",
            old_secs, head_clean, pane_clean.display(), sid = sid_clean
        );
        std::fs::write(base_clean.join(".meta.json"), meta_clean).unwrap();

        // Old protected (ahead) session
        let sid_prot = "sid-protected-old";
        let base_prot = root.join(".aifo-coder").join("forks").join(sid_prot);
        let pane_prot = base_prot.join("pane-1");
        std::fs::create_dir_all(&pane_prot).unwrap();
        init_repo(&pane_prot);
        let head_prot = String::from_utf8_lossy(
            &std::process::Command::new("git")
                .args(["rev-parse", "--verify", "HEAD"])
                .current_dir(&pane_prot)
                .output()
                .unwrap()
                .stdout,
        )
        .trim()
        .to_string();
        std::fs::create_dir_all(&base_prot).unwrap();
        let meta_prot = format!(
            "{{ \"created_at\": {}, \"base_label\": \"main\", \"base_ref_or_sha\": \"main\", \"base_commit_sha\": \"{}\", \"panes\": 1, \"pane_dirs\": [\"{}\"], \"branches\": [\"fork/main/{sid}-1\"], \"layout\": \"tiled\" }}",
            old_secs, head_prot, pane_prot.display(), sid = sid_prot
        );
        std::fs::write(base_prot.join(".meta.json"), meta_prot).unwrap();
        // Make pane ahead of base_commit_sha
        std::fs::write(pane_prot.join("new.txt"), "y\n").unwrap();
        assert!(std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(&pane_prot)
            .status()
            .unwrap()
            .success());
        assert!(std::process::Command::new("git")
            .args(["commit", "-m", "advance pane"])
            .current_dir(&pane_prot)
            .status()
            .unwrap()
            .success());

        // Run autoclean with threshold 1 day
        let old_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&root).unwrap();
        let old_env1 = std::env::var("AIFO_CODER_FORK_AUTOCLEAN").ok();
        let old_env2 = std::env::var("AIFO_CODER_FORK_STALE_DAYS").ok();
        std::env::set_var("AIFO_CODER_FORK_AUTOCLEAN", "1");
        std::env::set_var("AIFO_CODER_FORK_STALE_DAYS", "1");
        fork_autoclean_if_enabled();
        // Restore cwd and env
        std::env::set_current_dir(old_cwd).ok();
        if let Some(v) = old_env1 {
            std::env::set_var("AIFO_CODER_FORK_AUTOCLEAN", v);
        } else {
            std::env::remove_var("AIFO_CODER_FORK_AUTOCLEAN");
        }
        if let Some(v) = old_env2 {
            std::env::set_var("AIFO_CODER_FORK_STALE_DAYS", v);
        } else {
            std::env::remove_var("AIFO_CODER_FORK_STALE_DAYS");
        }

        assert!(
            !base_clean.exists(),
            "clean old session should have been deleted by autoclean"
        );
        assert!(
            base_prot.exists(),
            "protected old session should have been kept by autoclean"
        );
    }

    #[cfg(windows)]
    #[test]
    fn test_windows_helpers_orient_and_builders() {
        use crate::{
            fork_bash_inner_string, fork_ps_inner_string, wt_build_new_tab_args,
            wt_build_split_args, wt_orient_for_layout,
        };
        let agent = "aider";
        let sid = "sidw";
        let tmp = tempfile::tempdir().expect("tmpdir");
        let pane_dir = tmp.path().join("p");
        std::fs::create_dir_all(&pane_dir).unwrap();
        let state_dir = tmp.path().join("s");
        std::fs::create_dir_all(&state_dir).unwrap();
        let child = vec!["aider".to_string(), "--help".to_string()];
        let ps = fork_ps_inner_string(agent, sid, 1, &pane_dir, &state_dir, &child);
        assert!(
            ps.contains("Set-Location '"),
            "ps inner should set location: {}",
            ps
        );
        assert!(
            ps.contains("$env:AIFO_CODER_SKIP_LOCK='1'"),
            "ps inner should set env"
        );
        let bash = fork_bash_inner_string(agent, sid, 2, &pane_dir, &state_dir, &child);
        assert!(bash.contains("cd "), "bash inner should cd");
        assert!(
            bash.contains("export AIFO_CODER_SKIP_LOCK='1'"),
            "bash inner export env"
        );
        // wt orientation
        assert_eq!(wt_orient_for_layout("even-h", 3), "-H");
        assert_eq!(wt_orient_for_layout("even-v", 4), "-V");
        // tiled alternates
        let o2 = wt_orient_for_layout("tiled", 2);
        let o3 = wt_orient_for_layout("tiled", 3);
        assert!(o2 == "-H" || o2 == "-V");
        assert!(o3 == "-H" || o3 == "-V");
        // arg builders
        let psbin = std::path::PathBuf::from("powershell.exe");
        let inner = "cmds";
        let newtab = wt_build_new_tab_args(&psbin, &pane_dir, inner);
        assert_eq!(newtab[0], "wt");
        assert_eq!(newtab[1], "new-tab");
        let split = wt_build_split_args("-H", &psbin, &pane_dir, inner);
        assert_eq!(split[1], "split-pane");
        assert_eq!(split[2], "-H");
        // Wait-Process cmd builder
        let w = crate::ps_wait_process_cmd(&["101", "202", "303"]);
        assert_eq!(w, "Wait-Process -Id 101,202,303");
    }
}
