use std::env;
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use fs2::FileExt;
use std::io;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::net::{TcpStream, ToSocketAddrs, TcpListener, Shutdown};
#[cfg(target_os = "linux")]
use std::os::unix::net::{UnixListener, UnixStream};
use std::time::{Duration, SystemTime};
use which::which;
use once_cell::sync::{Lazy, OnceCell};
use atty;

#[cfg(unix)]
use nix::unistd::{getgid, getuid};

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
 
// Cache for preferred registry prefix resolution within a single process run.
static REGISTRY_PREFIX_CACHE: OnceCell<String> = OnceCell::new();
// Record how the registry prefix was determined this run.
static REGISTRY_PREFIX_SOURCE: OnceCell<String> = OnceCell::new();

/// Locate the Docker runtime binary.
pub fn container_runtime_path() -> io::Result<PathBuf> {
    if let Ok(p) = which("docker") {
        return Ok(p);
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "Docker is required but was not found in PATH.",
    ))
}

/// Bootstrap: install a global typescript in the node sidecar (best-effort).
pub fn toolchain_bootstrap_typescript_global(session_id: &str, verbose: bool) -> io::Result<()> {
    let runtime = container_runtime_path()?;
    let name = sidecar_container_name("node", session_id);

    #[cfg(unix)]
    let uid: u32 = u32::from(getuid());
    #[cfg(unix)]
    let gid: u32 = u32::from(getgid());
    #[cfg(not(unix))]
    let (uid, gid) = (0u32, 0u32);

    let mut args: Vec<String> = vec![
        "docker".to_string(),
        "exec".to_string(),
    ];
    if cfg!(unix) {
        args.push("-u".to_string());
        args.push(format!("{uid}:{gid}"));
    }
    args.push("-w".to_string());
    args.push("/workspace".to_string());
    args.push(name);
    args.push("npm".to_string());
    args.push("install".to_string());
    args.push("-g".to_string());
    args.push("typescript".to_string());

    if verbose {
        eprintln!("aifo-coder: docker: {}", shell_join(&args));
    }

    let mut cmd = Command::new(&runtime);
    for a in &args[1..] {
        cmd.arg(a);
    }
    if !verbose {
        cmd.stdout(Stdio::null()).stderr(Stdio::null());
    }
    let _ = cmd.status();
    Ok(())
}

/// Probe whether the Docker daemon reports AppArmor support, and (on Linux)
/// that the kernel AppArmor facility is enabled.
pub fn docker_supports_apparmor() -> bool {
    let runtime = match container_runtime_path() {
        Ok(p) => p,
        Err(_) => return false,
    };
    let output = Command::new(runtime)
        .args(["info", "--format", "{{json .SecurityOptions}}"])
        .output();
    let Ok(out) = output else { return false };
    let s = String::from_utf8_lossy(&out.stdout).to_lowercase();
    let docker_reports_apparmor = s.contains("apparmor");
    if !docker_reports_apparmor {
        return false;
    }
    // On Linux hosts, also require kernel AppArmor to be enabled.
    if cfg!(target_os = "linux") && !kernel_apparmor_enabled() {
        return false;
    }
    true
}

/// Best-effort detection of AppArmor being enabled in the Linux kernel.
/// Returns true if the kernel facility appears available/enabled.
fn kernel_apparmor_enabled() -> bool {
    // Prefer authoritative kernel knob when present
    if let Ok(content) = fs::read_to_string("/sys/module/apparmor/parameters/enabled") {
        let c = content.trim().to_lowercase();
        if c.starts_with('y') || c.contains("enforce") || c.contains("complain") || c == "1" || c == "yes" || c == "true" {
            // Double-check proc LSM interface presence
            return Path::new("/proc/self/attr/apparmor/current").exists()
                && Path::new("/proc/self/attr/apparmor/exec").exists();
        } else {
            return false;
        }
    }
    // Fallback: require both current and exec proc attributes to exist
    Path::new("/proc/self/attr/apparmor/current").exists()
        && Path::new("/proc/self/attr/apparmor/exec").exists()
}

