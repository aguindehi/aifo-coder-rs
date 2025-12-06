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

#[cfg(feature = "otel")]
use tracing::instrument;

// Mirror registry: in-process cache and source
static MIRROR_REGISTRY_PREFIX_CACHE: OnceCell<String> = OnceCell::new();
static MIRROR_REGISTRY_SOURCE: OnceCell<String> = OnceCell::new();
// Internal registry (env-only): in-process cache and source
static INTERNAL_REGISTRY_PREFIX_CACHE: OnceCell<String> = OnceCell::new();
static INTERNAL_REGISTRY_SOURCE: OnceCell<String> = OnceCell::new();

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
    Some(base.join("aifo-coder.mirrorprefix"))
}

fn write_registry_cache_disk(s: &str) {
    if let Some(path) = registry_cache_path() {
        let _ = fs::write(path, s);
    }
}

/// Attempt to read the mirror registry prefix from on-disk cache and normalize it.
/// Normalization: return "" for empty/whitespace; otherwise ensure a single trailing '/'.
fn read_mirror_cache_disk_normalized() -> Option<String> {
    let raw = registry_cache_path().and_then(|p| fs::read_to_string(p).ok())?;
    let t = raw.trim();
    if t.is_empty() {
        Some(String::new())
    } else {
        let mut s = t.trim_end_matches('/').to_string();
        s.push('/');
        Some(s)
    }
}

/// Public helper to invalidate the on-disk registry cache before probing.
/// Does not affect the in-process OnceCell cache for this run.
pub fn invalidate_registry_cache() {
    if let Some(path) = registry_cache_path() {
        let _ = fs::remove_file(path);
    }
}

/// Mirror registry (quiet): probe via curl then TCP; cache OnceCell + on-disk.
#[cfg_attr(
    feature = "otel",
    instrument(level = "debug", skip(), fields(aifo_coder_source = "mirror_quiet"))
)]
pub fn preferred_mirror_registry_prefix_quiet() -> String {
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
    // Env override: prefer explicit mirror registry prefix when provided
    if let Ok(val) = env::var("AIFO_CODER_MIRROR_REGISTRY_PREFIX") {
        let trimmed = val.trim();
        let v = if trimmed.is_empty() {
            String::new()
        } else {
            let mut s = trimmed.trim_end_matches('/').to_string();
            s.push('/');
            s
        };
        let _ = MIRROR_REGISTRY_PREFIX_CACHE.set(v.clone());
        let _ = MIRROR_REGISTRY_SOURCE.set("env".to_string());
        write_registry_cache_disk(&v);
        return v;
    }
    if let Some(v) = MIRROR_REGISTRY_PREFIX_CACHE.get() {
        return v.clone();
    }
    // Try on-disk cache first to avoid probe flapping across short-lived runs
    if let Some(s) = read_mirror_cache_disk_normalized() {
        let _ = MIRROR_REGISTRY_PREFIX_CACHE.set(s.clone());
        // Mark source as coming from disk cache for consistent reporting
        let _ = MIRROR_REGISTRY_SOURCE.set("cache".to_string());
        return s;
    }

    let _started = std::time::Instant::now();
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
                let _ = MIRROR_REGISTRY_PREFIX_CACHE.set(v.clone());
                let _ = MIRROR_REGISTRY_SOURCE.set("curl".to_string());
                write_registry_cache_disk(&v);

                #[cfg(feature = "otel")]
                {
                    let secs = _started.elapsed().as_secs_f64();
                    crate::telemetry::metrics::record_registry_probe_duration("curl", secs);
                }

                return v;
            } else {
                let v = String::new();
                let _ = MIRROR_REGISTRY_PREFIX_CACHE.set(v.clone());
                let _ = MIRROR_REGISTRY_SOURCE.set("curl".to_string());
                write_registry_cache_disk(&v);

                #[cfg(feature = "otel")]
                {
                    let secs = _started.elapsed().as_secs_f64();
                    crate::telemetry::metrics::record_registry_probe_duration("curl", secs);
                }

                return v;
            }
        }
    }

    let v = if is_host_port_reachable("repository.migros.net", 443, 300) {
        "repository.migros.net/".to_string()
    } else {
        String::new()
    };
    let _ = MIRROR_REGISTRY_PREFIX_CACHE.set(v.clone());
    let _ = MIRROR_REGISTRY_SOURCE.set("tcp".to_string());
    write_registry_cache_disk(&v);

    #[cfg(feature = "otel")]
    {
        let secs = _started.elapsed().as_secs_f64();
        crate::telemetry::metrics::record_registry_probe_duration("tcp", secs);
    }

    v
}

