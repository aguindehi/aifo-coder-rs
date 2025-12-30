#![allow(clippy::module_name_repetitions)]
//! Docker `-e` argument helpers and env forwarding policy.

use once_cell::sync::Lazy;
use std::env;
use std::ffi::OsString;

// Pass-through environment variables to the containerized agent
pub(crate) static PASS_ENV_VARS: Lazy<Vec<&'static str>> = Lazy::new(|| {
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
        "AIFO_GPG_REQUIRE_PRIME",
        "AIFO_GPG_CACHE_TTL_SECONDS",
        "AIFO_GPG_CACHE_MAX_TTL_SECONDS",
        // Timezone
        "TZ",
        // Editor preferences
        "EDITOR",
        "VISUAL",
        "TERM",
    ]
});

pub(crate) fn push_env_if_set(args: &mut Vec<OsString>, key: &str) {
    if env::var(key).ok().is_some_and(|v| !v.is_empty()) {
        args.push(OsString::from("-e"));
        args.push(OsString::from(key));
    }
}

pub(crate) fn push_env_kv(args: &mut Vec<OsString>, key: &str, val: &str) {
    args.push(OsString::from("-e"));
    args.push(OsString::from(format!("{key}={val}")));
}

pub(crate) fn push_env_kv_if_set(args: &mut Vec<OsString>, key: &str) {
    if let Ok(v) = env::var(key) {
        if !v.is_empty() {
            push_env_kv(args, key, &v);
        }
    }
}

// Allow users to opt-in arbitrary environment variables via the AIFO_ENV_<NAME>=<VALUE> pattern.
// Example: AIFO_ENV_MY_KEY=foo -> container receives MY_KEY=foo.
//
// Safeguards:
// - Skip empty values
// - Skip empty suffixes after the prefix
// - Only allow ASCII alnum/underscore names to avoid surprising behavior
// - Skip a small set of reserved keys to prevent breaking core runtime assumptions
pub(crate) fn push_prefixed_env_vars(args: &mut Vec<OsString>) {
    // Keep ordering stable for deterministic test assertions
    let mut pairs: Vec<(String, String)> = env::vars()
        .filter_map(|(k, v)| {
            let stripped = k.strip_prefix("AIFO_ENV_")?;
            if stripped.is_empty() || v.is_empty() {
                return None;
            }
            let valid = stripped
                .bytes()
                .all(|b| matches!(b, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_'));
            if !valid {
                return None;
            }
            Some((stripped.to_string(), v))
        })
        .collect();

    pairs.sort_by(|a, b| a.0.cmp(&b.0));

    // Minimal reserved list to avoid clobbering critical runtime env
    const RESERVED: &[&str] = &["HOME", "USER", "SHELL", "PATH", "TERM", "PWD"];

    for (k, v) in pairs {
        if RESERVED.iter().any(|r| r == &k) {
            continue;
        }
        push_env_kv(args, &k, &v);
    }
}