#[cfg(target_os = "linux")]
fn apparmor_profile_available(name: &str) -> bool {
    if let Ok(list) = fs::read_to_string("/sys/kernel/security/apparmor/profiles") {
        for line in list.lines() {
            let l = line.trim();
            if l.is_empty() {
                continue;
            }
            if l.starts_with(&format!("{name} (")) || l.starts_with(&format!("{name} ")) {
                return true;
            }
        }
    }
    false
}

#[cfg(not(target_os = "linux"))]
fn apparmor_profile_available(_name: &str) -> bool {
    true
}

/// Choose the AppArmor profile to use, if any.
/// - If Docker supports AppArmor, prefer an explicit override via AIFO_CODER_APPARMOR_PROFILE.
/// - On macOS/Windows hosts (Docker-in-VM), default to docker-default to avoid requiring a host-installed custom profile.
/// - On native Linux hosts, prefer the custom "aifo-coder" profile if it is loaded; otherwise fall back to "docker-default"
///   if available; otherwise omit an explicit profile (Docker will choose its default).
pub fn desired_apparmor_profile() -> Option<String> {
    if !docker_supports_apparmor() {
        return None;
    }
    if let Ok(p) = env::var("AIFO_CODER_APPARMOR_PROFILE") {
        let trimmed = p.trim();
        let lower = trimmed.to_lowercase();
        // Allow explicit disabling via env var
        if trimmed.is_empty() || ["none", "no", "off", "false", "0", "disabled", "disable"].contains(&lower.as_str()) {
            return None;
        }
        if cfg!(target_os = "linux") && !apparmor_profile_available(trimmed) {
            eprintln!("aifo-coder: AppArmor profile '{}' not loaded on host; falling back to 'docker-default'.", trimmed);
            if apparmor_profile_available("docker-default") {
                return Some("docker-default".to_string());
            } else {
                eprintln!("aifo-coder: 'docker-default' profile not found; continuing without explicit AppArmor profile.");
                return None;
            }
        }
        return Some(trimmed.to_string());
    }
    if cfg!(target_os = "macos") || cfg!(target_os = "windows") {
        Some("docker-default".to_string())
    } else {
        if apparmor_profile_available("aifo-coder") {
            Some("aifo-coder".to_string())
        } else if apparmor_profile_available("docker-default") {
            eprintln!("aifo-coder: AppArmor profile 'aifo-coder' not loaded; using 'docker-default'.");
            Some("docker-default".to_string())
        } else {
            eprintln!("aifo-coder: No known AppArmor profile loaded; continuing without explicit profile.");
            None
        }
    }
}

/// Quiet variant of desired_apparmor_profile() for diagnostic contexts (no logging).
pub fn desired_apparmor_profile_quiet() -> Option<String> {
    if !docker_supports_apparmor() {
        return None;
    }
    if let Ok(p) = env::var("AIFO_CODER_APPARMOR_PROFILE") {
        let trimmed = p.trim();
        let lower = trimmed.to_lowercase();
        // Allow explicit disabling via env var
        if trimmed.is_empty() || ["none", "no", "off", "false", "0", "disabled", "disable"].contains(&lower.as_str()) {
            return None;
        }
        if cfg!(target_os = "linux") && !apparmor_profile_available(trimmed) {
            if apparmor_profile_available("docker-default") {
                return Some("docker-default".to_string());
            } else {
                return None;
            }
        }
        return Some(trimmed.to_string());
    }
    if cfg!(target_os = "macos") || cfg!(target_os = "windows") {
        Some("docker-default".to_string())
    } else {
        if apparmor_profile_available("aifo-coder") {
            Some("aifo-coder".to_string())
        } else if apparmor_profile_available("docker-default") {
            Some("docker-default".to_string())
        } else {
            None
        }
    }
}

fn is_host_port_reachable(host: &str, port: u16, timeout_ms: u64) -> bool {
    let addrs = (host, port).to_socket_addrs();
    if let Ok(addrs) = addrs {
        let timeout = Duration::from_millis(timeout_ms);
        for addr in addrs {
            if TcpStream::connect_timeout(&addr, timeout).is_ok() {
                return true;
            }
        }
    }
    false
}

