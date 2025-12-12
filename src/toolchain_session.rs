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

#[cfg(feature = "otel")]
use tracing::instrument;

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

/// Return true if a node-family toolchain is requested via --toolchain/--toolchain-spec.
pub(crate) fn node_toolchain_requested(cli: &Cli) -> bool {
    // Direct toolchain flag (matches ToolchainKind::Node)
    if cli.toolchain.iter().any(|k| k.as_str() == "node") {
        return true;
    }

    // Specs: node@XX, ts@YY, bun@ZZ (ToolchainKind::Node has aliases)
    for s in &cli.toolchain_spec {
        let t = s.trim();
        let kind = if let Some((k, _v)) = t.split_once('@') {
            k.trim()
        } else {
            t
        };
        let lower = kind.to_ascii_lowercase();
        if lower == "node" || lower == "ts" || lower == "bun" {
            return true;
        }
    }
    false
}

// One-shot npm/yarn → pnpm migration helper integrated into node toolchain startup.
fn maybe_migrate_node_to_pnpm_interactive() {
    use std::io::{self, Write};

    // Do not run in CI or when non-interactive mode is requested
    if std::env::var("CI").ok().as_deref() == Some("true")
        || std::env::var("AIFO_CODER_NON_INTERACTIVE").ok().as_deref() == Some("1")
    {
        return;
    }

    // Require pnpm to be available
    let pnpm_ok = std::process::Command::new("pnpm")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !pnpm_ok {
        return;
    }

    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(_) => return,
    };

    // Detect lockfiles and package manifest
    let has_pnpm_lock = cwd.join("pnpm-lock.yaml").is_file();
    let has_package_lock = cwd.join("package-lock.json").is_file();
    let has_yarn_lock = cwd.join("yarn.lock").is_file();
    let has_package_json = cwd.join("package.json").is_file();

    // Mode A: pnpm-first repo with legacy npm/yarn lockfiles to clean up
    if has_pnpm_lock {
        if !has_package_lock && !has_yarn_lock {
            // Nothing to migrate: pnpm-lock.yaml is already the source of truth and there are
            // no legacy npm/yarn lockfiles to clean up.
            return;
        }
    } else {
        // Mode B: clearly npm-based repo (package.json + package-lock.json, no pnpm lock yet)
        if !(has_package_json && has_package_lock && !has_pnpm_lock) {
            // Neither pnpm-first-with-legacy-locks nor npm-only with package-lock.json:
            // do not offer migration.
            return;
        }
    }

    let use_err = aifo_coder::color_enabled_stderr();
    let mut out = io::stderr();

    // Explain what will happen (mirrors Makefile semantics), but report actual artifacts found.
    let has_node_modules = cwd.join("node_modules").is_dir();
    let mut artifacts: Vec<&str> = Vec::new();
    if has_node_modules {
        artifacts.push("node_modules/");
    }
    if has_package_lock {
        artifacts.push("package-lock.json");
    }
    if has_yarn_lock {
        artifacts.push("yarn.lock");
    }
    let detected_line = if artifacts.is_empty() {
        "aifo-coder: detected npm/yarn artifacts.\n".to_string()
    } else {
        format!(
            "aifo-coder: detected npm/yarn artifacts: {}.\n",
            artifacts.join(", ")
        )
    };

    let msg = format!(
        "{detected}aifo-coder: this repository is pnpm-first.\n\
aifo-coder: we can migrate your project to pnpm by:\n\
  - Removing node_modules/\n\
  - Removing package-lock.json and yarn.lock (if present)\n\
  - Creating .pnpm-store/ with group-writable permissions\n\
  - Running 'pnpm install --frozen-lockfile'\n\n\
Do you want to perform this one-shot migration now? [y/N] ",
        detected = detected_line
    );
    let painted = aifo_coder::paint(use_err, "\x1b[33m", &msg);
    let _ = write!(out, "{}", painted);
    let _ = out.flush();

    let mut answer = String::new();
    if io::stdin().read_line(&mut answer).is_err() {
        return;
    }
    let ans = answer.trim();
    if ans != "y" && ans != "Y" {
        let _ = writeln!(
            out,
            "{}",
            aifo_coder::paint(
                use_err,
                "\x1b[33m",
                "aifo-coder: skipping pnpm migration; continuing with existing layout."
            )
        );
        let _ = out.flush();
        return;
    }

    // Inform user that migration is starting and may take a while
    let _ = writeln!(
        out,
        "{}",
        aifo_coder::paint(
            use_err,
            "\x1b[33m",
            "aifo-coder: migration start (do not interrupt) ..."
        )
    );
    let _ = out.flush();

    // Perform migration (mirrors Makefile node-migrate-to-pnpm)
    if has_node_modules {
        let _ = writeln!(
            out,
            "{}",
            aifo_coder::paint(
                use_err,
                "\x1b[33m",
                "aifo-coder: removing node_modules/ ..."
            )
        );
        let _ = out.flush();
        let _ = std::fs::remove_dir_all(cwd.join("node_modules"));
    }
    if has_package_lock {
        let _ = writeln!(
            out,
            "{}",
            aifo_coder::paint(
                use_err,
                "\x1b[33m",
                "aifo-coder: removing package-lock.json ..."
            )
        );
        let _ = out.flush();
        let _ = std::fs::remove_file(cwd.join("package-lock.json"));
    }
    if has_yarn_lock {
        let _ = writeln!(
            out,
            "{}",
            aifo_coder::paint(use_err, "\x1b[33m", "aifo-coder: removing yarn.lock ...")
        );
        let _ = out.flush();
        let _ = std::fs::remove_file(cwd.join("yarn.lock"));
    }

    let pnpm_store = cwd.join(".pnpm-store");
    if !pnpm_store.is_dir() {
        let _ = writeln!(
            out,
            "{}",
            aifo_coder::paint(
                use_err,
                "\x1b[33m",
                "aifo-coder: creating .pnpm-store with group-writable permissions ..."
            )
        );
        let _ = out.flush();
        if let Err(e) = std::fs::create_dir_all(&pnpm_store) {
            let _ = writeln!(
                out,
                "{}",
                aifo_coder::paint(
                    use_err,
                    "\x1b[31m",
                    &format!(
                        "aifo-coder: warning: failed to create .pnpm-store directory: {}",
                        e
                    )
                )
            );
            let _ = out.flush();
            return;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&pnpm_store, std::fs::Permissions::from_mode(0o775));
        }
    }

    let msg_run = "aifo-coder: running 'pnpm install --frozen-lockfile' using .pnpm-store/ ...";
    let _ = writeln!(out, "{}", aifo_coder::paint(use_err, "\x1b[32m", msg_run));
    let _ = out.flush();

    let store_path = pnpm_store
        .as_path()
        .to_str()
        .unwrap_or(".pnpm-store")
        .to_string();

    let status = std::process::Command::new("pnpm")
        .arg("install")
        .arg("--frozen-lockfile")
        .env("PNPM_STORE_PATH", &store_path)
        .status();

    match status {
        Ok(s) if s.success() => {
            let _ = writeln!(
                out,
                "{}",
                aifo_coder::paint(
                    use_err,
                    "\x1b[32m",
                    "aifo-coder: pnpm migration completed successfully."
                )
            );
            let _ = writeln!(out);
            let _ = out.flush();
        }
        Ok(s) => {
            let _ = writeln!(
                out,
                "{}",
                aifo_coder::paint(
                    use_err,
                    "\x1b[33m",
                    &format!(
                        "aifo-coder: warning: pnpm install exited with status {:?}; \
please check the output above.",
                        s.code()
                    )
                )
            );
            let _ = out.flush();
        }
        Err(e) => {
            let _ = writeln!(
                out,
                "{}",
                aifo_coder::paint(
                    use_err,
                    "\x1b[31m",
                    &format!("aifo-coder: warning: failed to run pnpm install: {}", e)
                )
            );
            let _ = out.flush();
        }
    }
}

