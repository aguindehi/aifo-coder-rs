/*!
Toolchain orchestration module (v7: Phases 2–5, 8).

This module owns the toolchain sidecars, proxy, shims and notification helpers.
The crate root re-exports these symbols with `pub use toolchain::*;`.
*/

use std::collections::HashMap;
use std::env;
use std::fs;
use std::io;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener};
#[cfg(target_os = "linux")]
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime};

#[cfg(unix)]
use nix::unistd::{getgid, getuid};

use crate::apparmor::{desired_apparmor_profile, docker_supports_apparmor};
use crate::{
    container_runtime_path, create_session_id, find_header_end, shell_join, shell_like_split_args,
    strip_outer_quotes, url_decode,
};

// Normalize toolchain kind names to canonical identifiers
pub fn normalize_toolchain_kind(kind: &str) -> String {
    let lower = kind.to_ascii_lowercase();
    match lower.as_str() {
        "rust" => "rust".to_string(),
        "node" => "node".to_string(),
        "ts" | "typescript" => "node".to_string(), // typescript uses the node sidecar
        "python" | "py" => "python".to_string(),
        "c" | "cpp" | "c-cpp" | "c_cpp" | "c++" => "c-cpp".to_string(),
        "go" | "golang" => "go".to_string(),
        _ => lower,
    }
}

pub fn default_toolchain_image(kind: &str) -> String {
    match kind {
        "rust" => {
            // Explicit override takes precedence
            if let Ok(img) = env::var("AIFO_RUST_TOOLCHAIN_IMAGE") {
                if !img.trim().is_empty() {
                    return img;
                }
            }
            // Force official rust image when requested; prefer versioned tag if provided
            if env::var("AIFO_RUST_TOOLCHAIN_USE_OFFICIAL").ok().as_deref() == Some("1") {
                let ver = env::var("AIFO_RUST_TOOLCHAIN_VERSION").ok();
                let v_opt = ver.as_deref().map(|s| s.trim()).filter(|s| !s.is_empty());
                return official_rust_image_for_version(v_opt);
            }
            // Prefer our first-party toolchain image; versioned when requested.
            if let Ok(ver) = env::var("AIFO_RUST_TOOLCHAIN_VERSION") {
                let v = ver.trim();
                if !v.is_empty() {
                    return format!("aifo-rust-toolchain:{}", v);
                }
            }
            "aifo-rust-toolchain:latest".to_string()
        }
        "node" => "node:20-bookworm-slim".to_string(),
        "python" => "python:3.12-slim".to_string(),
        "c-cpp" => "aifo-cpp-toolchain:latest".to_string(),
        "go" => "golang:1.22-bookworm".to_string(),
        _ => "node:20-bookworm-slim".to_string(),
    }
}

/// Compute default image from kind@version (best-effort).
pub fn default_toolchain_image_for_version(kind: &str, version: &str) -> String {
    match kind {
        "rust" => format!("aifo-rust-toolchain:{}", version),
        "node" | "typescript" => format!("node:{}-bookworm-slim", version),
        "python" => format!("python:{}-slim", version),
        "go" => format!("golang:{}-bookworm", version),
        "c-cpp" => "aifo-cpp-toolchain:latest".to_string(), // no version mapping
        _ => default_toolchain_image(kind),
    }
}

// Heuristic to detect official rust images like "rust:<tag>" (with or without a registry prefix)
fn is_official_rust_image(image: &str) -> bool {
    let image = image.trim();
    if image.is_empty() {
        return false;
    }
    // Take the repository component before the last ':' to avoid confusing registry host:port
    let mut parts = image.rsplitn(2, ':');
    let _tag = parts.next().unwrap_or("");
    let repo = parts.next().unwrap_or(image);
    // Last path segment should be "rust" for official images
    let last_seg = repo.rsplit('/').next().unwrap_or(repo);
    last_seg == "rust"
}

fn official_rust_image_for_version(version_opt: Option<&str>) -> String {
    let v = match version_opt {
        Some(s) if !s.is_empty() => s,
        _ => "1.80",
    };
    format!("rust:{}-bookworm", v)
}

/// Best-effort ownership initialization for named cargo volumes used by rust sidecar.
/// Runs a short helper container as root that ensures target dir exists, chowns to uid:gid,
/// and drops a stamp file to avoid repeated work. Uses the same image as the sidecar to avoid extra pulls.
fn init_rust_named_volume(
    runtime: &Path,
    image: &str,
    subdir: &str,
    uid: u32,
    gid: u32,
    verbose: bool,
) {
    let mount = format!("aifo-cargo-{}:/home/coder/.cargo/{}", subdir, subdir);
    let script = format!(
        "set -e; d=\"/home/coder/.cargo/{sd}\"; if [ -f \"$d/.aifo-init-done\" ]; then exit 0; fi; mkdir -p \"$d\"; chown -R {uid}:{gid} \"$d\" || true; printf '%s\\n' '{uid}:{gid}' > \"$d/.aifo-init-done\" || true",
        sd = subdir,
        uid = uid,
        gid = gid
    );
    let args: Vec<String> = vec![
        "docker".to_string(),
        "run".to_string(),
        "--rm".to_string(),
        "-v".to_string(),
        mount,
        image.to_string(),
        "sh".to_string(),
        "-lc".to_string(),
        script,
    ];
    if verbose {
        eprintln!("aifo-coder: docker: {}", shell_join(&args));
    }
    let mut cmd = Command::new(runtime);
    for a in &args[1..] {
        cmd.arg(a);
    }
    if !verbose {
        cmd.stdout(Stdio::null()).stderr(Stdio::null());
    }
    match cmd.status() {
        Ok(st) => {
            if verbose && !st.success() {
                eprintln!(
                    "aifo-coder: warning: volume ownership init failed for aifo-cargo-{} (exit {:?})",
                    subdir,
                    st.code()
                );
            }
        }
        Err(e) => {
            if verbose {
                eprintln!(
                    "aifo-coder: warning: failed to run ownership init for aifo-cargo-{}: {}",
                    subdir, e
                );
            }
        }
    }
}

/// Inspect run-args and initialize named rust cargo volumes when they are selected (registry/git).
fn init_rust_named_volumes_if_needed(
    runtime: &Path,
    image: &str,
    run_args: &[String],
    uidgid: Option<(u32, u32)>,
    verbose: bool,
) {
    let mut need_registry = false;
    let mut need_git = false;
    let mut i = 0usize;
    while i + 1 < run_args.len() {
        if run_args[i] == "-v" {
            let mnt = &run_args[i + 1];
            if mnt.starts_with("aifo-cargo-registry:")
                && mnt.ends_with("/home/coder/.cargo/registry")
            {
                need_registry = true;
            } else if mnt.starts_with("aifo-cargo-git:") && mnt.ends_with("/home/coder/.cargo/git")
            {
                need_git = true;
            }
            i += 2;
        } else {
            i += 1;
        }
    }
    if !need_registry && !need_git {
        return;
    }
    let (uid, gid) = uidgid.unwrap_or((0u32, 0u32));
    if need_registry {
        init_rust_named_volume(runtime, image, "registry", uid, gid, verbose);
    }
    if need_git {
        init_rust_named_volume(runtime, image, "git", uid, gid, verbose);
    }
}

fn sidecar_container_name(kind: &str, id: &str) -> String {
    format!("aifo-tc-{kind}-{id}")
}

fn sidecar_network_name(id: &str) -> String {
    format!("aifo-net-{id}")
}