fn registry_cache_path() -> Option<PathBuf> {
    let base = env::var("XDG_RUNTIME_DIR").ok().filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    Some(base.join("aifo-coder.regprefix"))
}


fn write_registry_cache_disk(s: &str) {
    if let Some(path) = registry_cache_path() {
        let _ = fs::write(path, s);
    }
}

/// Public helper to invalidate the on-disk registry cache before probing.
/// Does not affect the in-process OnceCell cache for this run.
pub fn invalidate_registry_cache() {
    if let Some(path) = registry_cache_path() {
        let _ = fs::remove_file(path);
    }
}

/// Determine the preferred registry prefix for image references.
/// Precedence:
/// 1) If AIFO_CODER_REGISTRY_PREFIX is set:
///    - empty string forces Docker Hub (no prefix)
///    - non-empty is normalized to end with a single '/' and used as-is
/// 2) Otherwise, if repository.migros.net:443 is reachable, use "repository.migros.net/"
/// 3) Fallback: empty string (Docker Hub)
pub fn preferred_registry_prefix() -> String {
    if let Some(v) = REGISTRY_PREFIX_CACHE.get() {
        return v.clone();
    }
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
        eprintln!("aifo-coder: Using AIFO_CODER_REGISTRY_PREFIX override: '{}'", s);
        let v = s;
        let _ = REGISTRY_PREFIX_CACHE.set(v.clone());
        let _ = REGISTRY_PREFIX_SOURCE.set("env".to_string());
        write_registry_cache_disk(&v);
        return v;
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
            eprintln!("aifo-coder: curl invocation failed; falling back to TCP reachability check.");
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

/// Quiet variant for preferred registry prefix resolution without emitting any logs.
pub fn preferred_registry_prefix_quiet() -> String {
    if let Some(v) = REGISTRY_PREFIX_CACHE.get() {
        return v.clone();
    }
    if let Ok(pref) = env::var("AIFO_CODER_REGISTRY_PREFIX") {
        let trimmed = pref.trim();
        if trimmed.is_empty() {
            let v = String::new();
            let _ = REGISTRY_PREFIX_CACHE.set(v.clone());
            let _ = REGISTRY_PREFIX_SOURCE.set("env-empty".to_string());
            write_registry_cache_disk(&v);
            return v;
        }
        let mut s = trimmed.trim_end_matches('/').to_string();
        s.push('/');
        let v = s;
        let _ = REGISTRY_PREFIX_CACHE.set(v.clone());
        let _ = REGISTRY_PREFIX_SOURCE.set("env".to_string());
        write_registry_cache_disk(&v);
        return v;
    }

    if which("curl").is_ok() {
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
                let v = "repository.migros.net/".to_string();
                let _ = REGISTRY_PREFIX_CACHE.set(v.clone());
                let _ = REGISTRY_PREFIX_SOURCE.set("curl".to_string());
                write_registry_cache_disk(&v);
                return v;
            } else {
                let v = String::new();
                let _ = REGISTRY_PREFIX_CACHE.set(v.clone());
                let _ = REGISTRY_PREFIX_SOURCE.set("curl".to_string());
                write_registry_cache_disk(&v);
                return v;
            }
        }
    }

    let v = if is_host_port_reachable("repository.migros.net", 443, 300) {
        "repository.migros.net/".to_string()
    } else {
        String::new()
    };
    let _ = REGISTRY_PREFIX_CACHE.set(v.clone());
    let _ = REGISTRY_PREFIX_SOURCE.set("tcp".to_string());
    write_registry_cache_disk(&v);
    v
}

/// Return how the registry prefix was determined in this process (env, disk, curl, tcp, unknown).
pub fn preferred_registry_source() -> String {
    REGISTRY_PREFIX_SOURCE
        .get()
        .cloned()
        .unwrap_or_else(|| "unknown".to_string())
}

/// Render a docker -v host:container pair.
pub fn path_pair(host: &Path, container: &str) -> OsString {
    OsString::from(format!("{}:{container}", host.display()))
}