// Best-effort: check if a toolchain image exists locally (by ref).
fn toolchain_image_exists_local(image: &str) -> bool {
    aifo_coder::container_runtime_path()
        .ok()
        .map(|rt| aifo_coder::image_exists(rt.as_path(), image))
        .unwrap_or(false)
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
    #[cfg_attr(
        feature = "otel",
        instrument(
            level = "info",
            err,
            skip(cli),
            fields(
                aifo_coder_toolchain_count = cli.toolchain.len(),
                aifo_coder_spec_count = cli.toolchain_spec.len(),
                aifo_coder_no_cache = %cli.no_toolchain_cache,
                aifo_coder_dry_run = %cli.dry_run,
                aifo_coder_verbose = %cli.verbose
            )
        )
    )]
    pub fn start_if_requested(cli: &Cli) -> Result<Option<Self>, io::Error> {
        if cli.toolchain.is_empty() && cli.toolchain_spec.is_empty() {
            return Ok(None);
        }
        if cli.dry_run {
            return Ok(None);
        }

        // Interactive node → pnpm migration when node toolchain is requested
        if node_toolchain_requested(cli) {
            maybe_migrate_node_to_pnpm_interactive();
        }

        // Inform about embedded shims (same text)
        if cli.verbose {
            let use_err = aifo_coder::color_enabled_stderr();
            aifo_coder::log_info_stderr(
                use_err,
                "aifo-coder: using embedded PATH shims from agent image (/opt/aifo/bin)",
            );
        }

        let (kinds, overrides) = plan_from_cli(cli);
        // Verbose: print chosen toolchain images per kind
        if cli.verbose {
            let use_err = aifo_coder::color_enabled_stderr();
            for k in &kinds {
                let img = overrides
                    .iter()
                    .find(|(kk, _)| kk == k)
                    .map(|(_, v)| v.clone())
                    .unwrap_or_else(|| aifo_coder::default_toolchain_image(k));
                aifo_coder::log_info_stderr(
                    use_err,
                    &format!("aifo-coder: toolchain image [{}]: {}", k, img),
                );
            }
        } else {
            // Non-verbose: print a short notice if a toolchain image will be pulled (not present locally).
            let use_err = aifo_coder::color_enabled_stderr();
            for k in &kinds {
                let img = overrides
                    .iter()
                    .find(|(kk, _)| kk == k)
                    .map(|(_, v)| v.clone())
                    .unwrap_or_else(|| aifo_coder::default_toolchain_image(k));
                if !toolchain_image_exists_local(&img) {
                    aifo_coder::log_info_stderr(
                        use_err,
                        &format!("aifo-coder: pulling toolchain image [{}]: {}", k, img),
                    );
                }
            }
        }

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
                    let use_err = aifo_coder::color_enabled_stderr();
                    aifo_coder::log_error_stderr(
                        use_err,
                        &format!("aifo-coder: typescript bootstrap failed: {}", e),
                    );
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
        // Touch guard for clippy; RAII cleans on Drop.
        let _ = self.bootstrap_guard.as_ref();
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