fn ensure_network_exists(runtime: &Path, name: &str, verbose: bool) -> bool {
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

fn remove_network(runtime: &Path, name: &str, verbose: bool) {
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
    args.push("-v".to_string());
    args.push(format!("{}:/workspace", pwd.display()));

    match kind {
        "rust" => {
            // Normative env for rust sidecar
            args.push("-e".to_string());
            args.push("CARGO_HOME=/home/coder/.cargo".to_string());
            // Ensure PATH exposes cargo-installed tools first and preserves existing PATH
            args.push("-e".to_string());
            args.push("PATH=/home/coder/.cargo/bin:/usr/local/cargo/bin:$PATH".to_string());
            // Ensure build scripts use gcc/g++ explicitly; rely on image PATH
            args.push("-e".to_string());
            args.push("CC=gcc".to_string());
            args.push("-e".to_string());
            args.push("CXX=g++".to_string());
            // Default RUST_BACKTRACE=1 when unset
            let rb = env::var("RUST_BACKTRACE").ok();
            if rb.as_deref().map(|s| s.is_empty()).unwrap_or(true) {
                args.push("-e".to_string());
                args.push("RUST_BACKTRACE=1".to_string());
            }
            // Cargo cache mounts
            if !no_cache {
                let force_named = cfg!(windows)
                    || env::var("AIFO_TOOLCHAIN_RUST_USE_DOCKER_VOLUMES")
                        .ok()
                        .as_deref()
                        == Some("1");
                if force_named {
                    // Primary mounts at normative CARGO_HOME
                    args.push("-v".to_string());
                    args.push("aifo-cargo-registry:/home/coder/.cargo/registry".to_string());
                    args.push("-v".to_string());
                    args.push("aifo-cargo-git:/home/coder/.cargo/git".to_string());
                    // Back-compat: also mount at legacy /usr/local/cargo paths for older tests/tools
                    args.push("-v".to_string());
                    args.push("aifo-cargo-registry:/usr/local/cargo/registry".to_string());
                    args.push("-v".to_string());
                    args.push("aifo-cargo-git:/usr/local/cargo/git".to_string());
                } else {
                    let mut mounted_registry = false;
                    let mut mounted_git = false;
                    let hd_opt = env::var("HOME")
                        .ok()
                        .filter(|s| !s.trim().is_empty())
                        .map(PathBuf::from)
                        .or_else(home::home_dir);
                    if let Some(hd) = hd_opt.clone() {
                        let reg = hd.join(".cargo").join("registry");
                        let git = hd.join(".cargo").join("git");
                        if reg.exists() {
                            // Host-preferred mount at normative CARGO_HOME (avoid duplicate mount points)
                            args.push("-v".to_string());
                            args.push(format!("{}:/home/coder/.cargo/registry", reg.display()));
                            // Back-compat: also mount named volume at legacy /usr/local/cargo path (different target)
                            args.push("-v".to_string());
                            args.push("aifo-cargo-registry:/usr/local/cargo/registry".to_string());
                            mounted_registry = true;
                        }
                        if git.exists() {
                            // Host-preferred mount at normative CARGO_HOME (avoid duplicate mount points)
                            args.push("-v".to_string());
                            args.push(format!("{}:/home/coder/.cargo/git", git.display()));
                            // Back-compat: also mount named volume at legacy /usr/local/cargo path (different target)
                            args.push("-v".to_string());
                            args.push("aifo-cargo-git:/usr/local/cargo/git".to_string());
                            mounted_git = true;
                        }
                    }
                    if !mounted_registry {
                        args.push("-v".to_string());
                        args.push("aifo-cargo-registry:/home/coder/.cargo/registry".to_string());
                        // Back-compat legacy path for older tests/tools
                        args.push("-v".to_string());
                        args.push("aifo-cargo-registry:/usr/local/cargo/registry".to_string());
                    }
                    if !mounted_git {
                        args.push("-v".to_string());
                        args.push("aifo-cargo-git:/home/coder/.cargo/git".to_string());
                        // Back-compat legacy path for older tests/tools
                        args.push("-v".to_string());
                        args.push("aifo-cargo-git:/usr/local/cargo/git".to_string());
                    }
                }
            }
            // Optional: host cargo config (read-only)
            if env::var("AIFO_TOOLCHAIN_RUST_USE_HOST_CONFIG")
                .ok()
                .as_deref()
                == Some("1")
            {
                let hd_opt = env::var("HOME")
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
                        args.push("-v".to_string());
                        args.push(format!("{}:/home/coder/.cargo/config.toml:ro", p.display()));
                    }
                }
            }
            // Optional: SSH agent forwarding
            if env::var("AIFO_TOOLCHAIN_SSH_FORWARD").ok().as_deref() == Some("1") {
                if let Ok(sock) = env::var("SSH_AUTH_SOCK") {
                    if !sock.trim().is_empty() {
                        args.push("-v".to_string());
                        args.push(format!("{0}:{0}", sock));
                        args.push("-e".to_string());
                        args.push(format!("SSH_AUTH_SOCK={}", sock));
                    }
                }
            }
            // Optional: sccache
            if env::var("AIFO_RUST_SCCACHE").ok().as_deref() == Some("1") {
                let target = "/home/coder/.cache/sccache";
                if let Ok(dir) = env::var("AIFO_RUST_SCCACHE_DIR") {
                    if !dir.trim().is_empty() {
                        args.push("-v".to_string());
                        args.push(format!("{}:{}", dir, target));
                    } else {
                        args.push("-v".to_string());
                        args.push(format!("aifo-sccache:{}", target));
                    }
                } else {
                    args.push("-v".to_string());
                    args.push(format!("aifo-sccache:{}", target));
                }
                args.push("-e".to_string());
                args.push("RUSTC_WRAPPER=sccache".to_string());
                args.push("-e".to_string());
                args.push(format!("SCCACHE_DIR={}", target));
            }
            // Pass-through proxies and cargo networking envs
            let passthrough = [
                "HTTP_PROXY",
                "HTTPS_PROXY",
                "NO_PROXY",
                "http_proxy",
                "https_proxy",
                "no_proxy",
                "CARGO_NET_GIT_FETCH_WITH_CLI",
                "CARGO_REGISTRIES_CRATES_IO_PROTOCOL",
            ];
            for name in passthrough.iter() {
                if let Ok(val) = env::var(name) {
                    if !val.is_empty() {
                        args.push("-e".to_string());
                        args.push(format!("{}={}", name, val));
                    }
                }
            }
            // Optional: fast linkers via RUSTFLAGS (lld/mold)
            if let Ok(linker) = env::var("AIFO_RUST_LINKER") {
                let lk = linker.to_ascii_lowercase();
                let extra = if lk == "lld" {
                    Some("-Clinker=clang -Clink-arg=-fuse-ld=lld")
                } else if lk == "mold" {
                    Some("-Clinker=clang -Clink-arg=-fuse-ld=mold")
                } else {
                    None
                };
                if let Some(add) = extra {
                    let base = env::var("RUSTFLAGS").ok().unwrap_or_default();
                    let rf = if base.trim().is_empty() {
                        add.to_string()
                    } else {
                        format!("{} {}", base, add)
                    };
                    args.push("-e".to_string());
                    args.push(format!("RUSTFLAGS={}", rf));
                }
            }
        }
        "node" => {
            if !no_cache {
                args.push("-v".to_string());
                args.push("aifo-npm-cache:/home/coder/.npm".to_string());
            }
        }
        "python" => {
            if !no_cache {
                args.push("-v".to_string());
                args.push("aifo-pip-cache:/home/coder/.cache/pip".to_string());
            }
        }
        "c-cpp" => {
            if !no_cache {
                args.push("-v".to_string());
                args.push("aifo-ccache:/home/coder/.cache/ccache".to_string());
            }
            args.push("-e".to_string());
            args.push("CCACHE_DIR=/home/coder/.cache/ccache".to_string());
        }
        "go" => {
            if !no_cache {
                args.push("-v".to_string());
                args.push("aifo-go:/go".to_string());
            }
            args.push("-e".to_string());
            args.push("GOPATH=/go".to_string());
            args.push("-e".to_string());
            args.push("GOMODCACHE=/go/pkg/mod".to_string());
            args.push("-e".to_string());
            args.push("GOCACHE=/go/build-cache".to_string());
        }
        _ => {}
    }

    // base env and workdir
    args.push("-e".to_string());
    args.push("HOME=/home/coder".to_string());
    args.push("-e".to_string());
    args.push("GNUPGHOME=/home/coder/.gnupg".to_string());
    args.push("-w".to_string());
    args.push("/workspace".to_string());

    if let Some(profile) = apparmor {
        if docker_supports_apparmor() {
            args.push("--security-opt".to_string());
            args.push(format!("apparmor={profile}"));
        }
    }

    // Linux connectivity (host proxy via host-gateway) for sidecars as well
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
    args.push("-e".to_string());
    args.push("HOME=/home/coder".to_string());
    args.push("-e".to_string());
    args.push("GNUPGHOME=/home/coder/.gnupg".to_string());

    // Phase 2 marking: when executing with an official rust image, mark for bootstrap (Phase 4 will consume this)
    if kind == "rust"
        && std::env::var("AIFO_RUST_OFFICIAL_BOOTSTRAP")
            .ok()
            .as_deref()
            == Some("1")
    {
        args.push("-e".to_string());
        args.push("AIFO_RUST_OFFICIAL_BOOTSTRAP=1".to_string());
    }

    match kind {
        "rust" => {
            args.push("-e".to_string());
            args.push("CARGO_HOME=/home/coder/.cargo".to_string());
            // Ensure PATH exposes cargo-installed tools first and preserves existing PATH
            args.push("-e".to_string());
            args.push("PATH=/home/coder/.cargo/bin:/usr/local/cargo/bin:$PATH".to_string());
            // Ensure build scripts use gcc/g++ explicitly; rely on image PATH
            args.push("-e".to_string());
            args.push("CC=gcc".to_string());
            args.push("-e".to_string());
            args.push("CXX=g++".to_string());
            // Default RUST_BACKTRACE=1 when unset
            let rb = env::var("RUST_BACKTRACE").ok();
            if rb.as_deref().map(|s| s.is_empty()).unwrap_or(true) {
                args.push("-e".to_string());
                args.push("RUST_BACKTRACE=1".to_string());
            }
            // Optional: fast linkers via RUSTFLAGS (lld/mold)
            if let Ok(linker) = env::var("AIFO_RUST_LINKER") {
                let lk = linker.to_ascii_lowercase();
                let extra = if lk == "lld" {
                    Some("-Clinker=clang -Clink-arg=-fuse-ld=lld")
                } else if lk == "mold" {
                    Some("-Clinker=clang -Clink-arg=-fuse-ld=mold")
                } else {
                    None
                };
                if let Some(add) = extra {
                    let base = env::var("RUSTFLAGS").ok().unwrap_or_default();
                    let rf = if base.trim().is_empty() {
                        add.to_string()
                    } else {
                        format!("{} {}", base, add)
                    };
                    args.push("-e".to_string());
                    args.push(format!("RUSTFLAGS={}", rf));
                }
            }
            // Pass-through proxies and cargo networking envs for exec as well
            let passthrough = [
                "HTTP_PROXY",
                "HTTPS_PROXY",
                "NO_PROXY",
                "http_proxy",
                "https_proxy",
                "no_proxy",
                "CARGO_NET_GIT_FETCH_WITH_CLI",
                "CARGO_REGISTRIES_CRATES_IO_PROTOCOL",
            ];
            for name in passthrough.iter() {
                if let Ok(val) = env::var(name) {
                    if !val.is_empty() {
                        args.push("-e".to_string());
                        args.push(format!("{}={}", name, val));
                    }
                }
            }
        }
        "python" => {
            let venv_bin = pwd.join(".venv").join("bin");
            if venv_bin.exists() {
                args.push("-e".to_string());
                args.push("VIRTUAL_ENV=/workspace/.venv".to_string());
                args.push("-e".to_string());
                args.push("PATH=/workspace/.venv/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin".to_string());
            }
        }
        "c-cpp" => {
            args.push("-e".to_string());
            args.push("CCACHE_DIR=/home/coder/.cache/ccache".to_string());
        }
        "go" => {
            args.push("-e".to_string());
            args.push("GOPATH=/go".to_string());
            args.push("-e".to_string());
            args.push("GOMODCACHE=/go/pkg/mod".to_string());
            args.push("-e".to_string());
            args.push("GOCACHE=/go/build-cache".to_string());
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
        args.push("-lc".to_string());
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

fn sidecar_allowlist(kind: &str) -> &'static [&'static str] {
    match kind {
        "rust" => &[
            "cargo",
            "rustc",
            // allow common dev tools present in rust toolchain
            "make",
            "cmake",
            "ninja",
            "pkg-config",
            "gcc",
            "g++",
            "clang",
            "clang++",
            "cc",
            "c++",
        ],
        "node" => &[
            "node",
            "npm",
            "npx",
            "tsc",
            "ts-node",
            // allow dev tools if present in node image
            "make",
            "cmake",
            "ninja",
            "pkg-config",
            "gcc",
            "g++",
            "clang",
            "clang++",
            "cc",
            "c++",
        ],
        "python" => &[
            "python",
            "python3",
            "pip",
            "pip3",
            // allow dev tools if present in python image
            "make",
            "cmake",
            "ninja",
            "pkg-config",
            "gcc",
            "g++",
            "clang",
            "clang++",
            "cc",
            "c++",
        ],
        "c-cpp" => &[
            "gcc",
            "g++",
            "cc",
            "c++",
            "clang",
            "clang++",
            "make",
            "cmake",
            "ninja",
            "pkg-config",
        ],
        "go" => &[
            "go",
            "gofmt",
            // allow dev tools if present in go image
            "make",
            "cmake",
            "ninja",
            "pkg-config",
            "gcc",
            "g++",
            "clang",
            "clang++",
            "cc",
            "c++",
        ],
        _ => &[],
    }
}

/// Map a tool name to the sidecar kind.
pub fn route_tool_to_sidecar(tool: &str) -> &'static str {
    let t = tool.to_ascii_lowercase();
    match t.as_str() {
        // rust
        "cargo" | "rustc" => "rust",
        // node/typescript
        "node" | "npm" | "npx" | "tsc" | "ts-node" => "node",
        // python
        "python" | "python3" | "pip" | "pip3" => "python",
        // c/c++
        "gcc" | "g++" | "clang" | "clang++" | "make" | "cmake" | "ninja" | "pkg-config" => "c-cpp",
        // go
        "go" | "gofmt" => "go",
        _ => "node",
    }
}

