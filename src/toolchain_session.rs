//! Toolchain session RAII: start sidecars, start proxy, export env, stop on drop.
//!
//! Behavior
//! - Honors CLI flags (unix socket on Linux, no-cache, bootstrap) without changing user strings.
//! - Exports AIFO_TOOLEEXEC_URL/TOKEN for agent and shims; sets AIFO_SESSION_NETWORK.
//! - Cleans up proxy, sidecars and unix socket dir in Drop unless running inside a fork pane.

use std::io;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use crate::cli::Cli;

pub(crate) fn plan_from_cli(cli: &Cli) -> (Vec<String>, Vec<(String, String)>) {
    // Normalize requested kinds
    let mut kinds: Vec<String> = cli
        .toolchain
        .iter()
        .map(|k| k.as_str().to_string())
        .collect();

    fn parse_spec(s: &str) -> (String, Option<String>) {
        let t = s.trim();
        if let Some((k, v)) = t.split_once('@') {
            (k.trim().to_string(), Some(v.trim().to_string()))
        } else {
            (t.to_string(), None)
        }
    }

    let mut spec_versions: Vec<(String, String)> = Vec::new();
    for s in &cli.toolchain_spec {
        let (k, v) = parse_spec(s);
        if !k.is_empty() {
            kinds.push(k.clone());
            if let Some(ver) = v {
                spec_versions.push((k, ver));
            }
        }
    }
    use std::collections::BTreeSet;
    let mut set = BTreeSet::new();
    let mut kinds_norm: Vec<String> = Vec::new();
    for k in kinds {
        let norm = aifo_coder::normalize_toolchain_kind(&k);
        if set.insert(norm.clone()) {
            kinds_norm.push(norm);
        }
    }
    let kinds = kinds_norm;

    // Compute overrides (kind=image), with version-derived defaults
    let mut overrides: Vec<(String, String)> = Vec::new();
    for s in &cli.toolchain_image {
        if let Some((k, v)) = s.split_once('=') {
            if !k.trim().is_empty() && !v.trim().is_empty() {
                overrides.push((
                    aifo_coder::normalize_toolchain_kind(k),
                    v.trim().to_string(),
                ));
            }
        }
    }
    for (k, ver) in spec_versions {
        let kind = aifo_coder::normalize_toolchain_kind(&k);
        if !overrides.iter().any(|(kk, _)| kk == &kind) {
            let img = aifo_coder::default_toolchain_image_for_version(&kind, &ver);
            overrides.push((kind, img));
        }
    }

    (kinds, overrides)
}

/// RAII for toolchain sidecars + proxy. On cleanup, stops proxy and optionally sidecars.
pub struct ToolchainSession {
    sid: String,
    proxy_flag: Option<Arc<AtomicBool>>,
    proxy_handle: Option<std::thread::JoinHandle<()>>,
    verbose: bool,
    in_fork_pane: bool,
    bootstrap_guard: Option<aifo_coder::BootstrapGuard>,
}

