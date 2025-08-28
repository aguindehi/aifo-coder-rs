use std::env;
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use fs2::FileExt;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::net::{TcpStream, ToSocketAddrs};
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

fn read_registry_cache_disk(max_age_secs: u64) -> Option<String> {
    let path = registry_cache_path()?;
    let meta = fs::metadata(&path).ok()?;
    let modified = meta.modified().ok()?;
    let now = SystemTime::now();
    let age = now.duration_since(modified).ok()?.as_secs();
    if age <= max_age_secs {
        let v = fs::read_to_string(&path).ok()?;
        Some(v.trim().to_string())
    } else {
        None
    }
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

    // Try on-disk cache across invocations (5 minutes TTL).
    if let Some(cached) = read_registry_cache_disk(300) {
        let v = cached;
        let _ = REGISTRY_PREFIX_CACHE.set(v.clone());
        let _ = REGISTRY_PREFIX_SOURCE.set("disk".to_string());
        return v;
    }

    // Prefer probing with curl for HTTPS reachability using short timeouts.
    if let Some(cached) = read_registry_cache_disk(300) {
        let v = cached;
        let _ = REGISTRY_PREFIX_CACHE.set(v.clone());
        let _ = REGISTRY_PREFIX_SOURCE.set("disk".to_string());
        return v;
    }
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
         export PATH=\"/opt/venv/bin:$PATH\"; \
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

fn normalize_toolchain_kind(kind: &str) -> String {
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

fn sidecar_container_name(kind: &str, id: &str) -> String {
    format!("aifo-tc-{kind}-{id}")
}

fn sidecar_network_name(id: &str) -> String {
    format!("aifo-net-{id}")
}

fn create_network_if_possible(runtime: &Path, name: &str, verbose: bool) {
    let mut cmd = Command::new(runtime);
    cmd.arg("network").arg("create").arg(name);
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
    let mut cmd = Command::new(runtime);
    cmd.arg("network").arg("rm").arg(name);
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
            args.push("-v".to_string());
            args.push("aifo-cargo-registry:/usr/local/cargo/registry".to_string());
            args.push("-v".to_string());
            args.push("aifo-cargo-git:/usr/local/cargo/git".to_string());
            args.push("-e".to_string());
            args.push("CARGO_HOME=/usr/local/cargo".to_string());
        }
        "node" => {
            args.push("-v".to_string());
            args.push("aifo-npm-cache:/home/coder/.npm".to_string());
        }
        "python" => {
            args.push("-v".to_string());
            args.push("aifo-pip-cache:/home/coder/.cache/pip".to_string());
        }
        "c-cpp" => {
            args.push("-v".to_string());
            args.push("aifo-ccache:/home/coder/.cache/ccache".to_string());
            args.push("-e".to_string());
            args.push("CCACHE_DIR=/home/coder/.cache/ccache".to_string());
        }
        "go" => {
            args.push("-v".to_string());
            args.push("aifo-go:/go".to_string());
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
pub fn toolchain_run(kind_in: &str, args: &[String], verbose: bool, dry_run: bool) -> io::Result<i32> {
    let runtime = container_runtime_path()?;
    let pwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    #[cfg(unix)]
    let uid: u32 = u32::from(getuid());
    #[cfg(unix)]
    let gid: u32 = u32::from(getgid());

    #[cfg(not(unix))]
    let (uid, gid) = (0u32, 0u32);

    let sidecar_kind = normalize_toolchain_kind(kind_in);
    let image = default_toolchain_image(sidecar_kind.as_str());
    let session_id = create_session_id();
    let net_name = sidecar_network_name(&session_id);
    let name = sidecar_container_name(sidecar_kind.as_str(), &session_id);

    // Create network (best-effort)
    create_network_if_possible(&runtime, &net_name, verbose);

    let apparmor_profile = desired_apparmor_profile();

    // Build and optionally run sidecar
    let run_preview_args = build_sidecar_run_preview(
        &name,
        Some(&net_name),
        if cfg!(unix) { Some((uid, gid)) } else { None },
        sidecar_kind.as_str(),
        &image,
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
        let _ = stop_cmd.status();
    }
    remove_network(&runtime, &net_name, verbose);

    Ok(exit_code)
}