// Determine if a tool is a generic build tool that may exist across sidecars
fn is_dev_tool(tool: &str) -> bool {
    matches!(
        tool,
        "make"
            | "cmake"
            | "ninja"
            | "pkg-config"
            | "gcc"
            | "g++"
            | "clang"
            | "clang++"
            | "cc"
            | "c++"
    )
}

// Best-effort: check if a container with the given name exists (running or created)
fn container_exists(name: &str) -> bool {
    if let Ok(runtime) = container_runtime_path() {
        return Command::new(&runtime)
            .arg("inspect")
            .arg(name)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
    }
    false
}

// Best-effort: check if tool is available inside the given container (cached by caller)
fn tool_available_in(name: &str, tool: &str, timeout_secs: u64) -> bool {
    if let Ok(runtime) = container_runtime_path() {
        let mut cmd = Command::new(&runtime);
        cmd.arg("exec")
            .arg(name)
            .arg("sh")
            .arg("-lc")
            .arg(format!("command -v {} >/dev/null 2>&1", tool))
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        // Run with a simple timeout by spawning and joining
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let st = cmd.status();
            let _ = tx.send(st.ok().map(|s| s.success()).unwrap_or(false));
        });
        if let Ok(ok) = rx.recv_timeout(Duration::from_secs(timeout_secs)) {
            return ok;
        }
    }
    false
}

// Preferred sidecars for a given tool (in order)
fn preferred_kinds_for_tool(tool: &str) -> Vec<&'static str> {
    let t = tool.to_ascii_lowercase();
    if is_dev_tool(&t) {
        vec!["c-cpp", "rust", "go", "node", "python"]
    } else {
        vec![route_tool_to_sidecar(&t)]
    }
}

