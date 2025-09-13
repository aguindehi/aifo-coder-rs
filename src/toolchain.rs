/*!
Toolchain orchestration module (v7: Phases 2–5, 8).

This module owns the toolchain sidecars, proxy, shims and notification helpers.
The crate root re-exports these symbols with `pub use toolchain::*;`.
*/

use std::collections::HashMap;
use std::env as std_env;
#[cfg(target_os = "linux")]
use std::fs;
use std::io;
use std::io::{Read, Write};
use std::net::TcpListener;
#[cfg(target_os = "linux")]
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime};

#[cfg(unix)]
use nix::unistd::{getgid, getuid};

use crate::{container_runtime_path, create_session_id, find_header_end, shell_join, url_decode};

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

/// Proxy/cargo-related environment variables to pass through to sidecars.
const PROXY_ENV_NAMES: &[&str] = &[
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "NO_PROXY",
    "http_proxy",
    "https_proxy",
    "no_proxy",
    "CARGO_NET_GIT_FETCH_WITH_CLI",
    "CARGO_REGISTRIES_CRATES_IO_PROTOCOL",
];

fn log_parsed_request(verbose: bool, tool: &str, argv: &[String], cwd: &str) {
    if verbose {
        let _ = std::io::stdout().flush();
        let _ = std::io::stderr().flush();
        eprintln!(
            "\r\x1b[2Kaifo-coder: proxy parsed: tool={} argv={:?} cwd={}",
            tool, argv, cwd
        );
        eprintln!("\r");
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
        let _ = std::io::stdout().flush();
        let _ = std::io::stderr().flush();
        eprintln!(
            "\r\x1b[2Kaifo-coder: proxy result tool={} kind={} code={} dur_ms={}",
            tool,
            kind,
            code,
            started.elapsed().as_millis()
        );
        eprintln!("\r");
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

/// Response helpers (common).
fn respond_plain<W: Write>(w: &mut W, status: &str, exit_code: i32, body: &[u8]) {
    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/plain; charset=utf-8\r\nX-Exit-Code: {exit_code}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = w.write_all(header.as_bytes());
    let _ = w.write_all(body);
    let _ = w.flush();
}

fn respond_chunked_prelude<W: Write>(w: &mut W) {
    let hdr = b"HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nTransfer-Encoding: chunked\r\nTrailer: X-Exit-Code\r\nConnection: close\r\n\r\n";
    let _ = w.write_all(hdr);
    let _ = w.flush();
}

fn respond_chunked_write_chunk<W: Write>(w: &mut W, chunk: &[u8]) {
    if !chunk.is_empty() {
        let _ = write!(w, "{:X}\r\n", chunk.len());
        let _ = w.write_all(chunk);
        let _ = w.write_all(b"\r\n");
        let _ = w.flush();
    }
}

fn respond_chunked_trailer<W: Write>(w: &mut W, code: i32) {
    let _ = w.write_all(b"0\r\n");
    let trailer = format!("X-Exit-Code: {code}\r\n\r\n");
    let _ = w.write_all(trailer.as_bytes());
    let _ = w.flush();
}

/// Build streaming docker exec spawn args: add -t and wrap with sh -c "... 2>&1".
fn build_streaming_exec_args(container_name: &str, exec_preview_args: &[String]) -> Vec<String> {
    let mut spawn_args: Vec<String> = Vec::new();
    let mut idx = None;
    for (i, a) in exec_preview_args.iter().enumerate().skip(1) {
        if a == container_name {
            idx = Some(i);
            break;
        }
    }
    let idx = idx.unwrap_or(exec_preview_args.len().saturating_sub(1));
    // Up to and including container name
    spawn_args.extend(exec_preview_args[1..=idx].iter().cloned());
    // Allocate a TTY for streaming to improve interactive flushing.
    // Set AIFO_TOOLEEXEC_TTY=0 to disable TTY allocation if it interferes with tooling.
    let use_tty = std_env::var("AIFO_TOOLEEXEC_TTY").ok().as_deref() != Some("0");
    if use_tty {
        spawn_args.insert(1, "-t".to_string());
    }
    // User command slice after container name
    let user_slice: Vec<String> = exec_preview_args[idx + 1..].to_vec();
    let script = {
        let s = shell_join(&user_slice);
        format!("{} 2>&1", s)
    };
    spawn_args.push("sh".to_string());
    spawn_args.push("-c".to_string());
    spawn_args.push(script);
    spawn_args
}

fn is_tool_allowed_any_sidecar(tool: &str) -> bool {
    let tl = tool.to_ascii_lowercase();
    ["rust", "node", "python", "c-cpp", "go"]
        .iter()
        .any(|k| sidecar_allowlist(k).contains(&tl.as_str()))
}




