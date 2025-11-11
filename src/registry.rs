#![allow(clippy::module_name_repetitions)]
//! Registry prefix resolution with optional disk cache and test overrides.
//! Complies with v2: library module uses intra-crate references (crate::) only.

use once_cell::sync::{Lazy, OnceCell};
use std::env;
use std::fs;
use std::net::{TcpStream, ToSocketAddrs};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;
use which::which;

// Cache for preferred registry prefix resolution within a single process run.
static REGISTRY_PREFIX_CACHE: OnceCell<String> = OnceCell::new();
// Record how the registry prefix was determined this run.
static REGISTRY_PREFIX_SOURCE: OnceCell<String> = OnceCell::new();

#[derive(Clone, Copy)]
pub enum RegistryProbeTestMode {
    CurlOk,
    CurlFail,
    TcpOk,
    TcpFail,
}

// Test-only override for registry probing without relying on environment variables.
static REGISTRY_PROBE_OVERRIDE: Lazy<std::sync::Mutex<Option<RegistryProbeTestMode>>> =
    Lazy::new(|| std::sync::Mutex::new(None));

pub fn registry_probe_set_override_for_tests(mode: Option<RegistryProbeTestMode>) {
    let mut guard = REGISTRY_PROBE_OVERRIDE.lock().expect("probe override lock");
    *guard = mode;
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
    let base = env::var("XDG_RUNTIME_DIR")
        .ok()
        .filter(|s| !s.is_empty())
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
    let use_err = crate::color_enabled_stderr();
    // Env override always takes precedence within the current process
    if let Ok(pref) = env::var("AIFO_CODER_REGISTRY_PREFIX") {
        let trimmed = pref.trim();
        if trimmed.is_empty() {
            crate::log_info_stderr(use_err, "aifo-coder: AIFO_CODER_REGISTRY_PREFIX override set to empty; using Docker Hub (no registry prefix).");
            let v = String::new();
            let _ = REGISTRY_PREFIX_CACHE.set(v.clone());
            let _ = REGISTRY_PREFIX_SOURCE.set("env-empty".to_string());
            write_registry_cache_disk(&v);
            return v;
        }
        let mut s = trimmed.trim_end_matches('/').to_string();
        s.push('/');
        crate::log_info_stderr(
            use_err,
            &format!(
                "aifo-coder: Using AIFO_CODER_REGISTRY_PREFIX override: '{}'",
                s
            ),
        );
        let v = s;
        let _ = REGISTRY_PREFIX_CACHE.set(v.clone());
        let _ = REGISTRY_PREFIX_SOURCE.set("env".to_string());
        write_registry_cache_disk(&v);
        return v;
    }
    if let Some(mode) = *REGISTRY_PROBE_OVERRIDE.lock().expect("probe override lock") {
        return match mode {
            RegistryProbeTestMode::CurlOk => "repository.migros.net/".to_string(),
            RegistryProbeTestMode::CurlFail => String::new(),
            RegistryProbeTestMode::TcpOk => "repository.migros.net/".to_string(),
            RegistryProbeTestMode::TcpFail => String::new(),
        };
    }

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

    if which("curl").is_ok() {
        crate::log_info_stderr(use_err, "aifo-coder: checking https://repository.migros.net/v2/ availability with: curl --connect-timeout 1 --max-time 2 -sSI ...");
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
                crate::log_info_stderr(use_err, "aifo-coder: repository.migros.net reachable; using registry prefix 'repository.migros.net/'.");
                let v = "repository.migros.net/".to_string();
                let _ = REGISTRY_PREFIX_CACHE.set(v.clone());
                let _ = REGISTRY_PREFIX_SOURCE.set("curl".to_string());
                write_registry_cache_disk(&v);
                return v;
            } else {
                crate::log_warn_stderr(use_err, "aifo-coder: repository.migros.net not reachable (curl non-zero exit); using Docker Hub (no prefix).");
                let v = String::new();
                let _ = REGISTRY_PREFIX_CACHE.set(v.clone());
                let _ = REGISTRY_PREFIX_SOURCE.set("curl".to_string());
                write_registry_cache_disk(&v);
                return v;
            }
        } else {
            crate::log_warn_stderr(
                use_err,
                "aifo-coder: curl invocation failed; falling back to TCP reachability check.",
            );
        }
    } else {
        crate::log_warn_stderr(
            use_err,
            "aifo-coder: curl not found; falling back to TCP reachability check.",
        );
    }

    let v = if is_host_port_reachable("repository.migros.net", 443, 300) {
        crate::log_info_stderr(use_err, "aifo-coder: repository.migros.net appears reachable via TCP; using registry prefix 'repository.migros.net/'.");
        "repository.migros.net/".to_string()
    } else {
        crate::log_warn_stderr(use_err, "aifo-coder: repository.migros.net not reachable via TCP; using Docker Hub (no prefix).");
        String::new()
    };
    let _ = REGISTRY_PREFIX_CACHE.set(v.clone());
    let _ = REGISTRY_PREFIX_SOURCE.set("tcp".to_string());
    write_registry_cache_disk(&v);
    v
}

/// Quiet variant for preferred registry prefix resolution without emitting any logs.
pub fn preferred_registry_prefix_quiet() -> String {
    if let Some(mode) = *REGISTRY_PROBE_OVERRIDE.lock().expect("probe override lock") {
        return match mode {
            RegistryProbeTestMode::CurlOk => "repository.migros.net/".to_string(),
            RegistryProbeTestMode::CurlFail => String::new(),
            RegistryProbeTestMode::TcpOk => "repository.migros.net/".to_string(),
            RegistryProbeTestMode::TcpFail => String::new(),
        };
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
    // If env-probe is explicitly set, report that source first.
    if let Ok(mode) = std::env::var("AIFO_CODER_TEST_REGISTRY_PROBE") {
        let ml = mode.to_ascii_lowercase();
        return match ml.as_str() {
            "curl-ok" | "curl-fail" => "curl".to_string(),
            "tcp-ok" | "tcp-fail" => "tcp".to_string(),
            _ => "unknown".to_string(),
        };
    }
    // If a source was determined during resolution (env/env-empty/curl/tcp), prefer it.
    if let Some(src) = REGISTRY_PREFIX_SOURCE.get() {
        return src.clone();
    }
    // Otherwise, if a test override is active but no resolution has occurred, report unknown.
    if REGISTRY_PROBE_OVERRIDE
        .lock()
        .expect("probe override lock")
        .is_some()
    {
        return "unknown".to_string();
    }
    "unknown".to_string()
}