/// Mirror registry: return how it was determined ("curl", "tcp", or "unknown" for overrides/unset).
#[cfg_attr(
    feature = "otel",
    instrument(level = "debug", skip(), fields(aifo_coder_source = "mirror_source"))
)]
pub fn preferred_mirror_registry_source() -> String {
    if REGISTRY_PROBE_OVERRIDE
        .lock()
        .expect("probe override lock")
        .is_some()
    {
        return "unknown".to_string();
    }

    if let Ok(mode) = std::env::var("AIFO_CODER_TEST_REGISTRY_PROBE") {
        let ml = mode.to_ascii_lowercase();
        return match ml.as_str() {
            "curl-ok" | "curl-fail" => "curl".to_string(),
            "tcp-ok" | "tcp-fail" => "tcp".to_string(),
            _ => "unknown".to_string(),
        };
    }

    MIRROR_REGISTRY_SOURCE
        .get()
        .cloned()
        .unwrap_or_else(|| "unknown".to_string())
}

/// Internal registry (env-only; no probe, no disk cache)
#[cfg_attr(
    feature = "otel",
    instrument(level = "debug", skip(), fields(aifo_coder_source = "internal_quiet"))
)]
pub fn preferred_internal_registry_prefix_quiet() -> String {
    if let Some(v) = INTERNAL_REGISTRY_PREFIX_CACHE.get() {
        return v.clone();
    }
    match env::var("AIFO_CODER_INTERNAL_REGISTRY_PREFIX") {
        Ok(val) => {
            let trimmed = val.trim();
            if trimmed.is_empty() {
                let v = String::new();
                let _ = INTERNAL_REGISTRY_PREFIX_CACHE.set(v.clone());
                let _ = INTERNAL_REGISTRY_SOURCE.set("env-empty".to_string());
                v
            } else {
                let mut s = trimmed.trim_end_matches('/').to_string();
                s.push('/');
                let _ = INTERNAL_REGISTRY_PREFIX_CACHE.set(s.clone());
                let _ = INTERNAL_REGISTRY_SOURCE.set("env".to_string());
                s
            }
        }
        Err(_) => {
            let v = String::new();
            let _ = INTERNAL_REGISTRY_PREFIX_CACHE.set(v.clone());
            let _ = INTERNAL_REGISTRY_SOURCE.set("unset".to_string());
            v
        }
    }
}

/// Internal registry source: "env" | "env-empty" | "unset"
#[cfg_attr(
    feature = "otel",
    instrument(level = "debug", skip(), fields(aifo_coder_source = "internal_source"))
)]
pub fn preferred_internal_registry_source() -> String {
    INTERNAL_REGISTRY_SOURCE
        .get()
        .cloned()
        .unwrap_or_else(|| "unset".to_string())
}

const DEFAULT_INTERNAL_HOST: &str = "registry.intern.migros.net";
const DEFAULT_INTERNAL_NAMESPACE: &str = "ai-foundation/prototypes/aifo-coder-rs";

/// Optional namespace for our internal registry; env override or sensible default.
fn registry_namespace_opt() -> Option<String> {
    if let Ok(v) = env::var("AIFO_CODER_INTERNAL_REGISTRY_NAMESPACE") {
        let t = v.trim().trim_matches('/').to_string();
        if !t.is_empty() {
            return Some(t);
        }
    }
    Some(DEFAULT_INTERNAL_NAMESPACE.to_string())
}

/// Resolve an image reference against the configured registries.
///
/// Rules:
/// - If the reference already specifies a registry (first path component contains '.' or ':'
///   or equals "localhost"), return it unchanged.
/// - Otherwise prefer the internal registry prefix if set (env-based), else the mirror registry
///   prefix if available; both prefixes are normalized to include a trailing '/'.
fn is_our_image(image: &str) -> bool {
    // Examine the final name component without tag/digest
    let base = image.split_once('@').map(|(n, _)| n).unwrap_or(image);
    let name = base.rsplit('/').next().unwrap_or(base);
    name.starts_with("aifo-coder-") || name.starts_with("aifo-coder-toolchain-")
}

