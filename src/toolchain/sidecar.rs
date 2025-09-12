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

#[cfg(unix)]
use nix::unistd::{getgid, getuid};

use crate::apparmor::{desired_apparmor_profile, docker_supports_apparmor};
use crate::{container_runtime_path, shell_join};

use super::env::{
    apply_passthrough_envs, apply_rust_common_env, apply_rust_linker_flags_if_set, push_env,
};
use super::mounts::{init_rust_named_volumes_if_needed, push_mount};
use super::{
    default_toolchain_image, is_official_rust_image, normalize_toolchain_kind, PROXY_ENV_NAMES,
};

pub(crate) fn sidecar_container_name(kind: &str, id: &str) -> String {
    format!("aifo-tc-{kind}-{id}")
}

pub(crate) fn sidecar_network_name(id: &str) -> String {
    format!("aifo-net-{id}")
}

pub(crate) fn ensure_network_exists(runtime: &Path, name: &str, verbose: bool) -> bool {
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
        eprintln!(
            "aifo-coder: docker: {}",
            shell_join(&[
                "docker".to_string(),
                "network".to_string(),
                "create".to_string(),
                name.to_string()
            ])
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

pub(crate) fn remove_network(runtime: &Path, name: &str, verbose: bool) {
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
        eprintln!(
            "aifo-coder: docker: {}",
            shell_join(&[
                "docker".to_string(),
                "network".to_string(),
                "rm".to_string(),
                name.to_string()
            ])
        );
    }
    let _ = cmd.status();
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
                push_mount(&mut args, "aifo-npm-cache:/home/coder/.npm");
            }
            // Pass-through proxies for node sidecar
            apply_passthrough_envs(&mut args, PROXY_ENV_NAMES);
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

pub fn build_sidecar_exec_preview(
    name: &str,
    uidgid: Option<(u32, u32)>,
    pwd: &Path,
    kind: &str,
    user_args: &[String],
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
            // When bootstrapping official rust images, ensure $CARGO_HOME/bin is on PATH at exec time.
            if std::env::var("AIFO_RUST_OFFICIAL_BOOTSTRAP")
                .ok()
                .as_deref()
                == Some("1")
            {
                push_env(
                    &mut args,
                    "PATH",
                    "/home/coder/.cargo/bin:/usr/local/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
                );
            }
            // Optional: fast linkers via RUSTFLAGS (lld/mold)
            apply_rust_linker_flags_if_set(&mut args);
            // Pass-through proxies and cargo networking envs for exec as well
            apply_passthrough_envs(&mut args, PROXY_ENV_NAMES);
        }
        "node" => {
            // Pass-through proxies for node exec
            apply_passthrough_envs(&mut args, PROXY_ENV_NAMES);
        }
        "python" => {
            let venv_bin = pwd.join(".venv").join("bin");
            if venv_bin.exists() {
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
        let bootstrap = "set -e; if [ \"${AIFO_TOOLCHAIN_VERBOSE:-}\" = \"1\" ]; then set -x; fi; cargo nextest -V >/dev/null 2>&1 || cargo install cargo-nextest --locked >/dev/null 2>&1 || true; rustup component list 2>/dev/null | grep -q '^clippy ' || rustup component add clippy rustfmt >/dev/null 2>&1 || true; if [ \"${AIFO_RUST_SCCACHE:-}\" = \"1\" ] && ! command -v sccache >/dev/null 2>&1; then echo 'warning: sccache requested but not installed; install it inside the container or use aifo-rust-toolchain image with sccache' >&2; fi; exec \"$@\"";
        args.push("sh".to_string());
        args.push("-c".to_string());
        args.push(bootstrap.to_string());
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
    let net_name = sidecar_network_name(session_id);
    if skip_creation {
        return Some(net_name);
    }
    if ensure_network_exists(runtime, &net_name, verbose) {
        Some(net_name)
    } else {
        if verbose {
            eprintln!(
                "aifo-coder: warning: failed to create session network {}; falling back to default 'bridge' network",
                net_name
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

/// Run a tool in a toolchain sidecar; returns exit code.
/// Obeys --no-toolchain-cache and image overrides; prints docker previews when verbose/dry-run.
pub fn toolchain_run(
    kind_in: &str,
    args: &[String],
    image_override: Option<&str>,
    no_cache: bool,
    verbose: bool,
    dry_run: bool,
) -> io::Result<i32> {
    let runtime = container_runtime_path()?;
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
    mark_official_rust_bootstrap(sidecar_kind.as_str(), &image);

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
    let run_preview_args = build_sidecar_run_preview(
        &name,
        net_for_run.as_deref(),
        if cfg!(unix) { Some((uid, gid)) } else { None },
        sidecar_kind.as_str(),
        &image,
        no_cache,
        &pwd,
        apparmor_profile.as_deref(),
    );
    let run_preview = shell_join(&run_preview_args);

    if verbose || dry_run {
        eprintln!("aifo-coder: docker: {}", run_preview);
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
            let status = run_cmd
                .status()
                .map_err(|e| io::Error::new(e.kind(), format!("failed to start sidecar: {e}")))?;
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
                    return Err(io::Error::other(format!(
                        "sidecar container failed to start (exit: {:?})",
                        status.code()
                    )));
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
        eprintln!("aifo-coder: docker: {}", exec_preview);
    }

    let mut exit_code: i32 = 0;

    if !dry_run {
        let mut exec_cmd = Command::new(&runtime);
        for a in &exec_preview_args[1..] {
            exec_cmd.arg(a);
        }
        let status = exec_cmd
            .status()
            .map_err(|e| io::Error::new(e.kind(), format!("failed to exec in sidecar: {e}")))?;
        exit_code = status.code().unwrap_or(1);
    }
    // Clear bootstrap marker from environment (best-effort)
    std_env::remove_var("AIFO_RUST_OFFICIAL_BOOTSTRAP");

    // Cleanup: stop sidecar and remove network (best-effort)
    if !dry_run {
        let mut stop_cmd = Command::new(&runtime);
        stop_cmd.arg("stop").arg(&name);
        if !verbose {
            stop_cmd.stdout(Stdio::null()).stderr(Stdio::null());
        }
        let _ = stop_cmd.status();

        if let Some(net_name) = net_for_run {
            remove_network(&runtime, &net_name, verbose);
        }
    }

    Ok(exit_code)
}

/// Start sidecar session for requested kinds; returns the session id.
pub fn toolchain_start_session(
    kinds: &[String],
    overrides: &[(String, String)],
    no_cache: bool,
    verbose: bool,
) -> io::Result<String> {
    let runtime = container_runtime_path()?;
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
        mark_official_rust_bootstrap(&kind, &image);

        let name = sidecar_container_name(kind.as_str(), &session_id);
        let args = build_sidecar_run_preview(
            &name,
            net_for_run.as_deref(),
            if cfg!(unix) { Some((uid, gid)) } else { None },
            kind.as_str(),
            &image,
            no_cache,
            &pwd,
            apparmor_profile.as_deref(),
        );
        if verbose {
            eprintln!("aifo-coder: docker: {}", shell_join(&args));
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
            let st = run_cmd
                .status()
                .map_err(|e| io::Error::new(e.kind(), format!("failed to start sidecar: {e}")))?;
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
                    return Err(io::Error::other("failed to start one or more sidecars"));
                }
            }
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
                eprintln!("aifo-coder: docker: docker stop {}", name);
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

/// Purge all named Docker volumes used as toolchain caches (rust, node, python, c/cpp, go).
pub fn toolchain_purge_caches(verbose: bool) -> io::Result<()> {
    let runtime = container_runtime_path()?;
    let volumes = [
        "aifo-cargo-registry",
        "aifo-cargo-git",
        "aifo-npm-cache",
        "aifo-pip-cache",
        "aifo-ccache",
        "aifo-go",
    ];
    for v in volumes {
        if verbose {
            eprintln!("aifo-coder: docker: docker volume rm -f {}", v);
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
pub fn toolchain_bootstrap_typescript_global(session_id: &str, verbose: bool) -> io::Result<()> {
    let runtime = container_runtime_path()?;
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
        eprintln!("aifo-coder: docker: {}", shell_join(&args));
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