impl ToolchainSession {
    /// Start session and proxy when toolchains requested and not in dry-run.
    /// Prints identical messages as existing main.rs paths on success/failure.
    pub fn start_if_requested(cli: &Cli) -> Result<Option<Self>, io::Error> {
        if cli.toolchain.is_empty() && cli.toolchain_spec.is_empty() {
            return Ok(None);
        }
        if cli.dry_run {
            return Ok(None);
        }

        // Inform about embedded shims (same text)
        if cli.verbose {
            eprintln!("aifo-coder: using embedded PATH shims from agent image (/opt/aifo/bin)");
        }

        let (kinds, overrides) = plan_from_cli(cli);

        // Optional unix socket (Linux)
        #[cfg(target_os = "linux")]
        if cli.toolchain_unix_socket {
            std::env::set_var("AIFO_TOOLEEXEC_USE_UNIX", "1");
        }

        // Prepare session-scoped RAII guard for official Rust bootstrap (lives until session drop)
        let session_bootstrap_guard: Option<aifo_coder::BootstrapGuard> =
            if kinds.iter().any(|k| k == "rust") {
                // Determine rust image (override or default) and create guard
                let rust_image = overrides
                    .iter()
                    .find(|(k, _)| aifo_coder::normalize_toolchain_kind(k) == "rust")
                    .map(|(_, v)| v.clone())
                    .unwrap_or_else(|| aifo_coder::default_toolchain_image("rust"));
                Some(aifo_coder::BootstrapGuard::new("rust", &rust_image))
            } else {
                None
            };

        // Start sidecars
        let sid = match aifo_coder::toolchain_start_session(
            &kinds,
            &overrides,
            cli.no_toolchain_cache,
            cli.verbose,
        ) {
            Ok(s) => s,
            Err(e) => {
                let use_err = aifo_coder::color_enabled_stderr();
                aifo_coder::log_error_stderr(
                    use_err,
                    &format!("aifo-coder: failed to start toolchain sidecars: {}", e),
                );
                return Err(e);
            }
        };

        // Export network for agent to join
        let net = format!("aifo-net-{}", sid);
        std::env::set_var("AIFO_SESSION_NETWORK", &net);
        #[cfg(target_os = "linux")]
        {
            if !cli.toolchain_unix_socket {
                std::env::set_var("AIFO_TOOLEEXEC_ADD_HOST", "1");
            }
        }

        // Bootstrap (e.g. typescript=global) before starting proxy
        if !cli.toolchain_bootstrap.is_empty() {
            let want_ts_global = cli.toolchain_bootstrap.iter().any(|b| {
                let t = b.trim().to_ascii_lowercase();
                t == "typescript=global" || t == "ts=global"
            });
            if want_ts_global && kinds.iter().any(|k| k == "node") {
                if let Err(e) = aifo_coder::toolchain_bootstrap_typescript_global(&sid, cli.verbose)
                {
                    eprintln!("aifo-coder: typescript bootstrap failed: {}", e);
                }
            }
        }

        // Start proxy
        let (url, token, flag, handle) = match aifo_coder::toolexec_start_proxy(&sid, cli.verbose) {
            Ok(t) => t,
            Err(e) => {
                let use_err = aifo_coder::color_enabled_stderr();
                aifo_coder::log_error_stderr(
                    use_err,
                    &format!("aifo-coder: failed to start toolexec proxy: {}", e),
                );
                aifo_coder::toolchain_cleanup_session(&sid, cli.verbose);
                return Err(e);
            }
        };
        // Use loopback URL on host for tests, but rewrite to host.docker.internal for agent container env
        let url_for_env = if url.starts_with("http://127.0.0.1:") {
            url.replacen("http://127.0.0.1", "http://host.docker.internal", 1)
        } else {
            url.clone()
        };
        std::env::set_var("AIFO_TOOLEEXEC_URL", &url_for_env);
        std::env::set_var("AIFO_TOOLEEXEC_TOKEN", &token);
        if cli.verbose {
            std::env::set_var("AIFO_TOOLCHAIN_VERBOSE", "1");
        }

        let in_fork_pane = std::env::var("AIFO_CODER_FORK_SESSION")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .is_some();
        Ok(Some(Self {
            sid,
            proxy_flag: Some(flag),
            proxy_handle: Some(handle),
            verbose: cli.verbose,
            in_fork_pane,
            bootstrap_guard: session_bootstrap_guard,
        }))
    }

    /// Stop proxy and sidecars unless running inside a fork pane (shared lifecycle).
    fn cleanup_inner(&mut self, verbose: bool, in_fork_pane: bool) {
        if let Some(flag) = self.proxy_flag.take() {
            flag.store(false, Ordering::SeqCst);
        }
        if let Some(h) = self.proxy_handle.take() {
            let _ = h.join();
        }
        if !in_fork_pane {
            aifo_coder::toolchain_cleanup_session(&self.sid, verbose);
        }
    }
}

impl Drop for ToolchainSession {
    fn drop(&mut self) {
        let verbose = self.verbose;
        let in_fork_pane = self.in_fork_pane;
        self.cleanup_inner(verbose, in_fork_pane);
    }
}

#[cfg(test)]
mod bootstrap_session_tests {
    #[test]
    fn test_bootstrap_marker_cleared_on_early_error_session_scope() {
        // Force official mode so guard sets the marker even with non-official images
        std::env::set_var("AIFO_RUST_TOOLCHAIN_USE_OFFICIAL", "1");
        // Ensure unset before
        std::env::remove_var("AIFO_RUST_OFFICIAL_BOOTSTRAP");

        // Simulate an early error path: guard is created and then scope exits before session completes
        {
            let _g = aifo_coder::BootstrapGuard::new("rust", "rust:1.80-bookworm");
            let v = std::env::var("AIFO_RUST_OFFICIAL_BOOTSTRAP").ok();
            assert_eq!(
                v.as_deref(),
                Some("1"),
                "bootstrap marker should be set while guard is alive"
            );
            // early return simulated by scope end (Drop runs)
        }

        // After scope exit, marker must be cleared by Drop
        assert!(
            std::env::var("AIFO_RUST_OFFICIAL_BOOTSTRAP").is_err(),
            "bootstrap marker should be cleared after early error scope ends"
        );

        // Cleanup env
        std::env::remove_var("AIFO_RUST_TOOLCHAIN_USE_OFFICIAL");
    }
}
