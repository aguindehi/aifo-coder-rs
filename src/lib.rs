use std::env;
use std::ffi::OsString;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;
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
    // Common indicator: /sys/module/apparmor/parameters/enabled contains "Y", "enforce", or "complain"
    if let Ok(content) = fs::read_to_string("/sys/module/apparmor/parameters/enabled") {
        let c = content.trim().to_lowercase();
        if c.starts_with('y') || c.contains("enforce") || c.contains("complain") || c == "1" || c == "yes" || c == "true" {
            return true;
        }
    }
    // Fallback: presence of per-process AppArmor attr
    Path::new("/proc/self/attr/apparmor/current").exists()
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

/// Determine the preferred registry prefix for image references.
/// Precedence:
/// 1) If AIFO_CODER_REGISTRY_PREFIX is set:
///    - empty string forces Docker Hub (no prefix)
///    - non-empty is normalized to end with a single '/' and used as-is
/// 2) Otherwise, if repository.migros.net:443 is reachable, use "repository.migros.net/"
/// 3) Fallback: empty string (Docker Hub)
pub fn preferred_registry_prefix() -> String {
    if let Ok(pref) = env::var("AIFO_CODER_REGISTRY_PREFIX") {
        let trimmed = pref.trim();
        if trimmed.is_empty() {
            eprintln!("aifo-coder: AIFO_CODER_REGISTRY_PREFIX override set to empty; using Docker Hub (no registry prefix).");
            return String::new();
        }
        let mut s = trimmed.trim_end_matches('/').to_string();
        s.push('/');
        eprintln!("aifo-coder: Using AIFO_CODER_REGISTRY_PREFIX override: '{}'", s);
        return s;
    }

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
                return "repository.migros.net/".to_string();
            } else {
                eprintln!("aifo-coder: repository.migros.net not reachable (curl non-zero exit); using Docker Hub (no prefix).");
                return String::new();
            }
        } else {
            eprintln!("aifo-coder: curl invocation failed; falling back to TCP reachability check.");
        }
    } else {
        eprintln!("aifo-coder: curl not found; falling back to TCP reachability check.");
    }

    // Fallback quick TCP probe (short timeout).
    if is_host_port_reachable("repository.migros.net", 443, 300) {
        eprintln!("aifo-coder: repository.migros.net appears reachable via TCP; using registry prefix 'repository.migros.net/'.");
        "repository.migros.net/".to_string()
    } else {
        eprintln!("aifo-coder: repository.migros.net not reachable via TCP; using Docker Hub (no prefix).");
        String::new()
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
