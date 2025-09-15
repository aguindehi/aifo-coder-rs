/*!
Toolchain orchestration module (v7: Phases 2–5, 8).

This module owns the toolchain sidecars, proxy, shims and notification helpers.
The crate root re-exports these symbols with `pub use toolchain::*;`.
*/

use std::time::{Duration, SystemTime};

pub(crate) use crate::create_session_id;
use crate::shell_join;

mod images;
pub use images::{
    default_toolchain_image, default_toolchain_image_for_version, is_official_rust_image,
    normalize_toolchain_kind, official_rust_image_for_version,
};

mod routing;
pub use routing::{
    container_exists, route_tool_to_sidecar, select_kind_for_tool, sidecar_allowlist,
};

mod env;
mod mounts;

mod auth;
mod http;
mod notifications;

mod sidecar;
pub use sidecar::{
    build_sidecar_exec_preview, build_sidecar_run_preview, toolchain_bootstrap_typescript_global,
    toolchain_cleanup_session, toolchain_purge_caches, toolchain_run, toolchain_start_session,
};

mod proxy;
pub use proxy::toolexec_start_proxy;

mod shim;
pub use shim::toolchain_write_shims;

fn log_parsed_request(verbose: bool, tool: &str, argv: &[String], cwd: &str) {
    if verbose {
        eprintln!(
            "\r\naifo-coder: proxy parsed tool={} argv={} cwd={}",
            tool,
            shell_join(argv),
            cwd
        );
    }
}

fn log_request_result(
    verbose: bool,
    tool: &str,
    kind: &str,
    code: i32,
    started: &std::time::Instant,
) {
    if verbose {
        eprintln!(
            "\r\n\raifo-coder: proxy result tool={} kind={} code={} dur_ms={}\r\n\r",
            tool,
            kind,
            code,
            started.elapsed().as_millis()
        );
    }
}

fn random_token() -> String {
    // Cross-platform secure RNG using getrandom
    let mut buf = [0u8; 16]; // 128-bit token
    match getrandom::getrandom(&mut buf) {
        Ok(_) => {
            let mut s = String::with_capacity(buf.len() * 2);
            for b in buf {
                use std::fmt::Write as _;
                let _ = write!(&mut s, "{:02x}", b);
            }
            s
        }
        Err(e) => {
            // Very rare fallback: deterministic-ish token with warning
            eprintln!(
                "aifo-coder: warning: secure RNG failed ({}); falling back to time^pid",
                e
            );
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
    }
}

const ERR_UNAUTHORIZED: &[u8] = b"unauthorized\n";
const ERR_FORBIDDEN: &[u8] = b"forbidden\n";
const ERR_BAD_REQUEST: &[u8] = b"bad request\n";
const ERR_METHOD_NOT_ALLOWED: &[u8] = b"method not allowed\n";
const ERR_NOT_FOUND: &[u8] = b"not found\n";
const ERR_UNSUPPORTED_PROTO: &[u8] = b"Unsupported shim protocol; expected 1 or 2\n";

// Back-compat public wrappers to preserve crate-level API for tests and callers.
pub fn parse_form_urlencoded(body: &str) -> Vec<(String, String)> {
    http::parse_form_urlencoded(body)
}

pub fn parse_notifications_command_config() -> Result<Vec<String>, String> {
    notifications::parse_notifications_command_config()
}

pub fn notifications_handle_request(
    argv: &[String],
    verbose: bool,
    timeout_secs: u64,
) -> Result<(i32, Vec<u8>), String> {
    notifications::notifications_handle_request(argv, verbose, timeout_secs)
}

/// Expose auth::authorization_value_matches for unit tests.
pub fn authorization_value_matches(v: &str, token: &str) -> bool {
    auth::authorization_value_matches(v, token)
}
