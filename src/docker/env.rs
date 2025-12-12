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
