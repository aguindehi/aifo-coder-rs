//! Proxy environment handling and fallback policy.
use once_cell::sync::Lazy;
use std::env;
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use url::Url;

// Proxy environment variable names we track.
pub(crate) const PROXY_ENV_VARS: &[&str] = &[
    "http_proxy",
    "https_proxy",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "no_proxy",
    "NO_PROXY",
];
const PROXY_PROBE_ENV_VARS: &[&str] = &["http_proxy", "https_proxy", "HTTP_PROXY", "HTTPS_PROXY"];

static FORCE_DIRECT_PROXY: AtomicBool = AtomicBool::new(false);
static RECORDED_PROXY_VARS: Lazy<Mutex<Vec<String>>> =
    Lazy::new(|| Mutex::new(Vec::with_capacity(4)));
static IGNORE_LOCAL_IMAGES: AtomicBool = AtomicBool::new(false);

fn force_direct_allowed() -> bool {
    env::var("AIFO_PROXY_FORCE_PROXY").ok().as_deref() != Some("1")
}

/// Opt-out toggle for the fallback.
pub(crate) fn proxy_fallback_enabled() -> bool {
    env::var("AIFO_PROXY_FALLBACK")
        .ok()
        .as_deref()
        .unwrap_or("1")
        != "0"
}

/// Return proxy env var names that are currently set and non-empty.
pub fn proxy_env_vars_set() -> Vec<String> {
    PROXY_PROBE_ENV_VARS
        .iter()
        .filter_map(|k| {
            env::var(k)
                .ok()
                .filter(|v| !v.is_empty())
                .map(|_| k.to_string())
        })
        .collect()
}

/// Mark proxy variables as unreachable and request force-direct mode for downstream containers.
pub fn mark_proxy_unreachable(vars: &[String]) {
    if !force_direct_allowed() {
        return;
    }
    FORCE_DIRECT_PROXY.store(true, Ordering::Relaxed);
    if let Ok(mut guard) = RECORDED_PROXY_VARS.lock() {
        guard.clear();
        guard.extend(vars.iter().cloned());
    }
}

/// Should downstream containers clear proxy env (set http_proxy/https_proxy empty)?
pub fn should_force_direct_proxy() -> bool {
    force_direct_allowed() && FORCE_DIRECT_PROXY.load(Ordering::Relaxed)
}

/// Proxy variables that should be cleared when forcing direct connections.
pub(crate) fn proxy_clear_envs() -> &'static [&'static str] {
    PROXY_ENV_VARS
}

#[derive(Debug, PartialEq, Eq)]
pub enum ProxyCheckOutcome {
    Skipped,
    Retained,
    Cleared(Vec<String>),
}

fn parse_proxy_target(raw: &str) -> Option<(String, u16)> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let candidates = [trimmed.to_string(), format!("http://{trimmed}")];
    for candidate in candidates {
        let parsed = if candidate.contains("://") {
            Url::parse(&candidate).ok()
        } else {
            Url::parse(&format!("http://{candidate}")).ok()
        };
        if let Some(url) = parsed {
            if let (Some(host), Some(port)) = (url.host_str(), url.port_or_known_default()) {
                return Some((host.to_string(), port));
            }
        }
    }
    None
}

fn proxy_connectivity_check<F>(mut probe: F) -> ProxyCheckOutcome
where
    F: FnMut(&str, u16) -> bool,
{
    if !proxy_fallback_enabled() || !force_direct_allowed() {
        return ProxyCheckOutcome::Skipped;
    }
    let proxies = proxy_env_vars_set();
    if proxies.is_empty() {
        return ProxyCheckOutcome::Skipped;
    }

    let mut targets: Vec<(String, String, u16)> = Vec::new();
    for var in &proxies {
        if let Ok(val) = env::var(var) {
            if let Some((host, port)) = parse_proxy_target(&val) {
                targets.push((var.clone(), host, port));
            }
        }
    }
    if targets.is_empty() {
        return ProxyCheckOutcome::Retained;
    }

    if targets
        .iter()
        .any(|(_, host, port)| probe(host.as_str(), *port))
    {
        return ProxyCheckOutcome::Retained;
    }

    mark_proxy_unreachable(&proxies);
    for k in proxy_clear_envs() {
        env::set_var(k, "");
    }
    ProxyCheckOutcome::Cleared(proxies)
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

/// Test helper: run proxy connectivity check with a custom probe function.
pub fn proxy_connectivity_check_with<F>(probe: F) -> ProxyCheckOutcome
where
    F: FnMut(&str, u16) -> bool,
{
    proxy_connectivity_check(probe)
}

pub fn check_proxy_connectivity() -> ProxyCheckOutcome {
    proxy_connectivity_check(|host, port| is_host_port_reachable(host, port, 750))
}

pub fn reset_proxy_state_for_tests() {
    FORCE_DIRECT_PROXY.store(false, Ordering::Relaxed);
    IGNORE_LOCAL_IMAGES.store(false, Ordering::Relaxed);
    if let Ok(mut guard) = RECORDED_PROXY_VARS.lock() {
        guard.clear();
    }
}

/// Mark CLI preference to ignore local images (set via flag or env).
pub fn set_ignore_local_images(val: bool) {
    IGNORE_LOCAL_IMAGES.store(val, Ordering::Relaxed);
}

/// Whether CLI requested to ignore local images.
pub fn cli_ignore_local_images() -> bool {
    if env::var("AIFO_CODER_IGNORE_LOCAL_IMAGES").ok().as_deref() == Some("1") {
        return true;
    }
    IGNORE_LOCAL_IMAGES.load(Ordering::Relaxed)
}
