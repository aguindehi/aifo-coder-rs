/*!
Sidecar lifecycle and previews.

- build_sidecar_run_preview / build_sidecar_exec_preview
- choose_session_network, ensure/remove network
- toolchain_run, toolchain_start_session/cleanup/purge/bootstrap
*/
use std::env as std_env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

#[cfg(feature = "otel")]
use tracing::instrument;

#[cfg(unix)]
use nix::unistd::{getgid, getuid};

use crate::apparmor::{desired_apparmor_profile, docker_supports_apparmor};
use crate::ToolchainError;
use crate::{container_runtime_path, shell_join, ShellScript};

use super::env::{
    apply_passthrough_envs, apply_rust_common_env, apply_rust_linker_flags_if_set, push_env,
    PROXY_ENV_NAMES,
};
use super::mounts::{
    init_node_cache_volume_if_needed, init_rust_named_volumes_if_needed, push_mount,
};
use super::{default_toolchain_image, is_official_rust_image, normalize_toolchain_kind};

pub(crate) fn sidecar_container_name(kind: &str, id: &str) -> String {
    format!("aifo-tc-{kind}-{id}")
}

pub(crate) fn sidecar_network_name(id: &str) -> String {
    format!("aifo-net-{id}")
}

#[cfg_attr(
    feature = "otel",
    instrument(
        level = "info",
        skip(runtime),
        fields(aifo_coder_network = %name, aifo_coder_verbose = %verbose)
    )
)]
pub(crate) fn ensure_network_exists(runtime: &Path, name: &str, verbose: bool) -> bool {
    let use_err = crate::color_enabled_stderr();
    // Fast path: already exists
    let exists = Command::new(runtime)
        .arg("network")
        .arg("inspect")
        .arg(name)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if exists {
        return true;
    }

    // Create the network (best-effort)
    if verbose {
        crate::log_info_stderr(
            use_err,
            &format!(
                "aifo-coder: docker: {}",
                shell_join(&[
                    "docker".to_string(),
                    "network".to_string(),
                    "create".to_string(),
                    name.to_string()
                ])
            ),
        );
    }
    let mut cmd = Command::new(runtime);
    cmd.arg("network").arg("create").arg(name);
    if !verbose {
        cmd.stdout(Stdio::null()).stderr(Stdio::null());
    }
    let _ = cmd.status();

    // Verify with brief retries to absorb races between concurrent creators
    for _ in 0..20 {
        let ok = Command::new(runtime)
            .arg("network")
            .arg("inspect")
            .arg(name)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if ok {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

#[cfg_attr(
    feature = "otel",
    instrument(
        level = "debug",
        skip(runtime),
        fields(aifo_coder_network = %name, aifo_coder_verbose = %verbose)
    )
)]
pub(crate) fn remove_network(runtime: &Path, name: &str, verbose: bool) {
    let use_err = crate::color_enabled_stderr();
    // Only attempt removal if network exists to avoid noisy errors
    let exists = Command::new(runtime)
        .arg("network")
        .arg("inspect")
        .arg(name)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !exists {
        return;
    }

    let mut cmd = Command::new(runtime);
    cmd.arg("network").arg("rm").arg(name);
    if !verbose {
        cmd.stdout(Stdio::null()).stderr(Stdio::null());
    }
    if verbose {
        crate::log_info_stderr(
            use_err,
            &format!(
                "aifo-coder: docker: {}",
                shell_join(&[
                    "docker".to_string(),
                    "network".to_string(),
                    "rm".to_string(),
                    name.to_string()
                ])
            ),
        );
    }
    let _ = cmd.status();
}

#[allow(clippy::too_many_arguments)]
pub fn build_sidecar_run_preview_with_overrides(
    name: &str,
    network: Option<&str>,
    uidgid: Option<(u32, u32)>,
    kind: &str,
    image: &str,
    no_cache: bool,
    pwd: &Path,
    overrides: &[(String, String)],
    apparmor: Option<&str>,
) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "docker".to_string(),
        "run".to_string(),
        "-d".to_string(),
        "--rm".to_string(),
    ];
    args.push("--name".to_string());
    args.push(name.to_string());
    if let Some(net) = network {
        args.push("--network".to_string());
        args.push(net.to_string());
    }
    if let Some((uid, gid)) = uidgid {
        args.push("--user".to_string());
        args.push(format!("{uid}:{gid}"));
    }
    // mounts
    push_mount(&mut args, &format!("{}:/workspace", pwd.display()));

    match kind {
        "rust" => {
            // Normative env for rust sidecar
            apply_rust_common_env(&mut args);
            // If using the official rust image or forced-official mode, align CARGO_HOME/RUSTUP_HOME to image defaults.
            if is_official_rust_image(image)
                || std_env::var("AIFO_RUST_TOOLCHAIN_USE_OFFICIAL")
                    .ok()
                    .as_deref()
                    == Some("1")
            {
                push_env(&mut args, "CARGO_HOME", "/usr/local/cargo");
                push_env(&mut args, "RUSTUP_HOME", "/usr/local/rustup");
            }
            // Cargo cache mounts
            if !no_cache {
                let force_named = cfg!(windows)
                    || std_env::var("AIFO_TOOLCHAIN_RUST_USE_DOCKER_VOLUMES")
                        .ok()
                        .as_deref()
                        == Some("1");
                if force_named {
                    // Primary mounts at normative CARGO_HOME
                    push_mount(&mut args, "aifo-cargo-registry:/home/coder/.cargo/registry");
                    push_mount(&mut args, "aifo-cargo-git:/home/coder/.cargo/git");
                    // Back-compat: also mount at legacy /usr/local/cargo paths for older tests/tools
                    push_mount(&mut args, "aifo-cargo-registry:/usr/local/cargo/registry");
                    push_mount(&mut args, "aifo-cargo-git:/usr/local/cargo/git");
                } else {
                    let mut mounted_registry = false;
                    let mut mounted_git = false;
                    let hd_opt = std_env::var("HOME")
                        .ok()
                        .filter(|s| !s.trim().is_empty())
                        .map(PathBuf::from)
                        .or_else(home::home_dir);
                    if let Some(hd) = hd_opt.clone() {
                        let reg = hd.join(".cargo").join("registry");
                        let git = hd.join(".cargo").join("git");
                        if reg.exists() {
                            // Host-preferred mount at normative CARGO_HOME (avoid duplicate mount points)
                            push_mount(
                                &mut args,
                                &format!("{}:/home/coder/.cargo/registry", reg.display()),
                            );
                            // Back-compat: also mount named volume at legacy /usr/local/cargo path (different target)
                            push_mount(&mut args, "aifo-cargo-registry:/usr/local/cargo/registry");
                            mounted_registry = true;
                        }
                        if git.exists() {
                            // Host-preferred mount at normative CARGO_HOME (avoid duplicate mount points)
                            push_mount(
                                &mut args,
                                &format!("{}:/home/coder/.cargo/git", git.display()),
                            );
                            // Back-compat: also mount named volume at legacy /usr/local/cargo path (different target)
                            push_mount(&mut args, "aifo-cargo-git:/usr/local/cargo/git");
                            mounted_git = true;
                        }
                    }
                    if !mounted_registry {
                        push_mount(&mut args, "aifo-cargo-registry:/home/coder/.cargo/registry");
                        // Back-compat legacy path for older tests/tools
                        push_mount(&mut args, "aifo-cargo-registry:/usr/local/cargo/registry");
                    }
                    if !mounted_git {
                        push_mount(&mut args, "aifo-cargo-git:/home/coder/.cargo/git");
                        // Back-compat legacy path for older tests/tools
                        push_mount(&mut args, "aifo-cargo-git:/usr/local/cargo/git");
                    }
                }
            }
            // Optional: host cargo config (read-only)
            if std_env::var("AIFO_TOOLCHAIN_RUST_USE_HOST_CONFIG")
                .ok()
                .as_deref()
                == Some("1")
            {
                let hd_opt = std_env::var("HOME")
                    .ok()
                    .filter(|s| !s.trim().is_empty())
                    .map(PathBuf::from)
                    .or_else(home::home_dir);
                if let Some(hd) = hd_opt {
                    let cargo_dir = hd.join(".cargo");
                    let cfg_toml = cargo_dir.join("config.toml");
                    let cfg = cargo_dir.join("config");
                    let src = if cfg_toml.exists() {
                        Some(cfg_toml)
                    } else if cfg.exists() {
                        Some(cfg)
                    } else {
                        None
                    };
                    if let Some(p) = src {
                        push_mount(
                            &mut args,
                            &format!("{}:/home/coder/.cargo/config.toml:ro", p.display()),
                        );
                    }
                }
            }
            // Optional: SSH agent forwarding
            if std_env::var("AIFO_TOOLCHAIN_SSH_FORWARD").ok().as_deref() == Some("1") {
                if let Ok(sock) = std_env::var("SSH_AUTH_SOCK") {
                    if !sock.trim().is_empty() {
                        push_mount(&mut args, &format!("{0}:{0}", sock));
                        push_env(&mut args, "SSH_AUTH_SOCK", &sock);
                    }
                }
            }
            // Optional: sccache
            if std_env::var("AIFO_RUST_SCCACHE").ok().as_deref() == Some("1") {
                let target = "/home/coder/.cache/sccache";
                if let Ok(dir) = std_env::var("AIFO_RUST_SCCACHE_DIR") {
                    if !dir.trim().is_empty() {
                        push_mount(&mut args, &format!("{dir}:{target}"));
                    } else {
                        push_mount(&mut args, &format!("aifo-sccache:{target}"));
                    }
                } else {
                    push_mount(&mut args, &format!("aifo-sccache:{target}"));
                }
                push_env(&mut args, "RUSTC_WRAPPER", "sccache");
                push_env(&mut args, "SCCACHE_DIR", target);
            }
            // Pass-through proxies and cargo networking envs
            apply_passthrough_envs(&mut args, PROXY_ENV_NAMES);
            // Optional: fast linkers via RUSTFLAGS (lld/mold)
            apply_rust_linker_flags_if_set(&mut args);
        }
        "node" => {
            if !no_cache {
                // Consolidated Node caches under XDG_CACHE_HOME
                push_mount(&mut args, "aifo-node-cache:/home/coder/.cache");
            }
            // Shared pnpm store inside /workspace so host/container can reuse it across installs.
            // This matches the pnpm plan: per-OS node_modules overlay, shared content-addressable store.
            let store_host = pwd.join(".pnpm-store");
            push_mount(
                &mut args,
                &format!("{}:/workspace/.pnpm-store", store_host.display()),
            );
            // Per-OS node_modules overlay: keep host and container installs isolated.
            // The orchestrator/toolchain plan expects a dedicated volume at this path.
            push_mount(&mut args, "aifo-node-modules:/workspace/node_modules");

            // Cache envs for Node ecosystem tools
            push_env(&mut args, "XDG_CACHE_HOME", "/home/coder/.cache");
            push_env(&mut args, "NPM_CONFIG_CACHE", "/home/coder/.cache/npm");
            push_env(&mut args, "YARN_CACHE_FOLDER", "/home/coder/.cache/yarn");
            // Point pnpm store to the shared repo-local store; prefer env override when present.
            let store_path = std_env::var("PNPM_STORE_PATH")
                .ok()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| "/workspace/.pnpm-store".to_string());
            push_env(&mut args, "PNPM_STORE_PATH", &store_path);
            push_env(&mut args, "PNPM_HOME", "/home/coder/.local/share/pnpm");
            push_env(&mut args, "DENO_DIR", "/home/coder/.cache/deno");
            // Ensure pnpm-managed binaries are on PATH
            push_env(
                &mut args,
                "PATH",
                "/opt/aifo/bin:/usr/local/bin:/home/coder/.local/share/pnpm/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
            );
            // Pass-through proxies for node sidecar
            apply_passthrough_envs(&mut args, PROXY_ENV_NAMES);
            // Overlay sentinel for guard logic (container-only; host must not see this file).
            push_env(
                &mut args,
                "AIFO_NODE_OVERLAY_SENTINEL",
                "/workspace/node_modules/.aifo-node-overlay",
            );
            // Best-effort overlay guard and bootstrap: if overlay is empty, seed sentinel and
            // run pnpm install using the shared /workspace/.pnpm-store. This keeps native
            // artifacts per-OS while reusing the content-addressable store.
            let sentinel = "/workspace/node_modules/.aifo-node-overlay";
            let bootstrap = ShellScript::new()
                .extend([
                    "set -e".to_string(),
                    r#"d="/workspace/node_modules""#.to_string(),
                    r#"s="/workspace/pnpm-lock.yaml""#.to_string(),
                    r#"if [ ! -d "$d" ] || [ -z "$(ls -A "$d" 2>/dev/null || true)" ]; then mkdir -p "$d"; if [ -f "$s" ]; then if command -v pnpm >/dev/null 2>&1; then echo "aifo-coder: node sidecar: bootstrapping node_modules via pnpm install --frozen-lockfile" >&2; PNPM_STORE_PATH="${PNPM_STORE_PATH:-/workspace/.pnpm-store}" pnpm install --frozen-lockfile || true; else echo "aifo-coder: warning: pnpm not found in node toolchain image; skipping automatic install" >&2; fi; fi; fi"#.to_string(),
                    format!(
                        r#"if [ ! -f "{s}" ]; then printf '%s\n' 'overlay' > "{s}" || true; fi"#,
                        s = sentinel
                    ),
                    r#"exec "$@""#.to_string(),
                ])
                .build()
                .unwrap_or_else(|_| r#"exec "$@""#.to_string());
            push_env(&mut args, "AIFO_NODE_OVERLAY_BOOTSTRAP", &bootstrap);
        }
        "python" => {
            if !no_cache {
                push_mount(&mut args, "aifo-pip-cache:/home/coder/.cache/pip");
            }
            // Pass-through proxies for python sidecar
            apply_passthrough_envs(&mut args, PROXY_ENV_NAMES);
        }
        "c-cpp" => {
            if !no_cache {
                push_mount(&mut args, "aifo-ccache:/home/coder/.cache/ccache");
            }
            push_env(&mut args, "CCACHE_DIR", "/home/coder/.cache/ccache");
            // Pass-through proxies for c/c++ sidecar
            apply_passthrough_envs(&mut args, PROXY_ENV_NAMES);
        }
        "go" => {
            if !no_cache {
                push_mount(&mut args, "aifo-go:/go");
            }
            push_env(&mut args, "GOPATH", "/go");
            push_env(&mut args, "GOMODCACHE", "/go/pkg/mod");
            push_env(&mut args, "GOCACHE", "/go/build-cache");
            // Pass-through proxies for go sidecar
            apply_passthrough_envs(&mut args, PROXY_ENV_NAMES);
        }
        _ => {
            // Pass-through proxies for other toolchains (e.g., node) during exec
            apply_passthrough_envs(&mut args, PROXY_ENV_NAMES);
        }
    }

    // base env and workdir
    push_env(&mut args, "HOME", "/home/coder");
    push_env(&mut args, "GNUPGHOME", "/home/coder/.gnupg");
    args.push("-w".to_string());
    args.push("/workspace".to_string());

    // Corporate CA bridging: detect host CA at AIFO_TEST_CORP_CA or $HOME/.certificates/MigrosRootCA2.crt
    {
        let mut host_ca: Option<PathBuf> = std_env::var("AIFO_TEST_CORP_CA")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .map(PathBuf::from)
            .filter(|p| p.exists());
        if host_ca.is_none() {
            let hd_opt = std_env::var("HOME")
                .ok()
                .filter(|s| !s.trim().is_empty())
                .map(PathBuf::from)
                .or_else(home::home_dir);
            if let Some(hd) = hd_opt {
                let p = hd.join(".certificates").join("MigrosRootCA2.crt");
                if p.exists() {
                    host_ca = Some(p);
                }
            }
        }
        if let Some(h) = host_ca {
            let target = "/etc/ssl/certs/aifo-corp-ca.crt";
            // Mount the CA into the container and set conventional TLS envs
            push_mount(&mut args, &format!("{}:{}:ro", h.display(), target));
            push_env(&mut args, "SSL_CERT_FILE", target);
            push_env(&mut args, "CURL_CA_BUNDLE", target);
            push_env(&mut args, "CARGO_HTTP_CAINFO", target);
            push_env(&mut args, "REQUESTS_CA_BUNDLE", target);
            if kind == "rust" {
                push_env(&mut args, "RUSTUP_USE_CURL", "1");
            }
        }
    }

    // Apply test/CI-provided overrides (e.g., SSL_CERT_FILE, CURL_CA_BUNDLE, etc.)
    for (k, v) in overrides {
        if !k.trim().is_empty() && !v.trim().is_empty() {
            push_env(&mut args, k, v);
        }
    }

    if let Some(profile) = apparmor {
        if docker_supports_apparmor() {
            args.push("--security-opt".to_string());
            args.push(format!("apparmor={profile}"));
        }
    }

    // Linux connectivity for sidecars (optional; typically only the agent needs host-gateway).
    // Enable via AIFO_TOOLEEXEC_ADD_HOST=1 for troubleshooting if required.
    #[cfg(target_os = "linux")]
    {
        if std::env::var("AIFO_TOOLEEXEC_ADD_HOST").ok().as_deref() == Some("1") {
            args.push("--add-host".to_string());
            args.push("host.docker.internal:host-gateway".to_string());
        }
    }

    args.push(image.to_string());
    args.push("/bin/sleep".to_string());
    args.push("infinity".to_string());
    args
}