/// Ensure a file exists by creating parent directories as needed.
pub fn ensure_file_exists(p: &Path) -> io::Result<()> {
    if !p.exists() {
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent)?;
        }
        File::create(p)?;
    }
    Ok(())
}

/// Join arguments with conservative shell escaping.
pub fn shell_join(args: &[String]) -> String {
    args.iter().map(|a| shell_escape(a)).collect::<Vec<_>>().join(" ")
}

/// Escape a single shell word safely for POSIX sh.
pub fn shell_escape(s: &str) -> String {
    if s.is_empty() {
        "''".to_string()
    } else if s.chars().all(|c| c.is_ascii_alphanumeric() || "-_=./:@".contains(c)) {
        s.to_string()
    } else {
        let escaped = s.replace('\'', "'\"'\"'");
        format!("'{}'", escaped)
    }
}

/// Candidate lock file locations, ordered by preference.
pub fn candidate_lock_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(home) = home::home_dir() {
        paths.push(home.join(".aifo-coder.lock"));
    }
    if let Ok(rt) = env::var("XDG_RUNTIME_DIR") {
        if !rt.is_empty() {
            paths.push(PathBuf::from(rt).join("aifo-coder.lock"));
        }
    }
    paths.push(PathBuf::from("/tmp/aifo-coder.lock"));
    if let Ok(cwd) = env::current_dir() {
        paths.push(cwd.join(".aifo-coder.lock"));
    }
    paths
}