/// Probe default internal registry reachability (curl HEAD, else TCP).
fn internal_registry_reachable() -> bool {
    if which("curl").is_ok() {
        let status = Command::new("curl")
            .args([
                "--connect-timeout",
                "1",
                "--max-time",
                "2",
                "-sSI",
                &format!("https://{}/v2/", DEFAULT_INTERNAL_HOST),
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        if let Ok(st) = status {
            if st.success() {
                return true;
            }
        }
    }
    is_host_port_reachable(DEFAULT_INTERNAL_HOST, 443, 300)
}

/// Autodetect internal registry prefix:
/// - Env AIFO_CODER_INTERNAL_REGISTRY_PREFIX wins (normalized trailing '/')
/// - Else if registry.intern.migros.net reachable, compose "<host>/<namespace>/"
/// - Else empty (Docker Hub fallback)
#[cfg_attr(
    feature = "otel",
    instrument(
        level = "debug",
        skip(),
        fields(aifo_coder_kind = "internal_autodetect")
    )
)]
pub fn preferred_internal_registry_prefix_autodetect() -> String {
    if let Ok(val) = env::var("AIFO_CODER_INTERNAL_REGISTRY_PREFIX") {
        let trimmed = val.trim();
        if trimmed.is_empty() {
            return String::new();
        }
        let mut s = trimmed.trim_end_matches('/').to_string();
        s.push('/');
        return s;
    }
    if internal_registry_reachable() {
        let ns = registry_namespace_opt().unwrap_or_else(|| DEFAULT_INTERNAL_NAMESPACE.to_string());
        let mut s = format!("{}/{}/", DEFAULT_INTERNAL_HOST, ns.trim_matches('/'));
        while s.contains("//") {
            s = s.replace("//", "/");
        }
        return s;
    }
    String::new()
}

#[cfg_attr(
    feature = "otel",
    instrument(
        level = "debug",
        skip(),
        fields(aifo_coder_image = %image)
    )
)]
pub fn resolve_image(image: &str) -> String {
    // Detect if image already specifies an explicit registry
    if let Some((first, _rest)) = image.split_once('/') {
        if first.contains('.') || first.contains(':') || first == "localhost" {
            return image.to_string();
        }
    }
    let unqualified = !image.contains('/');

    // Our images: prefer internal autodetect (with namespace already in prefix), else leave unqualified
    if unqualified && is_our_image(image) {
        let internal = preferred_internal_registry_prefix_autodetect();
        if !internal.is_empty() {
            return format!("{}{}", internal, image);
        }
        return image.to_string();
    }

    // Third-party images: use mirror when reachable; do not apply internal namespace to mirror
    let mirror = preferred_mirror_registry_prefix_quiet();
    if !mirror.is_empty() {
        return format!("{}{}", mirror, image);
    }

    // No registry configured; return unchanged (Docker Hub fallback)
    image.to_string()
}

/// Retag an image by setting a new ':tag' (dropping any '@digest').
fn retag_image(image: &str, new_tag: &str) -> String {
    let base = image.split_once('@').map(|(n, _)| n).unwrap_or(image);
    let last_slash = base.rfind('/');
    let last_colon = base.rfind(':');
    let without_tag = match (last_slash, last_colon) {
        (Some(slash), Some(colon)) if colon > slash => &base[..colon],
        (None, Some(_colon)) => base.split(':').next().unwrap_or(base),
        _ => base,
    };
    format!("{}:{}", without_tag, new_tag)
}

/// Compute the effective agent image for logging: applies env overrides and registry resolution.
#[cfg_attr(
    feature = "otel",
    instrument(
        level = "debug",
        skip(),
        fields(aifo_coder_image = %image)
    )
)]
pub fn resolve_agent_image_log_display(image: &str) -> String {
    // Full image override takes precedence; used verbatim (then resolved for registry/namespace).
    if let Ok(v) = env::var("AIFO_CODER_AGENT_IMAGE") {
        let t = v.trim();
        if !t.is_empty() {
            return resolve_image(t);
        }
    }
    // Tag override: retag default then resolve via registry/namespace.
    if let Ok(tag) = env::var("AIFO_CODER_AGENT_TAG") {
        let t = tag.trim();
        if !t.is_empty() {
            let retagged = retag_image(image, t);
            return resolve_image(&retagged);
        }
    }
    resolve_image(image)
}
