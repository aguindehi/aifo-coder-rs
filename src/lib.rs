use std::env;
use std::ffi::OsString;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use which::which;

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

/// Probe whether the Docker daemon reports AppArmor support.
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
    s.contains("apparmor")
}

/// Choose the AppArmor profile to use, if any.
/// - If Docker supports AppArmor, prefer an explicit override via AIFO_CODER_APPARMOR_PROFILE.
/// - On macOS/Windows hosts (Docker-in-VM), default to docker-default to avoid requiring a host-installed custom profile.
/// - On native Linux hosts, default to the custom "aifo-coder" profile.
pub fn desired_apparmor_profile() -> Option<String> {
    if !docker_supports_apparmor() {
        return None;
    }
    if let Ok(p) = env::var("AIFO_CODER_APPARMOR_PROFILE") {
        if !p.trim().is_empty() {
            return Some(p);
        }
    }
    if cfg!(target_os = "macos") || cfg!(target_os = "windows") {
        Some("docker-default".to_string())
    } else {
        Some("aifo-coder".to_string())
    }
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