fn node_overlay_state_and_guard(
    runtime: &Path,
    container_name: &str,
    verbose: bool,
) -> io::Result<bool> {
    let use_err = crate::color_enabled_stderr();
    let mut cmd = Command::new(runtime);
    let script = ShellScript::new()
        .extend([
            "set -e".to_string(),
            r#"wd="/workspace""#.to_string(),
            r#"nd="/workspace/node_modules""#.to_string(),
            r#"s="/workspace/node_modules/.aifo-node-overlay""#.to_string(),
            r#"if [ ! -d "$nd" ]; then echo "error:overlay-missing"; exit 0; fi"#.to_string(),
            r#"if [ -f "$s" ]; then if [ "$(stat -c '%d:%i' "$wd" 2>/dev/null || echo '?')" = "$(stat -c '%d:%i' "$nd" 2>/dev/null || echo '!')" ]; then echo "error:overlay-device-mismatch"; exit 0; fi; fi"#.to_string(),
            r#"if find "$nd" -mindepth 1 -maxdepth 1 ! -name '.*' | head -n 1 | grep -q .; then echo "nonempty"; else echo "empty"; fi"#.to_string(),
        ])
        .build()
        .unwrap_or_else(|_| "echo error:overlay-missing".to_string());

    cmd.arg("exec")
        .arg(container_name)
        .arg("sh")
        .arg("-lc")
        .arg(script)
        .stdout(Stdio::piped())
        .stderr(if verbose {
            Stdio::inherit()
        } else {
            Stdio::null()
        });
    let out = cmd.output().map_err(|e| {
        io::Error::new(
            e.kind(),
            crate::display_for_toolchain_error(&ToolchainError::Message(format!(
                "failed to inspect node_modules overlay: {e}"
            ))),
        )
    })?;
    let s = String::from_utf8_lossy(&out.stdout);
    let trimmed = s.trim();
    match trimmed {
        "empty" => Ok(true),
        "nonempty" => Ok(false),
        "error:overlay-missing" | "error:overlay-device-mismatch" => {
            let msg = "aifo-coder: error: node toolchain overlay misconfigured: \
                       /workspace/node_modules must be a dedicated container volume or tmpfs, \
                       not a bind mount of host node_modules. \
                       Please update your Docker/colima configuration to mount a volume at \
                       /workspace/node_modules.";
            crate::log_error_stderr(use_err, msg);
            Err(io::Error::other(crate::display_for_toolchain_error(
                &ToolchainError::Message(msg.to_string()),
            )))
        }
        _ => Ok(false),
    }
}