/// Build the docker run command for the given agent invocation, and return a preview string.
pub fn build_docker_cmd(agent: &str, passthrough: &[String], image: &str, apparmor_profile: Option<&str>) -> io::Result<(Command, String)> {
    let runtime = container_runtime_path()?;

    // TTY flags
    let tty_flags: Vec<&str> = if atty::is(atty::Stream::Stdin) || atty::is(atty::Stream::Stdout) {
        vec!["-it"]
    } else {
        vec!["-i"]
    };

    let pwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    // UID/GID mapping
    #[cfg(unix)]
    let (uid, gid) = {
        (u32::from(getuid()), u32::from(getgid()))
    };

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
        env_flags.push(OsString::from(format!("XDG_RUNTIME_DIR=/tmp/runtime-{uid}")));
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

    // Aider root-level config files
    for fname in [".aider.conf.yml", ".aider.model.metadata.json", ".aider.model.settings.yml"] {
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
    for (host_path, container_path) in [("/etc/localtime", "/etc/localtime"), ("/etc/timezone", "/etc/timezone")] {
        let hp = Path::new(host_path);
        if hp.exists() {
            volume_flags.push(OsString::from("-v"));
            volume_flags.push(OsString::from(format!("{}:{}:ro", hp.display(), container_path)));
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
    volume_flags.push(OsString::from(format!("{}:/home/coder/.gnupg-host:ro", gnupg_dir.display())));
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
            eprintln!("Warning: Docker daemon does not report AppArmor support. Continuing without AppArmor.");
        }
    }
    // Image prefix used for container naming
    let prefix = env::var("AIFO_CODER_IMAGE_PREFIX").unwrap_or_else(|_| "aifo-coder".to_string());

    // Container name/hostname
    let container_name = env::var("AIFO_CODER_CONTAINER_NAME")
        .unwrap_or_else(|_| format!("{}-{}", prefix, agent));
    let hostname = env::var("AIFO_CODER_HOSTNAME").unwrap_or_else(|_| container_name.clone());
    let name_flags = vec![OsString::from("--name"), OsString::from(&container_name), OsString::from("--hostname"), OsString::from(&hostname)];

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
            cmd.arg("--add-host").arg("host.docker.internal:host-gateway");
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

/// Acquire a non-blocking exclusive lock using default candidate lock paths.
pub fn acquire_lock() -> io::Result<File> {
    let paths = candidate_lock_paths();
    let mut last_err: Option<io::Error> = None;

    for p in paths {
        // Best effort to ensure parent exists
        if let Some(parent) = p.parent() {
            let _ = fs::create_dir_all(parent);
        }
        match OpenOptions::new().create(true).read(true).write(true).open(&p) {
            Ok(f) => {
                match f.try_lock_exclusive() {
                    Ok(_) => {
                        return Ok(f);
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                        return Err(io::Error::new(
                            io::ErrorKind::Other,
                            "Another coding agent is already running (lock held). Please try again later.",
                        ));
                    }
                    Err(e) => {
                        last_err = Some(e);
                        continue;
                    }
                }
            }
            Err(e) => {
                last_err = Some(e);
                continue;
            }
        }
    }

    let mut msg = String::from("Failed to create lock file in any candidate location: ");
    msg.push_str(
        &candidate_lock_paths()
            .into_iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", "),
    );
    if let Some(e) = last_err {
        msg.push_str(&format!(" (last error: {e})"));
    }
    Err(io::Error::new(io::ErrorKind::Other, msg))
}

/// Acquire a lock at a specific path (helper for tests).
pub fn acquire_lock_at(p: &Path) -> io::Result<File> {
    if let Some(parent) = p.parent() {
        let _ = fs::create_dir_all(parent);
    }
    match OpenOptions::new().create(true).read(true).write(true).open(p) {
        Ok(f) => {
            match f.try_lock_exclusive() {
                Ok(_) => Ok(f),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => Err(io::Error::new(
                    io::ErrorKind::Other,
                    "Another coding agent is already running (lock held). Please try again later.",
                )),
                Err(e) => Err(e),
            }
        }
        Err(e) => Err(e),
    }
}

fn create_session_id() -> String {
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

fn sidecar_container_name(kind: &str, id: &str) -> String {
    format!("aifo-tc-{kind}-{id}")
}

fn sidecar_network_name(id: &str) -> String {
    format!("aifo-net-{id}")
}

fn create_network_if_possible(runtime: &Path, name: &str, verbose: bool) {
    let mut cmd = Command::new(runtime);
    cmd.arg("network").arg("create").arg(name);
    if !verbose {
        cmd.stdout(Stdio::null()).stderr(Stdio::null());
    }
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
    let _ = cmd.status();
}

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
    let mut args: Vec<String> = vec!["docker".to_string(), "run".to_string(), "-d".to_string(), "--rm".to_string()];
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
pub fn toolchain_run(kind_in: &str, args: &[String], image_override: Option<&str>, no_cache: bool, verbose: bool, dry_run: bool) -> io::Result<i32> {
    let runtime = container_runtime_path()?;
    let pwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

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
    let session_id = create_session_id();
    let net_name = sidecar_network_name(&session_id);
    let name = sidecar_container_name(sidecar_kind.as_str(), &session_id);

    // Create network (best-effort)
    if !dry_run {
        create_network_if_possible(&runtime, &net_name, verbose);
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
        let mut run_cmd = Command::new(&runtime);
        for a in &run_preview_args[1..] {
            run_cmd.arg(a);
        }
        if !verbose {
            run_cmd.stdout(Stdio::null()).stderr(Stdio::null());
        }
        let status = run_cmd.status().map_err(|e| io::Error::new(e.kind(), format!("failed to start sidecar: {e}")))?;
        if !status.success() {
            // Cleanup network
            remove_network(&runtime, &net_name, verbose);
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("sidecar container failed to start (exit: {:?})", status.code()),
            ));
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
        let status = exec_cmd.status().map_err(|e| io::Error::new(e.kind(), format!("failed to exec in sidecar: {e}")))?;
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

fn sidecar_allowlist(kind: &str) -> &'static [&'static str] {
    match kind {
        "rust" => &["cargo", "rustc"],
        "node" => &["node", "npm", "npx", "tsc", "ts-node"],
        "python" => &["python", "python3", "pip", "pip3"],
        "c-cpp" => &["gcc", "g++", "clang", "clang++", "make", "cmake", "ninja", "pkg-config"],
        "go" => &["go", "gofmt"],
        _ => &[],
    }
}

/// Map a tool name to the sidecar kind.
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

fn url_decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let h1 = bytes[i + 1];
                let h2 = bytes[i + 2];
                let v1 = (h1 as char).to_digit(16);
                let v2 = (h2 as char).to_digit(16);
                if let (Some(a), Some(b)) = (v1, v2) {
                    out.push(((a << 4) + b) as u8 as char);
                    i += 3;
                } else {
                    out.push('%');
                    i += 1;
                }
            }
            _ => {
                out.push(bytes[i] as char);
                i += 1;
            }
        }
    }
    out
}

