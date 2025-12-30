//! Proxy environment handling and fallback policy.
use once_cell::sync::Lazy;
use std::env;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

// Proxy environment variable names we track.
pub(crate) const PROXY_ENV_VARS: &[&str] =
    &["http_proxy", "https_proxy", "HTTP_PROXY", "HTTPS_PROXY"];

static FORCE_DIRECT_PROXY: AtomicBool = AtomicBool::new(false);
static RECORDED_PROXY_VARS: Lazy<Mutex<Vec<String>>> =
    Lazy::new(|| Mutex::new(Vec::with_capacity(4)));

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
    PROXY_ENV_VARS
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

pub fn reset_proxy_state_for_tests() {
    FORCE_DIRECT_PROXY.store(false, Ordering::Relaxed);
    if let Ok(mut guard) = RECORDED_PROXY_VARS.lock() {
        guard.clear();
    }
}