fn ensure_node_overlay_and_install(
    runtime: &Path,
    container_name: &str,
    verbose: bool,
) -> io::Result<()> {
    let use_err = crate::color_enabled_stderr();
    let mut cmd = Command::new(runtime);
    let script = ShellScript::new()
        .extend([
            "set -e".to_string(),
            r#"d="/workspace/node_modules""#.to_string(),
            r#"s="/workspace/node_modules/.aifo-node-overlay""#.to_string(),
            r#"lock="/workspace/pnpm-lock.yaml""#.to_string(),
            r#"hash_file="/workspace/node_modules/.aifo-pnpm-lock.hash""#.to_string(),
            r#"mkdir -p "$d""#.to_string(),
            r#"if [ -f "$lock" ] && command -v sha256sum >/dev/null 2>&1; then new_hash="$(sha256sum "$lock" 2>/dev/null | awk '{print $1}')"; old_hash=""; if [ -f "$hash_file" ]; then old_hash="$(cat "$hash_file" 2>/dev/null || echo '')"; fi; if [ "$new_hash" != "$old_hash" ] && command -v pnpm >/dev/null 2>&1; then echo "aifo-coder: node sidecar: pnpm-lock.yaml changed; running pnpm install --frozen-lockfile" >&2; PNPM_STORE_PATH="${PNPM_STORE_PATH:-/workspace/.pnpm-store}" pnpm install --frozen-lockfile || true; printf '%s\n' "$new_hash" >"$hash_file" 2>/dev/null || true; fi; elif [ -f "$lock" ] && command -v pnpm >/dev/null 2>&1; then echo "aifo-coder: node sidecar: ensuring node_modules via pnpm install --frozen-lockfile" >&2; PNPM_STORE_PATH="${PNPM_STORE_PATH:-/workspace/.pnpm-store}" pnpm install --frozen-lockfile || true; fi"#.to_string(),
            r#"if [ ! -f "$s" ]; then printf '%s\n' 'overlay' > "$s" || true; fi"#.to_string(),
        ])
        .build()
        .unwrap_or_else(|_| "true".to_string());

    cmd.arg("exec")
        .arg(container_name)
        .arg("sh")
        .arg("-lc")
        .arg(script)
        .stdout(if verbose {
            Stdio::inherit()
        } else {
            Stdio::null()
        })
        .stderr(if verbose {
            Stdio::inherit()
        } else {
            Stdio::null()
        });
    let status = cmd.status()?;
    if !status.success() && verbose {
        crate::log_warn_stderr(
            use_err,
            &format!(
                "aifo-coder: warning: pnpm bootstrap in node sidecar exited with status {:?}",
                status.code()
            ),
        );
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn build_sidecar_run_preview(
    name: &str,
    network: Option<&str>,
    uidgid: Option<(u32, u32)>,
    kind: &str,
    image: &str,
    no_cache: bool,
    pwd: &Path,
    apparmor: Option<&str>,
) -> Vec<String> {
    build_sidecar_run_preview_with_overrides(
        name,
        network,
        uidgid,
        kind,
        image,
        no_cache,
        pwd,
        &[],
        apparmor,
    )
}

pub fn build_sidecar_exec_preview(
    name: &str,
    uidgid: Option<(u32, u32)>,
    pwd: &Path,
    kind: &str,
    user_args: &[String],
) -> Vec<String> {
    build_sidecar_exec_preview_with_exec_id(name, uidgid, pwd, kind, user_args, None)
}

pub(crate) fn build_sidecar_exec_preview_with_exec_id(
    name: &str,
    uidgid: Option<(u32, u32)>,
    pwd: &Path,
    kind: &str,
    user_args: &[String],
    exec_id: Option<&str>,
) -> Vec<String> {
    let mut args: Vec<String> = vec!["docker".to_string(), "exec".to_string()];
    if let Some((uid, gid)) = uidgid {
        args.push("-u".to_string());
        args.push(format!("{uid}:{gid}"));
    }
    args.push("-w".to_string());
    args.push("/workspace".to_string());
    // base env
    push_env(&mut args, "HOME", "/home/coder");
    push_env(&mut args, "GNUPGHOME", "/home/coder/.gnupg");
    if let Some(eid) = exec_id {
        if !eid.trim().is_empty() {
            push_env(&mut args, "AIFO_EXEC_ID", eid);
        }
    }

    // Phase 2 marking: when executing with an official rust image, mark for bootstrap (Phase 4 will consume this)
    if kind == "rust"
        && std::env::var("AIFO_RUST_OFFICIAL_BOOTSTRAP")
            .ok()
            .as_deref()
            == Some("1")
    {
        push_env(&mut args, "AIFO_RUST_OFFICIAL_BOOTSTRAP", "1");
    }

    match kind {
        "rust" => {
            apply_rust_common_env(&mut args);
            // When bootstrapping official rust images, ensure official defaults to avoid rustup installs.
            if std::env::var("AIFO_RUST_OFFICIAL_BOOTSTRAP")
                .ok()
                .as_deref()
                == Some("1")
            {
                // PATH must expose both user and system cargo bins
                push_env(
                    &mut args,
                    "PATH",
                    "/home/coder/.cargo/bin:/usr/local/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
                );
                // Use official image defaults to prevent rustup from trying to install a toolchain
                push_env(&mut args, "CARGO_HOME", "/usr/local/cargo");
                push_env(&mut args, "RUSTUP_HOME", "/usr/local/rustup");
                // Prefer curl backend and corporate CA (mounted at run-time) during exec to avoid TLS stalls
                push_env(&mut args, "RUSTUP_USE_CURL", "1");
                push_env(
                    &mut args,
                    "SSL_CERT_FILE",
                    "/etc/ssl/certs/aifo-corp-ca.crt",
                );
                push_env(
                    &mut args,
                    "CURL_CA_BUNDLE",
                    "/etc/ssl/certs/aifo-corp-ca.crt",
                );
                push_env(
                    &mut args,
                    "CARGO_HTTP_CAINFO",
                    "/etc/ssl/certs/aifo-corp-ca.crt",
                );
                push_env(
                    &mut args,
                    "REQUESTS_CA_BUNDLE",
                    "/etc/ssl/certs/aifo-corp-ca.crt",
                );
            }
            // Optional: fast linkers via RUSTFLAGS (lld/mold)
            apply_rust_linker_flags_if_set(&mut args);
            // Pass-through proxies and cargo networking envs for exec as well
            apply_passthrough_envs(&mut args, PROXY_ENV_NAMES);
        }
        "node" => {
            // Ensure pnpm binaries resolve in exec even for pre-existing containers
            push_env(&mut args, "PNPM_HOME", "/home/coder/.local/share/pnpm");
            // Include $PNPM_HOME/bin explicitly in PATH to align with prebuilt toolchain image spec
            push_env(
                &mut args,
                "PATH",
                "/opt/aifo/bin:/usr/local/bin:$PNPM_HOME/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
            );
            // Pass-through proxies for node exec
            apply_passthrough_envs(&mut args, PROXY_ENV_NAMES);
        }
        "python" => {
            let venv_bin = pwd.join(".venv").join("bin");
            let venv_python = venv_bin.join("python");
            // Only use host .venv when its python looks like a Linux ELF binary.
            let mut use_host_venv = false;
            if venv_bin.exists() && venv_python.exists() {
                if let Ok(mut f) = std::fs::File::open(&venv_python) {
                    let mut magic = [0u8; 4];
                    if std::io::Read::read_exact(&mut f, &mut magic).is_ok()
                        && magic == [0x7F, b'E', b'L', b'F']
                    {
                        use_host_venv = true;
                    }
                }
            }
            if use_host_venv {
                push_env(&mut args, "VIRTUAL_ENV", "/workspace/.venv");
                push_env(
                    &mut args,
                    "PATH",
                    "/workspace/.venv/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
                );
            }
            // Pass-through proxies for python exec
            apply_passthrough_envs(&mut args, PROXY_ENV_NAMES);
        }
        "c-cpp" => {
            push_env(&mut args, "CCACHE_DIR", "/home/coder/.cache/ccache");
            // Pass-through proxies for c/c++ exec
            apply_passthrough_envs(&mut args, PROXY_ENV_NAMES);
        }
        "go" => {
            push_env(&mut args, "GOPATH", "/go");
            push_env(&mut args, "GOMODCACHE", "/go/pkg/mod");
            push_env(&mut args, "GOCACHE", "/go/build-cache");
            // Pass-through proxies for go exec
            apply_passthrough_envs(&mut args, PROXY_ENV_NAMES);
        }
        _ => {}
    }

    args.push(name.to_string());
    // user command (bootstrap on official rust images)
    let use_bootstrap = kind == "rust"
        && std::env::var("AIFO_RUST_OFFICIAL_BOOTSTRAP")
            .ok()
            .as_deref()
            == Some("1");
    if use_bootstrap {
        let bootstrap = ShellScript::new()
            .extend([
                r#"set -e"#.to_string(),
                r#"if [ "${AIFO_TOOLCHAIN_VERBOSE:-}" = "1" ]; then set -x; fi"#.to_string(),
                r#"mkdir -p /home/coder/.cargo/bin >/dev/null 2>&1 || true"#.to_string(),
                r#"T="${AIFO_RUST_BOOTSTRAP_TIMEOUT:-180}""#.to_string(),
                r#"NEEDS_NEXTEST=0"#.to_string(),
                r#"if [ "$#" -ge 2 ] && [ "$1" = "cargo" ] && [ "$2" = "nextest" ]; then NEEDS_NEXTEST=1; fi"#.to_string(),
                r#"if [ "$NEEDS_NEXTEST" = "1" ]; then"#.to_string(),
                r#"  cargo nextest -V >/dev/null 2>&1 || env CARGO_HOME=/home/coder/.cargo timeout "$T" cargo install cargo-nextest --locked >/dev/null 2>&1 || true"#.to_string(),
                r#"  cargo nextest -V >/dev/null 2>&1 || timeout "$T" cargo install --root /usr/local/cargo cargo-nextest --locked >/dev/null 2>&1 || true"#.to_string(),
                r#"fi"#.to_string(),
                r#"NEEDS_CLIPPY=0"#.to_string(),
                r#"if [ "$#" -ge 2 ] && [ "$1" = "cargo" ] && { [ "$2" = "clippy" ] || [ "$2" = "fmt" ] || [ "$2" = "fix" ]; }; then NEEDS_CLIPPY=1; fi"#.to_string(),
                r#"if [ "$NEEDS_CLIPPY" = "1" ]; then timeout "$T" rustup component add clippy rustfmt >/dev/null 2>&1 || true; fi"#.to_string(),
                r#"if [ "${AIFO_RUST_SCCACHE:-}" = "1" ] && ! command -v sccache >/dev/null 2>&1; then echo 'warning: sccache requested but not installed; install it inside the container or use aifo-coder-toolchain-rust image with sccache' >&2; fi"#.to_string(),
                r#"exec "$@""#.to_string(),
            ])
            .build()
            .unwrap_or_else(|_| r#"exec "$@""#.to_string());
        args.push("sh".to_string());
        args.push("-c".to_string());
        args.push(bootstrap);
        // Name for $0, subsequent args become "$@"
        args.push("aifo-exec".to_string());
        for a in user_args {
            args.push(a.clone());
        }
    } else {
        for a in user_args {
            args.push(a.clone());
        }
    }
    // include pwd to silence unused warning; it's already used for run mount
    let _ = pwd;
    args
}

/// Choose/create the session network and return its name (or None to omit --network).
pub(crate) fn choose_session_network(
    runtime: &Path,
    session_id: &str,
    verbose: bool,
    skip_creation: bool,
) -> Option<String> {
    let use_err = crate::color_enabled_stderr();
    let net_name = sidecar_network_name(session_id);
    if skip_creation {
        return Some(net_name);
    }
    if ensure_network_exists(runtime, &net_name, verbose) {
        Some(net_name)
    } else {
        if verbose {
            crate::log_warn_stderr(
                use_err,
                &format!(
                    "aifo-coder: warning: failed to create session network {}; falling back to default 'bridge' network",
                    net_name
                ),
            );
        }
        None
    }
}

/// Mark/unmark the bootstrap env for official rust images.
pub(crate) fn mark_official_rust_bootstrap(kind: &str, image: &str) {
    if kind == "rust"
        && (std_env::var("AIFO_RUST_TOOLCHAIN_USE_OFFICIAL")
            .ok()
            .as_deref()
            == Some("1")
            || is_official_rust_image(image))
    {
        std_env::set_var("AIFO_RUST_OFFICIAL_BOOTSTRAP", "1");
    } else {
        std_env::remove_var("AIFO_RUST_OFFICIAL_BOOTSTRAP");
    }
}

/// RAII guard for AIFO_RUST_OFFICIAL_BOOTSTRAP: set on create, clear on Drop.
pub struct BootstrapGuard {
    _was_set: bool,
}

impl BootstrapGuard {
    pub fn new(kind: &str, image: &str) -> Self {
        // Set marker according to rules and record whether it is set
        mark_official_rust_bootstrap(kind, image);
        let _was_set = std_env::var("AIFO_RUST_OFFICIAL_BOOTSTRAP").ok().as_deref() == Some("1");
        BootstrapGuard { _was_set }
    }
}

impl Drop for BootstrapGuard {
    fn drop(&mut self) {
        // Always clear marker best-effort
        std_env::remove_var("AIFO_RUST_OFFICIAL_BOOTSTRAP");
    }
}

#[cfg(test)]
mod bootstrap_guard_tests {
    use super::*;

    #[test]
    fn bootstrap_guard_sets_and_clears_var() {
        // Force official mode so guard sets the marker even with non-official images
        std_env::set_var("AIFO_RUST_TOOLCHAIN_USE_OFFICIAL", "1");
        // Ensure unset before
        std_env::remove_var("AIFO_RUST_OFFICIAL_BOOTSTRAP");
        {
            let _g = BootstrapGuard::new("rust", "rust:1.80-bookworm");
            let v = std_env::var("AIFO_RUST_OFFICIAL_BOOTSTRAP").ok();
            assert_eq!(v.as_deref(), Some("1"));
        }
        // After Drop, marker must be cleared
        assert!(std_env::var("AIFO_RUST_OFFICIAL_BOOTSTRAP").is_err());
        std_env::remove_var("AIFO_RUST_TOOLCHAIN_USE_OFFICIAL");
    }
}

/// Run a tool in a toolchain sidecar; returns exit code.
/// Obeys --no-toolchain-cache and image overrides; prints docker previews when verbose/dry-run.
#[cfg_attr(
    feature = "otel",
    instrument(
        level = "info",
        err,
        skip(args, image_override),
        fields(
            aifo_coder_kind = %kind_in,
            aifo_coder_no_cache = %no_cache,
            aifo_coder_verbose = %verbose,
            aifo_coder_dry_run = %dry_run
        )
    )
)]
pub fn toolchain_run(
    kind_in: &str,
    args: &[String],
    image_override: Option<&str>,
    no_cache: bool,
    verbose: bool,
    dry_run: bool,
) -> io::Result<i32> {
    let runtime: std::path::PathBuf = container_runtime_path()?;
    let use_err = crate::color_enabled_stderr();
    let pwd = {
        let p = std_env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        fs::canonicalize(&p).unwrap_or(p)
    };

    #[cfg(unix)]
    let uid: u32 = u32::from(getuid());
    #[cfg(unix)]
    let gid: u32 = u32::from(getgid());

    #[cfg(not(unix))]
    let (uid, gid) = (0u32, 0u32);

    let sidecar_kind = normalize_toolchain_kind(kind_in);
    let image = match image_override {
        Some(s) if !s.trim().is_empty() => s.to_string(),
        _ => default_toolchain_image(sidecar_kind.as_str()),
    };
    let _bootstrap_guard = BootstrapGuard::new(sidecar_kind.as_str(), &image);

    let session_id = std_env::var("AIFO_CODER_FORK_SESSION")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(super::create_session_id);
    let net_for_run = if dry_run {
        Some(sidecar_network_name(&session_id))
    } else {
        choose_session_network(&runtime, &session_id, verbose, false)
    };
    let name = sidecar_container_name(sidecar_kind.as_str(), &session_id);

    let apparmor_profile = desired_apparmor_profile();

    // Build and optionally run sidecar
    let run_preview_args = build_sidecar_run_preview_with_overrides(
        &name,
        net_for_run.as_deref(),
        if cfg!(unix) { Some((uid, gid)) } else { None },
        sidecar_kind.as_str(),
        &image,
        no_cache,
        &pwd,
        &[],
        apparmor_profile.as_deref(),
    );
    let run_preview = shell_join(&run_preview_args);

    if verbose || dry_run {
        crate::log_info_stderr(use_err, &format!("aifo-coder: docker: {}", run_preview));
    }

    if !dry_run {
        // Phase 5: initialize named cargo volumes ownership (best-effort) before starting sidecar
        if sidecar_kind == "rust" && !no_cache {
            init_rust_named_volumes_if_needed(
                &runtime,
                &image,
                &run_preview_args,
                if cfg!(unix) { Some((uid, gid)) } else { None },
                verbose,
            );
        }
        // Ensure host .pnpm-store exists and is writable for node sidecar
        if sidecar_kind == "node" {
            super::mounts::ensure_pnpm_store_host_writable(
                &pwd,
                if cfg!(unix) { Some((uid, gid)) } else { None },
                verbose,
            );
        }
        // Phase 5: initialize node cache and node_modules overlay volumes ownership (best-effort)
        if sidecar_kind == "node" && !no_cache {
            init_node_cache_volume_if_needed(
                &runtime,
                &image,
                &run_preview_args,
                if cfg!(unix) { Some((uid, gid)) } else { None },
                verbose,
            );
            super::mounts::init_node_modules_volume_if_needed(
                &runtime,
                &image,
                &run_preview_args,
                if cfg!(unix) { Some((uid, gid)) } else { None },
                verbose,
            );
        }
        // If a sidecar with this name already exists, reuse it (another pane may have started it)
        let exists = Command::new(&runtime)
            .arg("inspect")
            .arg(&name)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !exists {
            let mut run_cmd = Command::new(&runtime);
            for a in &run_preview_args[1..] {
                run_cmd.arg(a);
            }
            if !verbose {
                run_cmd.stdout(Stdio::null()).stderr(Stdio::null());
            }
            let status = run_cmd.status().map_err(|e| {
                io::Error::new(
                    e.kind(),
                    crate::display_for_toolchain_error(&ToolchainError::Message(format!(
                        "failed to start sidecar: {e}"
                    ))),
                )
            })?;
            if !status.success() {
                // Race-safe fallback: consider success if the container exists now (started by a peer)
                let mut exists_after = false;
                for _ in 0..5 {
                    exists_after = Command::new(&runtime)
                        .arg("inspect")
                        .arg(&name)
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .status()
                        .map(|s| s.success())
                        .unwrap_or(false);
                    if exists_after {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                if !exists_after {
                    return Err(io::Error::other(crate::display_for_toolchain_error(
                        &ToolchainError::Message(format!(
                            "sidecar container failed to start (exit: {:?})",
                            status.code()
                        )),
                    )));
                }
            }
            // Node overlay/bootstrap: if node sidecar was just created, ensure per-OS node_modules
            // overlay is initialized, sentinel is present, and lockfile changes trigger installs.
            if sidecar_kind == "node" {
                match node_overlay_state_and_guard(&runtime, &name, verbose) {
                    Ok(_need_install) => {
                        let _ = ensure_node_overlay_and_install(&runtime, &name, verbose);
                    }
                    Err(_) => {
                        return Err(io::Error::other(crate::display_for_toolchain_error(
                            &ToolchainError::Message(
                                "node toolchain overlay guard failed; see error above".to_string(),
                            ),
                        )));
                    }
                }
            }
        }
    }

    // docker exec
    let exec_preview_args = build_sidecar_exec_preview(
        &name,
        if cfg!(unix) { Some((uid, gid)) } else { None },
        &pwd,
        sidecar_kind.as_str(),
        args,
    );
    let exec_preview = shell_join(&exec_preview_args);

    if verbose || dry_run {
        crate::log_info_stderr(use_err, &format!("aifo-coder: docker: {}", exec_preview));
    }

    let mut exit_code: i32 = 0;

    if !dry_run {
        let _started = std::time::Instant::now();
        let mut exec_cmd = Command::new(&runtime);
        for a in &exec_preview_args[1..] {
            exec_cmd.arg(a);
        }
        let status = exec_cmd.status().map_err(|e| {
            io::Error::new(
                e.kind(),
                crate::display_for_toolchain_error(&ToolchainError::Message(format!(
                    "failed to exec in sidecar: {e}"
                ))),
            )
        })?;
        exit_code = status.code().unwrap_or(1);

        #[cfg(feature = "otel")]
        {
            use opentelemetry::trace::{Status, TraceContextExt};
            use tracing_opentelemetry::OpenTelemetrySpanExt;
            let secs = _started.elapsed().as_secs_f64();
            if exit_code != 0 {
                let cx = tracing::Span::current().context();
                cx.span()
                    .set_status(Status::error(format!("aifo_coder_exit_code={}", exit_code)));
            }
            crate::telemetry::metrics::record_docker_run_duration(kind_in, secs);
            crate::telemetry::metrics::record_docker_invocation("exec");
        }
    }
    // BootstrapGuard will clear marker on Drop

    // Cleanup: stop sidecar and remove network (best-effort)
    if !dry_run {
        let mut stop_cmd = Command::new(&runtime);
        stop_cmd.arg("stop").arg(&name);
        if !verbose {
            stop_cmd.stdout(Stdio::null()).stderr(Stdio::null());
        }
        let _ = stop_cmd.status();

        #[cfg(feature = "otel")]
        {
            crate::telemetry::metrics::record_sidecar_stopped(sidecar_kind.as_str());
        }

        if let Some(net_name) = net_for_run {
            remove_network(&runtime, &net_name, verbose);
        }
    }

    Ok(exit_code)
}

/// Start sidecar session for requested kinds; returns the session id.
/// Note: When invoked stand-alone (without ToolchainSession), callers that rely on the
/// AIFO_RUST_OFFICIAL_BOOTSTRAP marker during exec should create a BootstrapGuard
/// themselves around the session lifecycle to keep the marker set across preview+exec.
#[cfg_attr(
    feature = "otel",
    instrument(
        level = "info",
        err,
        skip(overrides),
        fields(
            aifo_coder_kinds = ?kinds,
            aifo_coder_no_cache = %no_cache,
            aifo_coder_verbose = %verbose
        )
    )
)]
pub fn toolchain_start_session(
    kinds: &[String],
    overrides: &[(String, String)],
    no_cache: bool,
    verbose: bool,
) -> io::Result<String> {
    let runtime: std::path::PathBuf = container_runtime_path()?;
    let use_err = crate::color_enabled_stderr();
    let pwd = {
        let p = std_env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        fs::canonicalize(&p).unwrap_or(p)
    };

    #[cfg(unix)]
    let uid: u32 = u32::from(getuid());
    #[cfg(unix)]
    let gid: u32 = u32::from(getgid());
    #[cfg(not(unix))]
    let (_uid, _gid) = (0u32, 0u32);

    let session_id = std_env::var("AIFO_CODER_FORK_SESSION")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(super::create_session_id);
    let net_for_run = choose_session_network(&runtime, &session_id, verbose, false);

    let apparmor_profile = desired_apparmor_profile();
    for k in kinds {
        let kind = normalize_toolchain_kind(k);
        // resolve image (override kind=image)
        let mut image = default_toolchain_image(kind.as_str());
        for (kk, vv) in overrides {
            if normalize_toolchain_kind(kk) == kind {
                image = vv.clone();
            }
        }
        // Bootstrap marker held at session level via ToolchainSession guard

        let name = sidecar_container_name(kind.as_str(), &session_id);
        let args = build_sidecar_run_preview_with_overrides(
            &name,
            net_for_run.as_deref(),
            if cfg!(unix) { Some((uid, gid)) } else { None },
            kind.as_str(),
            &image,
            no_cache,
            &pwd,
            overrides,
            apparmor_profile.as_deref(),
        );
        if verbose {
            crate::log_info_stderr(
                use_err,
                &format!("aifo-coder: docker: {}", shell_join(&args)),
            );
        }
        // Ensure host .pnpm-store exists and is writable for node session sidecar
        if kind == "node" {
            super::mounts::ensure_pnpm_store_host_writable(
                &pwd,
                if cfg!(unix) { Some((uid, gid)) } else { None },
                verbose,
            );
        }
        // Phase 5: initialize node cache and node_modules overlay volumes ownership (best-effort)
        if kind == "node" && !no_cache {
            init_node_cache_volume_if_needed(
                &runtime,
                &image,
                &args,
                if cfg!(unix) { Some((uid, gid)) } else { None },
                verbose,
            );
            super::mounts::init_node_modules_volume_if_needed(
                &runtime,
                &image,
                &args,
                if cfg!(unix) { Some((uid, gid)) } else { None },
                verbose,
            );
        }
        // If a sidecar with this name already exists, reuse it (another pane may have started it)
        let exists = Command::new(&runtime)
            .arg("inspect")
            .arg(&name)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !exists {
            let mut run_cmd = Command::new(&runtime);
            for a in &args[1..] {
                run_cmd.arg(a);
            }
            if !verbose {
                run_cmd.stdout(Stdio::null()).stderr(Stdio::null());
            }
            let st = run_cmd.status().map_err(|e| {
                io::Error::new(
                    e.kind(),
                    crate::display_for_toolchain_error(&ToolchainError::Message(format!(
                        "failed to start sidecar: {e}"
                    ))),
                )
            })?;
            if !st.success() {
                // Race-safe fallback: if the container exists now, proceed; otherwise fail
                let mut exists_after = false;
                for _ in 0..5 {
                    exists_after = Command::new(&runtime)
                        .arg("inspect")
                        .arg(&name)
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .status()
                        .map(|s| s.success())
                        .unwrap_or(false);
                    if exists_after {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                if !exists_after {
                    #[cfg(feature = "otel")]
                    {
                        use opentelemetry::trace::{Status, TraceContextExt};
                        use tracing_opentelemetry::OpenTelemetrySpanExt;
                        let cx = tracing::Span::current().context();
                        cx.span()
                            .set_status(Status::error("aifo_coder_sidecar_start_failed"));
                    }
                    return Err(io::Error::other(crate::display_for_toolchain_error(
                        &ToolchainError::Message(
                            "failed to start one or more sidecars".to_string(),
                        ),
                    )));
                }
            }
        }

        // Node overlay/bootstrap for sessions: ensure per-OS node_modules overlay and lock hash.
        if kind == "node" {
            match node_overlay_state_and_guard(&runtime, &name, verbose) {
                Ok(_need_install) => {
                    let _ = ensure_node_overlay_and_install(&runtime, &name, verbose);
                }
                Err(_) => {
                    return Err(io::Error::other(crate::display_for_toolchain_error(
                        &ToolchainError::Message(
                            "node toolchain overlay guard failed; see error above".to_string(),
                        ),
                    )));
                }
            }
        }

        #[cfg(feature = "otel")]
        {
            crate::telemetry::metrics::record_sidecar_started(kind.as_str());
        }
    }
    Ok(session_id)
}

/// Cleanup sidecars and network for a session id (best-effort).
pub fn toolchain_cleanup_session(session_id: &str, verbose: bool) {
    let runtime = match container_runtime_path() {
        Ok(p) => p,
        Err(_) => return,
    };
    let use_err = crate::color_enabled_stderr();
    let kinds = ["rust", "node", "python", "c-cpp", "go"];
    for k in kinds {
        let name = sidecar_container_name(k, session_id);
        // Only attempt stop when container exists to avoid noisy daemon errors
        let exists = Command::new(&runtime)
            .arg("inspect")
            .arg(&name)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if exists {
            if verbose {
                crate::log_info_stderr(
                    use_err,
                    &format!("aifo-coder: docker: docker stop {}", name),
                );
            }
            let _ = Command::new(&runtime)
                .arg("stop")
                .arg("--time")
                .arg("1")
                .arg(&name)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }
    let net = sidecar_network_name(session_id);
    remove_network(&runtime, &net, verbose);

    // Best-effort cleanup of unix socket directory (Linux, unix transport)
    if let Ok(dir) = std_env::var("AIFO_TOOLEEXEC_UNIX_DIR") {
        if !dir.trim().is_empty() {
            let p = PathBuf::from(dir);
            let _ = fs::remove_file(p.join("toolexec.sock"));
            let _ = fs::remove_dir_all(&p);
        }
    }
}

pub fn toolchain_purge_volume_names() -> &'static [&'static str] {
    &[
        "aifo-cargo-registry",
        "aifo-cargo-git",
        "aifo-node-cache",
        // Back-compat: legacy npm-only cache remains in purge list
        "aifo-npm-cache",
        "aifo-pip-cache",
        "aifo-ccache",
        "aifo-go",
    ]
}

/// Purge all named Docker volumes used as toolchain caches (rust, node, python, c/cpp, go).
#[cfg_attr(
    feature = "otel",
    instrument(
        level = "info",
        err,
        skip(),
        fields(aifo_coder_verbose = %verbose)
    )
)]
pub fn toolchain_purge_caches(verbose: bool) -> io::Result<()> {
    let runtime = container_runtime_path()?;
    let use_err = crate::color_enabled_stderr();
    // Phase 7: Purge caches
    // Include consolidated Node cache volume; retain legacy npm cache for back-compat cleanup.
    let volumes = toolchain_purge_volume_names();
    for v in volumes {
        if verbose {
            crate::log_info_stderr(
                use_err,
                &format!("aifo-coder: docker: docker volume rm -f {}", v),
            );
        }
        let _ = Command::new(&runtime)
            .arg("volume")
            .arg("rm")
            .arg("-f")
            .arg(v)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    Ok(())
}

/// Bootstrap: install a global typescript in the node sidecar (best-effort).
#[cfg_attr(
    feature = "otel",
    instrument(
        level = "info",
        err,
        skip(verbose),
        fields(
            aifo_coder_session_id = %session_id,
            aifo_coder_verbose = %verbose
        )
    )
)]
pub fn toolchain_bootstrap_typescript_global(session_id: &str, verbose: bool) -> io::Result<()> {
    let runtime = container_runtime_path()?;
    let use_err = crate::color_enabled_stderr();
    let name = sidecar_container_name("node", session_id);

    #[cfg(unix)]
    let uid: u32 = u32::from(getuid());
    #[cfg(unix)]
    let gid: u32 = u32::from(getgid());
    #[cfg(not(unix))]
    let (uid, gid) = (0u32, 0u32);

    let mut args: Vec<String> = vec!["docker".to_string(), "exec".to_string()];
    if cfg!(unix) {
        args.push("-u".to_string());
        args.push(format!("{uid}:{gid}"));
    }
    args.push("-w".to_string());
    args.push("/workspace".to_string());
    args.push(name);
    args.push("npm".to_string());
    args.push("install".to_string());
    args.push("-g".to_string());
    args.push("typescript".to_string());

    if verbose {
        crate::log_info_stderr(
            use_err,
            &format!("aifo-coder: docker: {}", shell_join(&args)),
        );
    }

    let mut cmd = Command::new(&runtime);
    for a in &args[1..] {
        cmd.arg(a);
    }
    if !verbose {
        cmd.stdout(Stdio::null()).stderr(Stdio::null());
    }
    let _ = cmd.status();
    Ok(())
}