fn find_crlfcrlf(buf: &[u8]) -> Option<usize> {
    if buf.len() < 4 {
        return None;
    }
    let pattern: &[u8; 4] = b"\r\n\r\n";
    buf.windows(4).position(|w| w == pattern)
}

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

/// Extract outer single or double quotes if the whole string is wrapped.
fn strip_outer_quotes(s: &str) -> String {
    if s.len() >= 2 {
        let b = s.as_bytes();
        let first = b[0] as char;
        let last = b[s.len() - 1] as char;
        if (first == '\'' && last == '\'') || (first == '"' && last == '"') {
            return s[1..s.len() - 1].to_string();
        }
    }
    s.to_string()
}

/// Minimal shell-like tokenizer supporting single and double quotes.
/// Does not support escapes; quotes preserve spaces.
fn shell_like_split_args(s: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;

    for ch in s.chars() {
        match ch {
            '\'' if !in_double => {
                if in_single {
                    in_single = false;
                } else {
                    in_single = true;
                }
            }
            '"' if !in_single => {
                if in_double {
                    in_double = false;
                } else {
                    in_double = true;
                }
            }
            c if c.is_whitespace() && !in_single && !in_double => {
                if !current.is_empty() {
                    out.push(current.clone());
                    current.clear();
                }
            }
            c => current.push(c),
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

/// Parse ~/.aider.conf.yml and extract notifications-command as argv tokens.
fn parse_notifications_command_config() -> Result<Vec<String>, String> {
    // Allow tests (and power users) to override config path explicitly
    let path = if let Ok(p) = env::var("AIFO_NOTIFICATIONS_CONFIG") {
        let p = p.trim().to_string();
        if !p.is_empty() {
            PathBuf::from(p)
        } else {
            home::home_dir().ok_or_else(|| "home directory not found".to_string())?.join(".aider.conf.yml")
        }
    } else {
        home::home_dir().ok_or_else(|| "home directory not found".to_string())?.join(".aider.conf.yml")
    };
    let content = fs::read_to_string(&path)
        .map_err(|e| format!("cannot read {}: {}", path.display(), e))?;
    for line in content.lines() {
        let l = line.trim_start();
        if l.starts_with('#') || l.is_empty() {
            continue;
        }
        if let Some(rest) = l.strip_prefix("notifications-command:") {
            let mut val = rest.trim().to_string();
            // Tolerate configs/tests that append a literal "\n" at end of line
            if val.ends_with("\\n") { val.truncate(val.len() - 2); }
            if val.is_empty() {
                return Err("notifications-command is empty or multi-line values are not supported".to_string());
            }
            // Support YAML/JSON-like inline array: ["say", "--title", "AIFO"]
            if val.starts_with('[') && val.ends_with(']') {
                let inner = &val[1..val.len() - 1];
                let mut argv: Vec<String> = Vec::new();
                let mut cur = String::new();
                let mut in_single = false;
                let mut in_double = false;
                let mut esc = false;
                for ch in inner.chars() {
                    if esc {
                        // simple unescape in quoted strings
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
                        '\\' if in_double || in_single => {
                            esc = true;
                        }
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
                        ',' if !in_single && !in_double => {
                            // item separator outside quotes; ignore
                        }
                        c => {
                            // collect only when inside quotes; ignore whitespace outside
                            if in_single || in_double {
                                cur.push(c);
                            }
                        }
                    }
                }
                if !cur.is_empty() && (in_single || in_double) == false {
                    // In case of a trailing unquoted token (not expected), include it
                    argv.push(cur);
                }
                if argv.is_empty() {
                    return Err("notifications-command parsed to an empty command".to_string());
                }
                return Ok(argv);
            }
            // Fallback: treat as a single-line shell-like string
            let unquoted = strip_outer_quotes(&val);
            let argv = shell_like_split_args(&unquoted);
            if argv.is_empty() {
                return Err("notifications-command parsed to an empty command".to_string());
            }
            return Ok(argv);
        }
    }
    Err("notifications-command not found in ~/.aider.conf.yml".to_string())
}

//// Validate and, if allowed, execute the host 'say' command with provided args.
/// Returns (exit_code, output_bytes) on success, or Err(reason) if rejected.
fn notifications_handle_request(argv: &[String], _verbose: bool, timeout_secs: u64) -> Result<(i32, Vec<u8>), String> {
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
pub fn toolchain_write_shims(dir: &Path) -> io::Result<()> {
    let tools = [
        "cargo","rustc","node","npm","npx","tsc","ts-node","python","pip","pip3",
        "gcc","g++","clang","clang++","make","cmake","ninja","pkg-config","go","gofmt","notifications-cmd",
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
        fs::write(&path, format!("#!/bin/sh\nexec \"$(dirname \"$0\")/aifo-shim\" \"$@\"\n"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
        }
    }
    Ok(())
}

/// Start sidecar session for requested kinds; returns the session id.
pub fn toolchain_start_session(kinds: &[String], overrides: &[(String, String)], no_cache: bool, verbose: bool) -> io::Result<String> {
    let runtime = container_runtime_path()?;
    let pwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    #[cfg(unix)]
    let uid: u32 = u32::from(getuid());
    #[cfg(unix)]
    let gid: u32 = u32::from(getgid());
    #[cfg(not(unix))]
    let (_uid, _gid) = (0u32, 0u32);

    let session_id = create_session_id();
    let net_name = sidecar_network_name(&session_id);
    create_network_if_possible(&runtime, &net_name, verbose);

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
        let mut run_cmd = Command::new(&runtime);
        for a in &args[1..] {
            run_cmd.arg(a);
        }
        if !verbose {
            run_cmd.stdout(Stdio::null()).stderr(Stdio::null());
        }
        let st = run_cmd.status().map_err(|e| io::Error::new(e.kind(), format!("failed to start sidecar: {e}")))?;
        if !st.success() {
            remove_network(&runtime, &net_name, verbose);
            return Err(io::Error::new(io::ErrorKind::Other, "failed to start one or more sidecars"));
        }
    }
    Ok(session_id)
}

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
pub fn toolexec_start_proxy(session_id: &str, verbose: bool) -> io::Result<(String, String, std::sync::Arc<std::sync::atomic::AtomicBool>, std::thread::JoinHandle<()>)> {
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
                                if let Some(pos) = find_crlfcrlf(&buf) {
                                    header_end = Some(pos + 4);
                                }
                                if buf.len() > 64 * 1024 {
                                    break;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                    let Some(hend) = header_end else { continue };
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
                                let v = v.trim();
                                if v == format!("Bearer {}", token_for_thread2) {
                                    auth_ok = true;
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
                    if !auth_ok {
                        let _ = stream.write_all(b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\n\r\n");
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
                        let _ = stream.write_all(b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n");
                        continue;
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
                        let _ = stream.write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n");
                        continue;
                    }
                    let name = sidecar_container_name(kind, &session);
                    let pwd = PathBuf::from(cwd);
                    if verbose {
                        eprintln!("aifo-coder: proxy exec: tool={} args={:?} cwd={}", tool, argv, pwd.display());
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
                        eprintln!("aifo-coder: proxy docker: {}", shell_join(&exec_preview_args));
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
                        eprintln!("aifo-coder: proxy result tool={} kind={} code={} dur_ms={}", tool, kind, status_code, dur_ms);
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
    let bind_host: &str = if cfg!(target_os = "linux") { "0.0.0.0" } else { "127.0.0.1" };
    let listener = TcpListener::bind((bind_host, 0)).map_err(|e| io::Error::new(e.kind(), format!("proxy bind failed: {e}")))?;
    let addr = listener.local_addr().map_err(|e| io::Error::new(e.kind(), format!("proxy addr failed: {e}")))?;
    let port = addr.port();
    let _ = listener.set_nonblocking(true);
    let running_cl = running.clone();

    let handle = std::thread::spawn(move || {
        if verbose {
            eprintln!("aifo-coder: toolexec proxy listening on {}:{port}", bind_host);
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
                        if let Some(pos) = find_crlfcrlf(&buf) {
                            header_end = Some(pos + 4);
                        }
                        // avoid overly large header
                        if buf.len() > 64 * 1024 {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            let Some(hend) = header_end else { continue };
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
                        let v = v.trim();
                        if v == format!("Bearer {}", token_for_thread) {
                            auth_ok = true;
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
            if !auth_ok {
                let _ = stream.write_all(b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\n\r\n");
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
                let _ = stream.write_all(b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n");
                continue;
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
                let _ = stream.write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n");
                continue;
            }
            let name = sidecar_container_name(kind, &session);
            let pwd = PathBuf::from(cwd);
            if verbose {
                eprintln!("aifo-coder: proxy exec: tool={} args={:?} cwd={}", tool, argv, pwd.display());
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
                eprintln!("aifo-coder: proxy docker: {}", shell_join(&exec_preview_args));
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
                eprintln!("aifo-coder: proxy result tool={} kind={} code={} dur_ms={}", tool, kind, status_code, dur_ms);
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

#[cfg(test)]
mod tests {
    use super::*;
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
        assert_eq!(args, vec!["a b".to_string(), "c".to_string(), "d e".to_string()]);

        let args2 = shell_like_split_args("  a   'b c'   d  ");
        assert_eq!(args2, vec!["a".to_string(), "b c".to_string(), "d".to_string()]);
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
        assert_eq!(argv, vec!["say".to_string(), "--title".to_string(), "AIFO".to_string()]);
        // Restore AIFO_NOTIFICATIONS_CONFIG
        if let Some(v) = old_cfg { std::env::set_var("AIFO_NOTIFICATIONS_CONFIG", v); } else { std::env::remove_var("AIFO_NOTIFICATIONS_CONFIG"); }

        // Restore HOME
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
        assert_eq!(argv, vec!["say".to_string(), "--title".to_string(), "AIFO".to_string()]);
        // Restore AIFO_NOTIFICATIONS_CONFIG
        if let Some(v) = old_cfg { std::env::set_var("AIFO_NOTIFICATIONS_CONFIG", v); } else { std::env::remove_var("AIFO_NOTIFICATIONS_CONFIG"); }

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
        let has_path_prefix = args.iter().any(|s| s.contains("PATH=/workspace/.venv/bin:"));
        assert!(has_virtual_env, "exec preview missing VIRTUAL_ENV: {:?}", args);
        assert!(has_path_prefix, "exec preview missing PATH venv prefix: {:?}", args);
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
        assert!(msg.contains("only 'say' is allowed"), "unexpected error: {}", msg);

        // Restore AIFO_NOTIFICATIONS_CONFIG
        if let Some(v) = old_cfg { std::env::set_var("AIFO_NOTIFICATIONS_CONFIG", v); } else { std::env::remove_var("AIFO_NOTIFICATIONS_CONFIG"); }

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
        let has_ccache = args.iter().any(|s| s == "CCACHE_DIR=/home/coder/.cache/ccache");
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
        assert!(has_gopath && has_mod && has_cache, "exec preview missing go envs: {:?}", args);
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
        assert!(msg.contains("arguments mismatch"), "unexpected error message: {}", msg);

        // Restore env
        if let Some(v) = old_cfg { std::env::set_var("AIFO_NOTIFICATIONS_CONFIG", v); } else { std::env::remove_var("AIFO_NOTIFICATIONS_CONFIG"); }
        if let Some(v) = old_home { std::env::set_var("HOME", v); } else { std::env::remove_var("HOME"); }
    }
}