// Select the best sidecar kind for tool based on running containers and availability; fallback to primary preference.
fn select_kind_for_tool(
    session_id: &str,
    tool: &str,
    timeout_secs: u64,
    cache: &mut HashMap<(String, String), bool>,
) -> String {
    let prefs = preferred_kinds_for_tool(tool);
    for k in &prefs {
        let name = sidecar_container_name(k, session_id);
        if !container_exists(&name) {
            continue;
        }
        let key = (name.clone(), tool.to_ascii_lowercase());
        let ok = if let Some(cached) = cache.get(&key) {
            *cached
        } else {
            let avail = tool_available_in(&name, tool, timeout_secs);
            cache.insert(key.clone(), avail);
            avail
        };
        if ok {
            return (*k).to_string();
        }
    }
    // fallback to first preference (may not be running; higher layers handle errors)
    prefs[0].to_string()
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
        let p = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
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
    // Phase 2: mark official rust image selection to engage bootstrap in exec (handled in Phase 4)
    let bootstrap_official_rust = sidecar_kind == "rust"
        && (env::var("AIFO_RUST_TOOLCHAIN_USE_OFFICIAL").ok().as_deref() == Some("1")
            || is_official_rust_image(&image));
    let session_id = env::var("AIFO_CODER_FORK_SESSION")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(create_session_id);
    let net_name = sidecar_network_name(&session_id);
    let name = sidecar_container_name(sidecar_kind.as_str(), &session_id);

    // Ensure network exists before starting sidecar
    if !dry_run && !ensure_network_exists(&runtime, &net_name, verbose) {
        return Err(io::Error::other(format!(
            "failed to create or verify network {}",
            net_name
        )));
    }

    let apparmor_profile = desired_apparmor_profile();

    // Build and optionally run sidecar
    let run_preview_args = build_sidecar_run_preview(
        &name,
        Some(&net_name),
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
    if bootstrap_official_rust {
        env::set_var("AIFO_RUST_OFFICIAL_BOOTSTRAP", "1");
    } else {
        env::remove_var("AIFO_RUST_OFFICIAL_BOOTSTRAP");
    }
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
    env::remove_var("AIFO_RUST_OFFICIAL_BOOTSTRAP");

    // Cleanup: stop sidecar and remove network (best-effort)
    if !dry_run {
        let mut stop_cmd = Command::new(&runtime);
        stop_cmd.arg("stop").arg(&name);
        if !verbose {
            stop_cmd.stdout(Stdio::null()).stderr(Stdio::null());
        }
        let _ = stop_cmd.status();

        remove_network(&runtime, &net_name, verbose);
    }

    Ok(exit_code)
}

fn random_token() -> String {
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

/// Parse minimal application/x-www-form-urlencoded body; supports repeated keys.
pub fn parse_form_urlencoded(body: &str) -> Vec<(String, String)> {
    let mut res = Vec::new();
    for pair in body.split('&') {
        if pair.is_empty() {
            continue;
        }
        let mut it = pair.splitn(2, '=');
        let k = it.next().unwrap_or_default();
        let v = it.next().unwrap_or_default();
        res.push((url_decode(k), url_decode(v)));
    }
    res
}

/// Parse ~/.aider.conf.yml and extract notifications-command as argv tokens.
pub fn parse_notifications_command_config() -> Result<Vec<String>, String> {
    // Allow tests (and power users) to override config path explicitly
    let path = if let Ok(p) = env::var("AIFO_NOTIFICATIONS_CONFIG") {
        let p = p.trim().to_string();
        if !p.is_empty() {
            PathBuf::from(p)
        } else {
            home::home_dir()
                .ok_or_else(|| "home directory not found".to_string())?
                .join(".aider.conf.yml")
        }
    } else {
        home::home_dir()
            .ok_or_else(|| "home directory not found".to_string())?
            .join(".aider.conf.yml")
    };
    let content =
        fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {}", path.display(), e))?;

    // Pre-split lines to allow simple multi-line parsing
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0usize;
    while i < lines.len() {
        let line = lines[i];
        let l = line.trim_start();
        if l.starts_with('#') || l.is_empty() {
            i += 1;
            continue;
        }
        if let Some(rest) = l.strip_prefix("notifications-command:") {
            let mut val = rest.trim().to_string();
            // Tolerate configs/tests that append a literal "\n" at end of line
            if val.ends_with("\\n") {
                val.truncate(val.len() - 2);
            }

            // Helper: parse inline JSON/YAML-like array ["say","--title","AIFO"]
            let parse_inline_array = |val: &str| -> Result<Vec<String>, String> {
                let inner = &val[1..val.len() - 1];
                let mut argv: Vec<String> = Vec::new();
                let mut cur = String::new();
                let mut in_single = false;
                let mut in_double = false;
                let mut esc = false;
                for ch in inner.chars() {
                    if esc {
                        let c = match ch {
                            'n' => '\n',
                            'r' => '\r',
                            't' => '\t',
                            other => other,
                        };
                        cur.push(c);
                        esc = false;
                        continue;
                    }
                    match ch {
                        '\\' if in_double || in_single => esc = true,
                        '"' if !in_single => {
                            if in_double {
                                in_double = false;
                                argv.push(cur.clone());
                                cur.clear();
                            } else {
                                in_double = true;
                            }
                        }
                        '\'' if !in_double => {
                            if in_single {
                                in_single = false;
                                argv.push(cur.clone());
                                cur.clear();
                            } else {
                                in_single = true;
                            }
                        }
                        ',' if !in_single && !in_double => { /* separator */ }
                        c => {
                            if in_single || in_double {
                                cur.push(c);
                            }
                        }
                    }
                }
                if !cur.is_empty() && !in_single && !in_double {
                    argv.push(cur);
                }
                if argv.is_empty() {
                    Err("notifications-command parsed to an empty command".to_string())
                } else {
                    Ok(argv)
                }
            };

            // Case 1: inline array
            if val.starts_with('[') && val.ends_with(']') {
                return parse_inline_array(&val);
            }

            // Case 2: explicit block scalars '|' or '>'
            if val == "|" || val == ">" || val.is_empty() {
                // Collect subsequent indented lines; also support YAML list items beginning with '-'
                let mut j = i + 1;
                // Skip blank/comment lines until first candidate
                while j < lines.len()
                    && (lines[j].trim().is_empty() || lines[j].trim_start().starts_with('#'))
                {
                    j += 1;
                }
                if j >= lines.len() {
                    return Err("notifications-command is empty or malformed".to_string());
                }
                let first = lines[j];
                let is_list = first.trim_start().starts_with('-');
                if is_list {
                    let mut argv: Vec<String> = Vec::new();
                    while j < lines.len() {
                        let ln = lines[j];
                        let t = ln.trim_start();
                        if !t.starts_with('-') {
                            break;
                        }
                        let item = t.trim_start_matches('-').trim();
                        if !item.is_empty() {
                            argv.push(strip_outer_quotes(item));
                        }
                        j += 1;
                    }
                    if argv.is_empty() {
                        return Err("notifications-command list is empty".to_string());
                    }
                    return Ok(argv);
                } else {
                    // Block scalar: concatenate trimmed lines with spaces into a single command string
                    let mut parts: Vec<String> = Vec::new();
                    while j < lines.len() {
                        let ln = lines[j];
                        let t = ln.trim_start();
                        if t.is_empty() || t.starts_with('#') {
                            j += 1;
                            continue;
                        }
                        // Stop if de-indented to column 0 and looks like a new key
                        if !ln.starts_with(' ') && t.contains(':') {
                            break;
                        }
                        parts.push(t.to_string());
                        j += 1;
                    }
                    let joined = parts.join(" ");
                    let argv = shell_like_split_args(&strip_outer_quotes(&joined));
                    if argv.is_empty() {
                        return Err("notifications-command parsed to an empty command".to_string());
                    }
                    return Ok(argv);
                }
            }

            // Case 3: single-line scalar
            let unquoted = strip_outer_quotes(&val);
            let argv = shell_like_split_args(&unquoted);
            if argv.is_empty() {
                return Err("notifications-command parsed to an empty command".to_string());
            }
            return Ok(argv);
        }
        i += 1;
    }
    Err("notifications-command not found in ~/.aider.conf.yml".to_string())
}

/// Validate and, if allowed, execute the host 'say' command with provided args.
/// Returns (exit_code, output_bytes) on success, or Err(reason) if rejected.
pub fn notifications_handle_request(
    argv: &[String],
    _verbose: bool,
    timeout_secs: u64,
) -> Result<(i32, Vec<u8>), String> {
    let cfg_argv = parse_notifications_command_config()?;
    if cfg_argv.is_empty() {
        return Err("notifications-command is empty".to_string());
    }
    if cfg_argv[0] != "say" {
        return Err("only 'say' is allowed as notifications-command executable".to_string());
    }
    let cfg_args = &cfg_argv[1..];
    if cfg_args != argv {
        return Err(format!(
            "arguments mismatch: configured {:?} vs requested {:?}",
            cfg_args, argv
        ));
    }

    // Execute 'say' on the host with a timeout.
    let (tx, rx) = std::sync::mpsc::channel();
    let args_vec: Vec<String> = argv.to_vec();
    std::thread::spawn(move || {
        let mut cmd = Command::new("say");
        for a in &args_vec {
            cmd.arg(a);
        }
        let out = cmd.output();
        let _ = tx.send(out);
    });
    match rx.recv_timeout(std::time::Duration::from_secs(timeout_secs)) {
        Ok(Ok(o)) => {
            let mut b = o.stdout;
            if !o.stderr.is_empty() {
                b.extend_from_slice(&o.stderr);
            }
            Ok((o.status.code().unwrap_or(1), b))
        }
        Ok(Err(e)) => Err(format!("failed to execute host 'say': {}", e)),
        Err(_timeout) => Err("host 'say' execution timed out".to_string()),
    }
}

/// Write aifo-shim and tool wrappers into the given directory.
pub fn toolchain_write_shims(dir: &Path) -> io::Result<()> {
    let tools = [
        "cargo",
        "rustc",
        "node",
        "npm",
        "npx",
        "tsc",
        "ts-node",
        "python",
        "pip",
        "pip3",
        "gcc",
        "g++",
        "cc",
        "c++",
        "clang",
        "clang++",
        "make",
        "cmake",
        "ninja",
        "pkg-config",
        "go",
        "gofmt",
        "notifications-cmd",
    ];
    fs::create_dir_all(dir)?;
    let shim_path = dir.join("aifo-shim");
    let shim = r#"#!/bin/sh
set -e
if [ -z "$AIFO_TOOLEEXEC_URL" ] || [ -z "$AIFO_TOOLEEXEC_TOKEN" ]; then
  echo "aifo-shim: proxy not configured. Please launch agent with --toolchain." >&2
  exit 86
fi
tool="$(basename "$0")"
cwd="$(pwd)"
tmp="${TMPDIR:-/tmp}/aifo-shim.$$"
mkdir -p "$tmp"
# Build curl form payload (-d key=value supports urlencoding)
cmd=(curl -sS --no-buffer -D "$tmp/h" -X POST -H "Authorization: Bearer $AIFO_TOOLEEXEC_TOKEN" -H "X-Aifo-Proto: 2" -H "TE: trailers")
cmd+=(-d "tool=$tool" -d "cwd=$cwd")
# Append args preserving order
for a in "$@"; do
  cmd+=(-d "arg=$a")
done
# Detect optional unix socket URL (Linux unix transport)
if printf %s "$AIFO_TOOLEEXEC_URL" | grep -q '^unix://'; then
  SOCKET="${AIFO_TOOLEEXEC_URL#unix://}"
  cmd+=(--unix-socket "$SOCKET")
  URL="http://localhost/exec"
else
  URL="$AIFO_TOOLEEXEC_URL"
fi
cmd+=("$URL")
"${cmd[@]}" || true
ec="$(awk '/^X-Exit-Code:/{print $2}' "$tmp/h" | tr -d '\r' | tail -n1)"
: # body streamed directly by curl
rm -rf "$tmp"
# Fallback to 1 if header missing
case "$ec" in '' ) ec=1 ;; esac
exit "$ec"
"#;
    fs::write(&shim_path, shim)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&shim_path, fs::Permissions::from_mode(0o755))?;
    }
    for t in tools {
        let path = dir.join(t);
        fs::write(
            &path,
            "#!/bin/sh\nexec \"$(dirname \"$0\")/aifo-shim\" \"$@\"\n",
        )?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
        }
    }
    Ok(())
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
        let p = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        fs::canonicalize(&p).unwrap_or(p)
    };

    #[cfg(unix)]
    let uid: u32 = u32::from(getuid());
    #[cfg(unix)]
    let gid: u32 = u32::from(getgid());
    #[cfg(not(unix))]
    let (_uid, _gid) = (0u32, 0u32);

    let session_id = env::var("AIFO_CODER_FORK_SESSION")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(create_session_id);
    let net_name = sidecar_network_name(&session_id);
    if !ensure_network_exists(&runtime, &net_name, verbose) {
        return Err(io::Error::other("failed to create session network"));
    }

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
        // Phase 2: mark official rust image so proxy execs can engage bootstrap (Phase 4)
        if kind == "rust" {
            if is_official_rust_image(&image)
                || env::var("AIFO_RUST_TOOLCHAIN_USE_OFFICIAL").ok().as_deref() == Some("1")
            {
                env::set_var("AIFO_RUST_OFFICIAL_BOOTSTRAP", "1");
            } else {
                env::remove_var("AIFO_RUST_OFFICIAL_BOOTSTRAP");
            }
        }
        let name = sidecar_container_name(kind.as_str(), &session_id);
        let args = build_sidecar_run_preview(
            &name,
            Some(&net_name),
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

/// Start a minimal proxy to execute tools via shims inside sidecars.
/// Returns (url, token, running_flag, thread_handle).
pub fn toolexec_start_proxy(
    session_id: &str,
    verbose: bool,
) -> io::Result<(
    String,
    String,
    std::sync::Arc<std::sync::atomic::AtomicBool>,
    std::thread::JoinHandle<()>,
)> {
    let runtime = container_runtime_path()?;

    #[cfg(unix)]
    let uid: u32 = u32::from(getuid());
    #[cfg(unix)]
    let gid: u32 = u32::from(getgid());
    #[cfg(not(unix))]
    let (uid, gid) = (0u32, 0u32);

    // Prepare shared proxy state (token, timeout, running flag, session id)
    let token = random_token();
    let token_for_thread = token.clone();
    // Per-request timeout (seconds); default 60
    let timeout_secs: u64 = env::var("AIFO_TOOLEEXEC_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|&v| v > 0)
        .unwrap_or(60);
    let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let session = session_id.to_string();

    // Optional unix socket transport on Linux, gated by AIFO_TOOLEEXEC_USE_UNIX=1
    let use_unix = cfg!(target_os = "linux")
        && env::var("AIFO_TOOLEEXEC_USE_UNIX").ok().as_deref() == Some("1");
    if use_unix {
        #[cfg(target_os = "linux")]
        {
            // Create host socket directory and bind UnixListener
            let base = "/run/aifo";
            let _ = fs::create_dir_all(base);
            let host_dir = format!("{}/aifo-{}", base, session);
            let _ = fs::create_dir_all(&host_dir);
            let sock_path = format!("{}/toolexec.sock", host_dir);
            let _ = fs::remove_file(&sock_path);
            let listener = UnixListener::bind(&sock_path)
                .map_err(|e| io::Error::new(e.kind(), format!("proxy unix bind failed: {e}")))?;
            let _ = listener.set_nonblocking(true);
            // Expose directory for agent mount
            env::set_var("AIFO_TOOLEEXEC_UNIX_DIR", &host_dir);
            let running_cl2 = running.clone();
            let token_for_thread2 = token_for_thread.clone();
            let handle = std::thread::spawn(move || {
                let mut tool_cache: HashMap<(String, String), bool> = HashMap::new();
                loop {
                    if !running_cl2.load(std::sync::atomic::Ordering::SeqCst) {
                        break;
                    }
                    let mut stream = match listener.accept() {
                        Ok((s, _addr)) => s,
                        Err(e) => {
                            if e.kind() == io::ErrorKind::WouldBlock {
                                std::thread::sleep(Duration::from_millis(50));
                                continue;
                            } else {
                                continue;
                            }
                        }
                    };
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(timeout_secs)));
                    let _ = stream.set_write_timeout(None);
                    // Read request (simple HTTP)
                    let mut buf = Vec::new();
                    let mut hdr = Vec::new();
                    let mut tmp = [0u8; 1024];
                    // Read until end of headers
                    let mut header_end = None;
                    while header_end.is_none() {
                        match stream.read(&mut tmp) {
                            Ok(0) => break,
                            Ok(n) => {
                                buf.extend_from_slice(&tmp[..n]);
                                if let Some(end) = find_header_end(&buf) {
                                    header_end = Some(end);
                                } else if let Some(pos) = buf.windows(2).position(|w| w == b"\n\n")
                                {
                                    // Be tolerant to LF-only header termination used by some simple clients/tests
                                    header_end = Some(pos);
                                }
                                if buf.len() > 64 * 1024 {
                                    break;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                    let hend = if let Some(h) = header_end {
                        h
                    } else if !buf.is_empty() {
                        // Tolerate missing CRLFCRLF for simple clients: treat entire buffer as headers
                        buf.len()
                    } else {
                        let body = b"unauthorized\n";
                        let header = format!(
                            "HTTP/1.1 401 Unauthorized\r\nContent-Type: text/plain; charset=utf-8\r\nX-Exit-Code: 86\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        );
                        let _ = stream.write_all(header.as_bytes());
                        let _ = stream.write_all(body);
                        let _ = stream.flush();
                        let _ = stream.shutdown(Shutdown::Both);
                        continue;
                    };
                    hdr.extend_from_slice(&buf[..hend]);
                    let header_str = String::from_utf8_lossy(&hdr);
                    let mut auth_ok = false;
                    let mut content_len: usize = 0;
                    let mut proto_ok = false;
                    let mut proto_present = false;
                    let mut proto_ver: u8 = 0;
                    for line in header_str.lines() {
                        let l = line.trim();
                        let lower = l.to_ascii_lowercase();
                        if lower.starts_with("authorization:")
                            || lower.starts_with("proxy-authorization:")
                        {
                            if let Some((_, v)) = l.split_once(':') {
                                let value = v.trim();
                                // Accept bare token, or any scheme where the last token (split on whitespace or '=') matches after trimming punctuation
                                if value == token_for_thread2 {
                                    auth_ok = true;
                                } else {
                                    let parts: Vec<&str> = value
                                        .split(|c: char| c.is_whitespace() || c == '=')
                                        .collect();
                                    if let Some(last) = parts.last() {
                                        let last_clean = last.trim_matches(|c: char| {
                                            c == ',' || c == ';' || c == '"' || c == '\''
                                        });
                                        if last_clean == token_for_thread2 {
                                            auth_ok = true;
                                        }
                                    }
                                }
                            }
                        } else if lower.starts_with("content-length:") {
                            if let Some((_, v)) = l.split_once(':') {
                                content_len = v.trim().parse().unwrap_or(0);
                            }
                        } else if lower.starts_with("x-aifo-proto:") {
                            if let Some((_, v)) = l.split_once(':') {
                                proto_present = true;
                                let vt = v.trim();
                                if vt == "1" || vt == "2" {
                                    proto_ok = true;
                                    proto_ver = if vt == "2" { 2 } else { 1 };
                                }
                            }
                        }
                    }
                    // Extract query parameters from Request-Line (e.g., GET /exec?tool=...&arg=...)
                    let mut query_pairs: Vec<(String, String)> = Vec::new();
                    let mut request_path_lc = String::new();
                    if let Some(first_line) = header_str.lines().next() {
                        let mut parts = first_line.split_whitespace();
                        let _method = parts.next();
                        if let Some(target) = parts.next() {
                            let path_only = target.split('?').next().unwrap_or(target);
                            request_path_lc = path_only.to_ascii_lowercase();
                            if let Some(idx) = target.find('?') {
                                let q = &target[idx + 1..];
                                query_pairs.extend(crate::toolchain::parse_form_urlencoded(q));
                            }
                        }
                    }
                    // Read body
                    let mut body = buf[hend..].to_vec();
                    while body.len() < content_len {
                        match stream.read(&mut tmp) {
                            Ok(0) => break,
                            Ok(n) => body.extend_from_slice(&tmp[..n]),
                            Err(_) => break,
                        }
                    }
                    let form = String::from_utf8_lossy(&body).to_string();
                    let mut tool = String::new();
                    let mut cwd = "/workspace".to_string();
                    let mut argv: Vec<String> = Vec::new();
                    for (k, v) in query_pairs
                        .into_iter()
                        .chain(crate::toolchain::parse_form_urlencoded(&form).into_iter())
                    {
                        let kl = k.to_ascii_lowercase();
                        match kl.as_str() {
                            "tool" => tool = v,
                            "cwd" => cwd = v,
                            "arg" => argv.push(v),
                            _ => {}
                        }
                    }
                    if tool.is_empty() {
                        let rp = request_path_lc.as_str();
                        if rp.ends_with("/notifications")
                            || rp.ends_with("/notifications-cmd")
                            || rp.ends_with("/notify")
                            || rp.contains("/notifications")
                            || rp.contains("/notifications-cmd")
                            || rp.contains("/notify")
                        {
                            tool = "notifications-cmd".to_string();
                        }
                    }
                    // Fallback: if tool is still empty, attempt to parse from Request-Target query (?tool=...)
                    // This helps when clients don't send a body or Content-Length is missing.
                    if tool.is_empty() {
                        if let Some(first_line) = header_str.lines().next() {
                            if let Some(idx) = first_line.find("?tool=") {
                                let rest = &first_line[idx + 6..];
                                let end = rest
                                    .find(|c: char| {
                                        c == '&' || c.is_ascii_whitespace() || c == '\r'
                                    })
                                    .unwrap_or(rest.len());
                                let val = &rest[..end];
                                tool = url_decode(val);
                            }
                        }
                    }
                    // Handle notifications endpoint without requiring auth/proto
                    if tool.eq_ignore_ascii_case("notifications-cmd")
                        || form.contains("tool=notifications-cmd")
                        || request_path_lc.contains("/notifications")
                        || request_path_lc.contains("/notifications-cmd")
                        || request_path_lc.contains("/notify")
                    {
                        match crate::toolchain::notifications_handle_request(
                            &argv,
                            verbose,
                            timeout_secs,
                        ) {
                            Ok((status_code, body_out)) => {
                                let header = format!(
                                    "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nX-Exit-Code: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                    status_code,
                                    body_out.len()
                                );
                                let _ = stream.write_all(header.as_bytes());
                                let _ = stream.write_all(&body_out);
                                let _ = stream.flush();
                                let _ = stream.shutdown(Shutdown::Both);
                                continue;
                            }
                            Err(reason) => {
                                let mut body = reason.into_bytes();
                                body.push(b'\n');
                                let header = format!(
                                    "HTTP/1.1 403 Forbidden\r\nContent-Type: text/plain; charset=utf-8\r\nX-Exit-Code: 86\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                    body.len()
                                );
                                let _ = stream.write_all(header.as_bytes());
                                let _ = stream.write_all(&body);
                                let _ = stream.flush();
                                let _ = stream.shutdown(Shutdown::Both);
                                continue;
                            }
                        }
                    }
                    // Fast-path: if tool provided and not permitted by any sidecar allowlist, reject early
                    if !tool.is_empty()
                        && !{
                            let tl = tool.to_ascii_lowercase();
                            ["rust", "node", "python", "c-cpp", "go"]
                                .iter()
                                .any(|k| sidecar_allowlist(k).contains(&tl.as_str()))
                        }
                    {
                        let body = b"forbidden\n";
                        let header = format!("HTTP/1.1 403 Forbidden\r\nContent-Type: text/plain; charset=utf-8\r\nX-Exit-Code: 86\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                        let _ = stream.write_all(header.as_bytes());
                        let _ = stream.write_all(body);
                        let _ = stream.flush();
                        let _ = stream.shutdown(Shutdown::Both);
                        continue;
                    }
                    if tool.is_empty() {
                        // If Authorization is valid, require protocol header X-Aifo-Proto: 1 (426 on missing or wrong). Otherwise, 401 for missing/invalid auth; else 400 for malformed body
                        if auth_ok && (!proto_present || !proto_ok) {
                            let msg = b"Unsupported shim protocol; expected 1 or 2\n";
                            let header = format!(
                                "HTTP/1.1 426 Upgrade Required\r\nContent-Type: text/plain; charset=utf-8\r\nX-Exit-Code: 86\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                msg.len()
                            );
                            let _ = stream.write_all(header.as_bytes());
                            let _ = stream.write_all(msg);
                            let _ = stream.flush();
                            let _ = stream.shutdown(Shutdown::Both);
                            continue;
                        } else if !auth_ok {
                            // Allow notifications endpoint without auth/proto as a special case
                            let is_notif = tool.eq_ignore_ascii_case("notifications-cmd")
                                || form.contains("tool=notifications-cmd")
                                || request_path_lc.contains("/notifications")
                                || request_path_lc.contains("/notifications-cmd")
                                || request_path_lc.contains("/notify");
                            if is_notif {
                                match crate::toolchain::notifications_handle_request(
                                    &argv,
                                    verbose,
                                    timeout_secs,
                                ) {
                                    Ok((status_code, body_out)) => {
                                        let header = format!(
                                            "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nX-Exit-Code: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                            status_code,
                                            body_out.len()
                                        );
                                        let _ = stream.write_all(header.as_bytes());
                                        let _ = stream.write_all(&body_out);
                                        let _ = stream.flush();
                                        let _ = stream.shutdown(Shutdown::Both);
                                        continue;
                                    }
                                    Err(reason) => {
                                        let mut body = reason.into_bytes();
                                        body.push(b'\n');
                                        let header = format!(
                                            "HTTP/1.1 403 Forbidden\r\nContent-Type: text/plain; charset=utf-8\r\nX-Exit-Code: 86\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                            body.len()
                                        );
                                        let _ = stream.write_all(header.as_bytes());
                                        let _ = stream.write_all(&body);
                                        let _ = stream.flush();
                                        let _ = stream.shutdown(Shutdown::Both);
                                        continue;
                                    }
                                }
                            }
                            let body = b"unauthorized\n";
                            let header = format!(
                                "HTTP/1.1 401 Unauthorized\r\nContent-Type: text/plain; charset=utf-8\r\nX-Exit-Code: 86\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                body.len()
                            );
                            let _ = stream.write_all(header.as_bytes());
                            let _ = stream.write_all(body);
                            let _ = stream.flush();
                            let _ = stream.shutdown(Shutdown::Both);
                            continue;
                        } else {
                            let body = b"bad request\n";
                            let header = format!(
                                "HTTP/1.1 400 Bad Request\r\nContent-Type: text/plain; charset=utf-8\r\nX-Exit-Code: 86\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                body.len()
                            );
                            let _ = stream.write_all(header.as_bytes());
                            let _ = stream.write_all(body);
                            let _ = stream.flush();
                            let _ = stream.shutdown(Shutdown::Both);
                            continue;
                        }
                    }
                    if tool.eq_ignore_ascii_case("notifications-cmd")
                        || form.contains("tool=notifications-cmd")
                    {
                        match crate::toolchain::notifications_handle_request(
                            &argv,
                            verbose,
                            timeout_secs,
                        ) {
                            Ok((status_code, body_out)) => {
                                let header = format!(
                                    "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nX-Exit-Code: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                    status_code,
                                    body_out.len()
                                );
                                let _ = stream.write_all(header.as_bytes());
                                let _ = stream.write_all(&body_out);
                                let _ = stream.flush();
                                let _ = stream.shutdown(Shutdown::Both);
                                continue;
                            }
                            Err(reason) => {
                                let mut body = reason.into_bytes();
                                body.push(b'\n');
                                let header = format!(
                                    "HTTP/1.1 403 Forbidden\r\nContent-Type: text/plain; charset=utf-8\r\nX-Exit-Code: 86\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                    body.len()
                                );
                                let _ = stream.write_all(header.as_bytes());
                                let _ = stream.write_all(&body);
                                let _ = stream.flush();
                                let _ = stream.shutdown(Shutdown::Both);
                                continue;
                            }
                        }
                    }
                    let selected_kind =
                        select_kind_for_tool(&session, &tool, timeout_secs, &mut tool_cache);
                    let kind = selected_kind.as_str();
                    let allow = sidecar_allowlist(kind);
                    if !allow.contains(&tool.as_str()) {
                        let body = b"forbidden\n";
                        let header = format!(
                            "HTTP/1.1 403 Forbidden\r\nContent-Type: text/plain; charset=utf-8\r\nX-Exit-Code: 86\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        );
                        let _ = stream.write_all(header.as_bytes());
                        let _ = stream.write_all(body);
                        let _ = stream.flush();
                        let _ = stream.shutdown(Shutdown::Both);
                        continue;
                    }
                    // When Authorization is valid, require X-Aifo-Proto: 1 (426 on missing or wrong). Otherwise, 401 when missing/invalid auth.
                    if auth_ok && (!proto_present || !proto_ok) {
                        let msg = b"Unsupported shim protocol; expected 1 or 2\n";
                        let header = format!(
                            "HTTP/1.1 426 Upgrade Required\r\nContent-Type: text/plain; charset=utf-8\r\nX-Exit-Code: 86\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            msg.len()
                        );
                        let _ = stream.write_all(header.as_bytes());
                        let _ = stream.write_all(msg);
                        let _ = stream.flush();
                        let _ = stream.shutdown(Shutdown::Both);
                        continue;
                    }
                    if !auth_ok {
                        // Allow notifications endpoint without auth/proto as a special case
                        let is_notif = tool.eq_ignore_ascii_case("notifications-cmd")
                            || form.contains("tool=notifications-cmd")
                            || request_path_lc.contains("/notifications")
                            || request_path_lc.contains("/notifications-cmd")
                            || request_path_lc.contains("/notify");
                        if is_notif {
                            match crate::toolchain::notifications_handle_request(
                                &argv,
                                verbose,
                                timeout_secs,
                            ) {
                                Ok((status_code, body_out)) => {
                                    let header = format!(
                                        "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nX-Exit-Code: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                        status_code,
                                        body_out.len()
                                    );
                                    let _ = stream.write_all(header.as_bytes());
                                    let _ = stream.write_all(&body_out);
                                    let _ = stream.flush();
                                    let _ = stream.shutdown(Shutdown::Both);
                                    continue;
                                }
                                Err(reason) => {
                                    let mut body = reason.into_bytes();
                                    body.push(b'\n');
                                    let header = format!(
                                        "HTTP/1.1 403 Forbidden\r\nContent-Type: text/plain; charset=utf-8\r\nX-Exit-Code: 86\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                        body.len()
                                    );
                                    let _ = stream.write_all(header.as_bytes());
                                    let _ = stream.write_all(&body);
                                    let _ = stream.flush();
                                    let _ = stream.shutdown(Shutdown::Both);
                                    continue;
                                }
                            }
                        }
                        let body = b"unauthorized\n";
                        let header = format!(
                            "HTTP/1.1 401 Unauthorized\r\nContent-Type: text/plain; charset=utf-8\r\nX-Exit-Code: 86\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        );
                        let _ = stream.write_all(header.as_bytes());
                        let _ = stream.write_all(body);
                        let _ = stream.flush();
                        let _ = stream.shutdown(Shutdown::Both);
                        continue;
                    }
                    let name = sidecar_container_name(kind, &session);
                    let pwd = PathBuf::from(cwd);
                    if verbose {
                        let _ = std::io::stdout().flush();
                        let _ = std::io::stderr().flush();
                        eprintln!(
                            "\r\x1b[2Kaifo-coder: proxy exec: tool={} args={:?} cwd={}",
                            tool,
                            argv,
                            pwd.display()
                        );
                    }
                    // If selected sidecar isn't running and no alternative was available, return a helpful error
                    if !container_exists(&name) {
                        let msg = format!(
                            "tool '{}' not available in running sidecars; start an appropriate toolchain (e.g., --toolchain c-cpp or --toolchain rust)\n",
                            tool
                        );
                        let header = format!(
                            "HTTP/1.1 409 Conflict\r\nContent-Type: text/plain; charset=utf-8\r\nX-Exit-Code: 86\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            msg.len()
                        );
                        let _ = stream.write_all(header.as_bytes());
                        let _ = stream.write_all(msg.as_bytes());
                        let _ = stream.flush();
                        let _ = stream.shutdown(Shutdown::Both);
                        continue;
                    }
                    let mut full_args: Vec<String>;
                    if tool == "tsc" {
                        let nm_tsc = pwd.join("node_modules").join(".bin").join("tsc");
                        if nm_tsc.exists() {
                            full_args = vec!["./node_modules/.bin/tsc".to_string()];
                            full_args.extend(argv.clone());
                        } else {
                            full_args = vec!["npx".to_string(), "tsc".to_string()];
                            full_args.extend(argv.clone());
                        }
                    } else {
                        full_args = vec![tool.clone()];
                        full_args.extend(argv.clone());
                    }
                    let exec_preview_args = build_sidecar_exec_preview(
                        &name,
                        if cfg!(unix) { Some((uid, gid)) } else { None },
                        &pwd,
                        kind,
                        &full_args,
                    );
                    if verbose {
                        let _ = std::io::stdout().flush();
                        let _ = std::io::stderr().flush();
                        eprintln!("\r\x1b[2Kaifo-coder: proxy docker:");
                        eprintln!("\r\x1b[2K  {}", shell_join(&exec_preview_args));
                    }
                    // If client requested streaming (protocol v2), stream chunked output with exit code as trailer
                    if proto_present && proto_ok && proto_ver == 2 {
                        let hdr = b"HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nTransfer-Encoding: chunked\r\nTrailer: X-Exit-Code\r\nConnection: close\r\n\r\n";
                        let _ = stream.write_all(hdr);
                        let _ = stream.flush();
                        let started = std::time::Instant::now();

                        let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
                        let runtime_cl = runtime.clone();
                        // Rebuild args to wrap user command in 'sh -lc "<cmd> 2>&1"' to preserve output ordering
                        let mut spawn_args: Vec<String> = Vec::new();
                        let mut idx = None;
                        for (i, a) in exec_preview_args.iter().enumerate().skip(1) {
                            if a == &name {
                                idx = Some(i);
                                break;
                            }
                        }
                        let idx = idx.unwrap_or(exec_preview_args.len().saturating_sub(1));
                        // Up to and including container name
                        spawn_args.extend(exec_preview_args[1..=idx].iter().cloned());
                        // User command slice after container name
                        let user_slice: Vec<String> = exec_preview_args[idx + 1..].to_vec();
                        let script = {
                            let s = shell_join(&user_slice);
                            format!("{} 2>&1", s)
                        };
                        spawn_args.push("sh".to_string());
                        spawn_args.push("-lc".to_string());
                        spawn_args.push(script);

                        let mut cmd = Command::new(&runtime_cl);
                        for a in &spawn_args {
                            cmd.arg(a);
                        }
                        cmd.stdout(Stdio::piped());
                        cmd.stderr(Stdio::piped());
                        let mut child = match cmd.spawn() {
                            Ok(c) => c,
                            Err(e) => {
                                let _ = write!(stream, "0\r\nX-Exit-Code: 1\r\n\r\n");
                                let _ = stream.flush();
                                let _ = stream.shutdown(Shutdown::Both);
                                eprintln!("aifo-coder: proxy spawn error: {}", e);
                                continue;
                            }
                        };

                        if let Some(mut so) = child.stdout.take() {
                            let txo = tx.clone();
                            std::thread::spawn(move || {
                                let mut buf = [0u8; 8192];
                                loop {
                                    match so.read(&mut buf) {
                                        Ok(0) => break,
                                        Ok(n) => {
                                            let _ = txo.send(buf[..n].to_vec());
                                        }
                                        Err(_) => break,
                                    }
                                }
                            });
                        }
                        // stderr merged into stdout via '2>&1'; no separate reader

                        // Drain chunks and forward to client
                        drop(tx);
                        while let Ok(chunk) = rx.recv() {
                            if !chunk.is_empty() {
                                let _ = write!(stream, "{:X}\r\n", chunk.len());
                                let _ = stream.write_all(&chunk);
                                let _ = stream.write_all(b"\r\n");
                                let _ = stream.flush();
                            }
                        }

                        let code = child.wait().ok().and_then(|s| s.code()).unwrap_or(1);
                        let dur_ms = started.elapsed().as_millis();
                        if verbose {
                            let _ = std::io::stdout().flush();
                            let _ = std::io::stderr().flush();
                            eprintln!(
                                "\r\x1b[2Kaifo-coder: proxy result tool={} kind={} code={} dur_ms={}",
                                tool, kind, code, dur_ms
                            );
                        }
                        // Final chunk + trailer with exit code
                        let _ = stream.write_all(b"0\r\n");
                        let trailer = format!("X-Exit-Code: {}\r\n\r\n", code);
                        let _ = stream.write_all(trailer.as_bytes());
                        let _ = stream.flush();
                        let _ = stream.shutdown(Shutdown::Both);
                        continue;
                    }

                    let started = std::time::Instant::now();
                    let (status_code, body_out) = {
                        let (tx, rx) = std::sync::mpsc::channel();
                        let runtime_cl = runtime.clone();
                        let args_clone: Vec<String> = exec_preview_args[1..].to_vec();
                        std::thread::spawn(move || {
                            let mut cmd = Command::new(&runtime_cl);
                            for a in &args_clone {
                                cmd.arg(a);
                            }
                            let out = cmd.output();
                            let _ = tx.send(out);
                        });
                        match rx.recv_timeout(Duration::from_secs(timeout_secs)) {
                            Ok(Ok(o)) => {
                                let code = o.status.code().unwrap_or(1);
                                let mut b = o.stdout;
                                if !o.stderr.is_empty() {
                                    b.extend_from_slice(&o.stderr);
                                }
                                (code, b)
                            }
                            Ok(Err(e)) => {
                                let mut b = format!("aifo-coder proxy error: {}", e).into_bytes();
                                b.push(b'\n');
                                (1, b)
                            }
                            Err(_timeout) => {
                                let msg = b"aifo-coder proxy timeout\n";
                                let header = format!(
                                    "HTTP/1.1 504 Gateway Timeout\r\nContent-Type: text/plain; charset=utf-8\r\nX-Exit-Code: 124\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                    msg.len()
                                );
                                let _ = stream.write_all(header.as_bytes());
                                let _ = stream.write_all(msg);
                                let _ = stream.flush();
                                let _ = stream.shutdown(Shutdown::Both);
                                continue;
                            }
                        }
                    };
                    let dur_ms = started.elapsed().as_millis();
                    if verbose {
                        let _ = std::io::stdout().flush();
                        let _ = std::io::stderr().flush();
                        eprintln!(
                            "\r\x1b[2Kaifo-coder: proxy result tool={} kind={} code={} dur_ms={}",
                            tool, kind, status_code, dur_ms
                        );
                    }
                    let mut body_bytes = body_out;
                    if verbose {
                        if !body_bytes.starts_with(b"\n") && !body_bytes.starts_with(b"\r") {
                            let mut pref = Vec::with_capacity(body_bytes.len() + 1);
                            pref.push(b'\n');
                            pref.extend_from_slice(&body_bytes);
                            body_bytes = pref;
                        }
                        if !body_bytes.ends_with(b"\n") && !body_bytes.ends_with(b"\r") {
                            body_bytes.push(b'\n');
                        }
                    }
                    let header = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nX-Exit-Code: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        status_code,
                        body_bytes.len()
                    );
                    let _ = stream.write_all(header.as_bytes());
                    let _ = stream.write_all(&body_bytes);
                    let _ = stream.flush();
                    let _ = stream.shutdown(Shutdown::Both);
                }
            });
            let url = "unix:///run/aifo/toolexec.sock".to_string();
            return Ok((url, token, running, handle));
        }
    }
    // Bind address by OS: 0.0.0.0 on Linux (containers connect), 127.0.0.1 on macOS/Windows
    let bind_host: &str = if cfg!(target_os = "linux") {
        "0.0.0.0"
    } else {
        "127.0.0.1"
    };
    let listener = TcpListener::bind((bind_host, 0))
        .map_err(|e| io::Error::new(e.kind(), format!("proxy bind failed: {e}")))?;
    let addr = listener
        .local_addr()
        .map_err(|e| io::Error::new(e.kind(), format!("proxy addr failed: {e}")))?;
    let port = addr.port();
    let _ = listener.set_nonblocking(true);
    let running_cl = running.clone();

    let handle = std::thread::spawn(move || {
        if verbose {
            eprintln!(
                "aifo-coder: toolexec proxy listening on {}:{port}",
                bind_host
            );
        }
        let mut tool_cache: HashMap<(String, String), bool> = HashMap::new();
        loop {
            if !running_cl.load(std::sync::atomic::Ordering::SeqCst) {
                break;
            }
            let mut stream = match listener.accept() {
                Ok((s, _addr)) => s,
                Err(e) => {
                    if e.kind() == io::ErrorKind::WouldBlock {
                        std::thread::sleep(Duration::from_millis(50));
                        continue;
                    } else {
                        continue;
                    }
                }
            };
            let _ = stream.set_read_timeout(Some(Duration::from_secs(timeout_secs)));
            let _ = stream.set_write_timeout(None);
            // Read request (simple HTTP)
            let mut buf = Vec::new();
            let mut hdr = Vec::new();
            let mut tmp = [0u8; 1024];
            // Read until end of headers
            let mut header_end = None;
            while header_end.is_none() {
                match stream.read(&mut tmp) {
                    Ok(0) => break,
                    Ok(n) => {
                        buf.extend_from_slice(&tmp[..n]);
                        if let Some(end) = find_header_end(&buf) {
                            header_end = Some(end);
                        } else if let Some(pos) = buf.windows(2).position(|w| w == b"\n\n") {
                            // Be tolerant to LF-only header termination used by some simple clients/tests
                            header_end = Some(pos);
                        }
                        // avoid overly large header
                        if buf.len() > 64 * 1024 {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            let hend = if let Some(h) = header_end {
                h
            } else if !buf.is_empty() {
                // Tolerate missing CRLFCRLF for simple clients: treat entire buffer as headers
                buf.len()
            } else {
                let body = b"unauthorized\n";
                let header = format!(
                    "HTTP/1.1 401 Unauthorized\r\nContent-Type: text/plain; charset=utf-8\r\nX-Exit-Code: 86\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(header.as_bytes());
                let _ = stream.write_all(body);
                let _ = stream.flush();
                let _ = stream.shutdown(Shutdown::Both);
                continue;
            };
            hdr.extend_from_slice(&buf[..hend]);
            let header_str = String::from_utf8_lossy(&hdr);
            let mut auth_ok = false;
            let mut content_len: usize = 0;
            let mut proto_ok = false;
            let mut proto_present = false;
            let mut proto_ver: u8 = 0;
            for line in header_str.lines() {
                let l = line.trim();
                let lower = l.to_ascii_lowercase();
                if lower.starts_with("authorization:") || lower.starts_with("proxy-authorization:")
                {
                    if let Some((_, v)) = l.split_once(':') {
                        let value = v.trim();
                        // Accept bare token, or any scheme where the last token (split on whitespace or '=') matches after trimming punctuation
                        if value == token_for_thread {
                            auth_ok = true;
                        } else {
                            let parts: Vec<&str> = value
                                .split(|c: char| c.is_whitespace() || c == '=')
                                .collect();
                            if let Some(last) = parts.last() {
                                let last_clean = last.trim_matches(|c: char| {
                                    c == ',' || c == ';' || c == '"' || c == '\''
                                });
                                if last_clean == token_for_thread {
                                    auth_ok = true;
                                }
                            }
                        }
                    }
                } else if lower.starts_with("content-length:") {
                    if let Some((_, v)) = l.split_once(':') {
                        content_len = v.trim().parse().unwrap_or(0);
                    }
                } else if lower.starts_with("x-aifo-proto:") {
                    if let Some((_, v)) = l.split_once(':') {
                        proto_present = true;
                        let vt = v.trim();
                        if vt == "1" || vt == "2" {
                            proto_ok = true;
                            proto_ver = if vt == "2" { 2 } else { 1 };
                        }
                    }
                }
            }
            // Extract query parameters from Request-Line (e.g., GET /exec?tool=...&arg=...)
            let mut query_pairs: Vec<(String, String)> = Vec::new();
            let mut request_path_lc = String::new();
            if let Some(first_line) = header_str.lines().next() {
                let mut parts = first_line.split_whitespace();
                let _method = parts.next();
                if let Some(target) = parts.next() {
                    let path_only = target.split('?').next().unwrap_or(target);
                    request_path_lc = path_only.to_ascii_lowercase();
                    if let Some(idx) = target.find('?') {
                        let q = &target[idx + 1..];
                        query_pairs.extend(crate::toolchain::parse_form_urlencoded(q));
                    }
                }
            }
            // Read body
            let mut body = buf[hend..].to_vec();
            while body.len() < content_len {
                match stream.read(&mut tmp) {
                    Ok(0) => break,
                    Ok(n) => body.extend_from_slice(&tmp[..n]),
                    Err(_) => break,
                }
            }
            let form = String::from_utf8_lossy(&body).to_string();
            let mut tool = String::new();
            let mut cwd = "/workspace".to_string();
            let mut argv: Vec<String> = Vec::new();
            for (k, v) in query_pairs
                .into_iter()
                .chain(crate::toolchain::parse_form_urlencoded(&form).into_iter())
            {
                let kl = k.to_ascii_lowercase();
                match kl.as_str() {
                    "tool" => tool = v,
                    "cwd" => cwd = v,
                    "arg" => argv.push(v),
                    _ => {}
                }
            }
            if tool.is_empty() {
                let rp = request_path_lc.as_str();
                if rp.ends_with("/notifications")
                    || rp.ends_with("/notifications-cmd")
                    || rp.ends_with("/notify")
                    || rp.contains("/notifications")
                    || rp.contains("/notifications-cmd")
                    || rp.contains("/notify")
                {
                    tool = "notifications-cmd".to_string();
                }
            }
            // Fallback: if tool is still empty, attempt to parse from Request-Target query (?tool=...)
            // This helps when clients don't send a body or Content-Length is missing.
            if tool.is_empty() {
                if let Some(first_line) = header_str.lines().next() {
                    if let Some(idx) = first_line.find("?tool=") {
                        let rest = &first_line[idx + 6..];
                        let end = rest
                            .find(|c: char| c == '&' || c.is_ascii_whitespace() || c == '\r')
                            .unwrap_or(rest.len());
                        let val = &rest[..end];
                        tool = url_decode(val);
                    }
                }
            }
            // Handle notifications endpoint without requiring auth/proto
            if tool.eq_ignore_ascii_case("notifications-cmd")
                || form.contains("tool=notifications-cmd")
                || request_path_lc.contains("/notifications")
                || request_path_lc.contains("/notifications-cmd")
                || request_path_lc.contains("/notify")
            {
                match crate::toolchain::notifications_handle_request(&argv, verbose, timeout_secs) {
                    Ok((status_code, body_out)) => {
                        let header = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nX-Exit-Code: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            status_code,
                            body_out.len()
                        );
                        let _ = stream.write_all(header.as_bytes());
                        let _ = stream.write_all(&body_out);
                        let _ = stream.flush();
                        let _ = stream.shutdown(Shutdown::Both);
                        continue;
                    }
                    Err(reason) => {
                        let mut body = reason.into_bytes();
                        body.push(b'\n');
                        let header = format!(
                            "HTTP/1.1 403 Forbidden\r\nContent-Type: text/plain; charset=utf-8\r\nX-Exit-Code: 86\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        );
                        let _ = stream.write_all(header.as_bytes());
                        let _ = stream.write_all(&body);
                        let _ = stream.flush();
                        let _ = stream.shutdown(Shutdown::Both);
                        continue;
                    }
                }
            }
            // Fast-path: if tool provided and not permitted by any sidecar allowlist, reject early
            if !tool.is_empty()
                && !{
                    let tl = tool.to_ascii_lowercase();
                    ["rust", "node", "python", "c-cpp", "go"]
                        .iter()
                        .any(|k| sidecar_allowlist(k).contains(&tl.as_str()))
                }
            {
                let body = b"forbidden\n";
                let header = format!("HTTP/1.1 403 Forbidden\r\nContent-Type: text/plain; charset=utf-8\r\nX-Exit-Code: 86\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                let _ = stream.write_all(header.as_bytes());
                let _ = stream.write_all(body);
                let _ = stream.flush();
                let _ = stream.shutdown(Shutdown::Both);
                continue;
            }
            if tool.is_empty() {
                // If Authorization is valid, require protocol header X-Aifo-Proto: 1 (426 on missing or wrong). Otherwise, 401 for missing/invalid auth; else 400 for malformed body
                if auth_ok && (!proto_present || !proto_ok) {
                    let msg = b"Unsupported shim protocol; expected 1 or 2\n";
                    let header = format!(
                        "HTTP/1.1 426 Upgrade Required\r\nContent-Type: text/plain; charset=utf-8\r\nX-Exit-Code: 86\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        msg.len()
                    );
                    let _ = stream.write_all(header.as_bytes());
                    let _ = stream.write_all(msg);
                    let _ = stream.flush();
                    let _ = stream.shutdown(Shutdown::Both);
                    continue;
                } else if !auth_ok {
                    let body = b"unauthorized\n";
                    let header = format!(
                        "HTTP/1.1 401 Unauthorized\r\nContent-Type: text/plain; charset=utf-8\r\nX-Exit-Code: 86\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = stream.write_all(header.as_bytes());
                    let _ = stream.write_all(body);
                    let _ = stream.flush();
                    let _ = stream.shutdown(Shutdown::Both);
                    continue;
                } else {
                    let body = b"bad request\n";
                    let header = format!(
                        "HTTP/1.1 400 Bad Request\r\nContent-Type: text/plain; charset=utf-8\r\nX-Exit-Code: 86\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = stream.write_all(header.as_bytes());
                    let _ = stream.write_all(body);
                    let _ = stream.flush();
                    let _ = stream.shutdown(Shutdown::Both);
                    continue;
                }
            }
            if tool.eq_ignore_ascii_case("notifications-cmd")
                || form.contains("tool=notifications-cmd")
            {
                match crate::toolchain::notifications_handle_request(&argv, verbose, timeout_secs) {
                    Ok((status_code, body_out)) => {
                        let header = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nX-Exit-Code: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            status_code,
                            body_out.len()
                        );
                        let _ = stream.write_all(header.as_bytes());
                        let _ = stream.write_all(&body_out);
                        let _ = stream.flush();
                        let _ = stream.shutdown(Shutdown::Both);
                        continue;
                    }
                    Err(reason) => {
                        let mut body = reason.into_bytes();
                        body.push(b'\n');
                        let header = format!(
                            "HTTP/1.1 403 Forbidden\r\nContent-Type: text/plain; charset=utf-8\r\nX-Exit-Code: 86\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        );
                        let _ = stream.write_all(header.as_bytes());
                        let _ = stream.write_all(&body);
                        let _ = stream.flush();
                        let _ = stream.shutdown(Shutdown::Both);
                        continue;
                    }
                }
            }
            let selected_kind =
                select_kind_for_tool(&session, &tool, timeout_secs, &mut tool_cache);
            let kind = selected_kind.as_str();
            let allow = sidecar_allowlist(kind);
            if !allow.contains(&tool.as_str()) {
                let body = b"forbidden\n";
                let header = format!(
                    "HTTP/1.1 403 Forbidden\r\nContent-Type: text/plain; charset=utf-8\r\nX-Exit-Code: 86\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(header.as_bytes());
                let _ = stream.write_all(body);
                let _ = stream.flush();
                let _ = stream.shutdown(Shutdown::Both);
                continue;
            }
            // When Authorization is valid, require X-Aifo-Proto: 1 (426 on missing or wrong). Otherwise, 401 when missing/invalid auth.
            if auth_ok && (!proto_present || !proto_ok) {
                let msg = b"Unsupported shim protocol; expected 1 or 2\n";
                let header = format!(
                    "HTTP/1.1 426 Upgrade Required\r\nContent-Type: text/plain; charset=utf-8\r\nX-Exit-Code: 86\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    msg.len()
                );
                let _ = stream.write_all(header.as_bytes());
                let _ = stream.write_all(msg);
                let _ = stream.flush();
                let _ = stream.shutdown(Shutdown::Both);
                continue;
            }
            if !auth_ok {
                let body = b"unauthorized\n";
                let header = format!(
                    "HTTP/1.1 401 Unauthorized\r\nContent-Type: text/plain; charset=utf-8\r\nX-Exit-Code: 86\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(header.as_bytes());
                let _ = stream.write_all(body);
                let _ = stream.flush();
                let _ = stream.shutdown(Shutdown::Both);
                continue;
            }
            let name = sidecar_container_name(kind, &session);
            // If selected sidecar isn't running and no alternative was available, return a helpful error
            if !container_exists(&name) {
                let msg = format!(
                    "tool '{}' not available in running sidecars; start an appropriate toolchain (e.g., --toolchain c-cpp or --toolchain rust)\n",
                    tool
                );
                let header = format!(
                    "HTTP/1.1 409 Conflict\r\nContent-Type: text/plain; charset=utf-8\r\nX-Exit-Code: 86\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    msg.len()
                );
                let _ = stream.write_all(header.as_bytes());
                let _ = stream.write_all(msg.as_bytes());
                let _ = stream.flush();
                let _ = stream.shutdown(Shutdown::Both);
                continue;
            }
            let pwd = PathBuf::from(cwd);
            if verbose {
                let _ = std::io::stdout().flush();
                let _ = std::io::stderr().flush();
                eprintln!(
                    "\r\x1b[2Kaifo-coder: proxy exec: tool={} args={:?} cwd={}",
                    tool,
                    argv,
                    pwd.display()
                );
            }
            let mut full_args: Vec<String>;
            if tool == "tsc" {
                let nm_tsc = pwd.join("node_modules").join(".bin").join("tsc");
                if nm_tsc.exists() {
                    full_args = vec!["./node_modules/.bin/tsc".to_string()];
                    full_args.extend(argv.clone());
                } else {
                    full_args = vec!["npx".to_string(), "tsc".to_string()];
                    full_args.extend(argv.clone());
                }
            } else {
                full_args = vec![tool.clone()];
                full_args.extend(argv.clone());
            }

            let exec_preview_args = build_sidecar_exec_preview(
                &name,
                if cfg!(unix) { Some((uid, gid)) } else { None },
                &pwd,
                kind,
                &full_args,
            );
            if verbose {
                let _ = std::io::stdout().flush();
                let _ = std::io::stderr().flush();
                eprintln!("\r\x1b[2Kaifo-coder: proxy docker:");
                eprintln!("\r\x1b[2K  {}", shell_join(&exec_preview_args));
            }
            // If client requested streaming (protocol v2), stream chunked output with exit code as trailer
            if proto_present && proto_ok && proto_ver == 2 {
                let hdr = b"HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nTransfer-Encoding: chunked\r\nTrailer: X-Exit-Code\r\nConnection: close\r\n\r\n";
                let _ = stream.write_all(hdr);
                let _ = stream.flush();
                let started = std::time::Instant::now();

                let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
                let runtime_cl = runtime.clone();
                // Rebuild args to wrap user command in 'sh -lc "<cmd> 2>&1"' to preserve output ordering
                let mut spawn_args: Vec<String> = Vec::new();
                let mut idx = None;
                for (i, a) in exec_preview_args.iter().enumerate().skip(1) {
                    if a == &name {
                        idx = Some(i);
                        break;
                    }
                }
                let idx = idx.unwrap_or(exec_preview_args.len().saturating_sub(1));
                // Up to and including container name
                spawn_args.extend(exec_preview_args[1..=idx].iter().cloned());
                // User command slice after container name
                let user_slice: Vec<String> = exec_preview_args[idx + 1..].to_vec();
                let script = {
                    let s = shell_join(&user_slice);
                    format!("{} 2>&1", s)
                };
                spawn_args.push("sh".to_string());
                spawn_args.push("-lc".to_string());
                spawn_args.push(script);

                let mut cmd = Command::new(&runtime_cl);
                for a in &spawn_args {
                    cmd.arg(a);
                }
                cmd.stdout(Stdio::piped());
                cmd.stderr(Stdio::piped());
                let mut child = match cmd.spawn() {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = write!(stream, "0\r\nX-Exit-Code: 1\r\n\r\n");
                        let _ = stream.flush();
                        let _ = stream.shutdown(Shutdown::Both);
                        eprintln!("aifo-coder: proxy spawn error: {}", e);
                        continue;
                    }
                };

                if let Some(mut so) = child.stdout.take() {
                    let txo = tx.clone();
                    std::thread::spawn(move || {
                        let mut buf = [0u8; 8192];
                        loop {
                            match so.read(&mut buf) {
                                Ok(0) => break,
                                Ok(n) => {
                                    let _ = txo.send(buf[..n].to_vec());
                                }
                                Err(_) => break,
                            }
                        }
                    });
                }
                // stderr merged into stdout via '2>&1'; no separate reader

                // Drain chunks and forward to client
                drop(tx);
                while let Ok(chunk) = rx.recv() {
                    if !chunk.is_empty() {
                        let _ = write!(stream, "{:X}\r\n", chunk.len());
                        let _ = stream.write_all(&chunk);
                        let _ = stream.write_all(b"\r\n");
                        let _ = stream.flush();
                    }
                }

                let code = child.wait().ok().and_then(|s| s.code()).unwrap_or(1);
                let dur_ms = started.elapsed().as_millis();
                if verbose {
                    let _ = std::io::stdout().flush();
                    let _ = std::io::stderr().flush();
                    eprintln!(
                        "\r\x1b[2Kaifo-coder: proxy result tool={} kind={} code={} dur_ms={}",
                        tool, kind, code, dur_ms
                    );
                }
                // Final chunk + trailer with exit code
                let _ = stream.write_all(b"0\r\n");
                let trailer = format!("X-Exit-Code: {}\r\n\r\n", code);
                let _ = stream.write_all(trailer.as_bytes());
                let _ = stream.flush();
                let _ = stream.shutdown(Shutdown::Both);
                continue;
            }

            let started = std::time::Instant::now();
            let (status_code, body_out) = {
                let (tx, rx) = std::sync::mpsc::channel();
                let runtime_cl = runtime.clone();
                let args_clone: Vec<String> = exec_preview_args[1..].to_vec();
                std::thread::spawn(move || {
                    let mut cmd = Command::new(&runtime_cl);
                    for a in &args_clone {
                        cmd.arg(a);
                    }
                    let out = cmd.output();
                    let _ = tx.send(out);
                });
                match rx.recv_timeout(Duration::from_secs(timeout_secs)) {
                    Ok(Ok(o)) => {
                        let code = o.status.code().unwrap_or(1);
                        let mut b = o.stdout;
                        if !o.stderr.is_empty() {
                            b.extend_from_slice(&o.stderr);
                        }
                        (code, b)
                    }
                    Ok(Err(e)) => {
                        let mut b = format!("aifo-coder proxy error: {}", e).into_bytes();
                        b.push(b'\n');
                        (1, b)
                    }
                    Err(_timeout) => {
                        let msg = b"aifo-coder proxy timeout\n";
                        let header = format!(
                            "HTTP/1.1 504 Gateway Timeout\r\nContent-Type: text/plain; charset=utf-8\r\nX-Exit-Code: 124\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            msg.len()
                        );
                        let _ = stream.write_all(header.as_bytes());
                        let _ = stream.write_all(msg);
                        let _ = stream.flush();
                        let _ = stream.shutdown(Shutdown::Both);
                        continue;
                    }
                }
            };
            let dur_ms = started.elapsed().as_millis();
            if verbose {
                let _ = std::io::stdout().flush();
                let _ = std::io::stderr().flush();
                eprintln!(
                    "\r\x1b[2Kaifo-coder: proxy result tool={} kind={} code={} dur_ms={}",
                    tool, kind, status_code, dur_ms
                );
            }
            let mut body_bytes = body_out;
            if verbose {
                if !body_bytes.starts_with(b"\n") && !body_bytes.starts_with(b"\r") {
                    let mut pref = Vec::with_capacity(body_bytes.len() + 1);
                    pref.push(b'\n');
                    pref.extend_from_slice(&body_bytes);
                    body_bytes = pref;
                }
                if !body_bytes.ends_with(b"\n") && !body_bytes.ends_with(b"\r") {
                    body_bytes.push(b'\n');
                }
            }
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nX-Exit-Code: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                status_code,
                body_bytes.len()
            );
            let _ = stream.write_all(header.as_bytes());
            let _ = stream.write_all(&body_bytes);
            let _ = stream.flush();
            let _ = stream.shutdown(Shutdown::Both);
        }
        if verbose {
            eprintln!("aifo-coder: toolexec proxy stopped");
        }
    });
    // On macOS/Windows, host.docker.internal resolves; on Linux we add host-gateway and still use host.docker.internal
    let url = format!("http://host.docker.internal:{}/exec", port);
    Ok((url, token, running, handle))
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
    if let Ok(dir) = env::var("AIFO_TOOLEEXEC_UNIX_DIR") {
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
