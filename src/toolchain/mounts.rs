/*!
Mounts and ownership initialization for sidecars.

- push_mount: add -v mounts
- init_rust_named_volumes_if_needed: one-shot chown for rust named volumes (registry/git)
*/
use std::path::Path;
use std::process::{Command, Stdio};

use crate::shell_join;

/// Push a volume mount (-v host:container) into docker args.
pub(crate) fn push_mount(args: &mut Vec<String>, spec: &str) {
    args.push("-v".to_string());
    args.push(spec.to_string());
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
    let use_err = aifo_coder::color_enabled_stderr();
    let mount = format!("aifo-cargo-{subdir}:/home/coder/.cargo/{subdir}");
    let script = format!(
        "set -e; d=\"/home/coder/.cargo/{sd}\"; if [ -f \"$d/.aifo-init-done\" ]; then exit 0; fi; mkdir -p \"$d\"; chown -R {uid}:{gid} \"$d\" || true; printf '%s\\n' '{uid}:{gid}' > \"$d/.aifo-init-done\" || true",
        sd = subdir,
        uid = uid,
        gid = gid
    );
    let args: Vec<String> = vec![
        "docker".into(),
        "run".into(),
        "--rm".into(),
        "-v".into(),
        mount,
        image.into(),
        "sh".into(),
        "-lc".into(),
        script,
    ];
    if verbose {
        aifo_coder::log_info_stderr(
            use_err,
            &format!("aifo-coder: docker: {}", shell_join(&args)),
        );
    }
    let mut cmd = Command::new(runtime);
    for a in &args[1..] {
        cmd.arg(a);
    }
    if !verbose {
        cmd.stdout(Stdio::null()).stderr(Stdio::null());
    }
    let _ = cmd.status();
}

/// Inspect run-args and initialize named rust cargo volumes when they are selected (registry/git).
pub(crate) fn init_rust_named_volumes_if_needed(
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

/// Best-effort ownership initialization for the consolidated Node cache volume.
/// Runs a short helper container that ensures /home/coder/.cache exists, chowns to uid:gid,
/// and stamps the directory to avoid repeated work.
pub(crate) fn init_node_cache_volume(
    runtime: &Path,
    image: &str,
    uid: u32,
    gid: u32,
    verbose: bool,
) {
    let use_err = aifo_coder::color_enabled_stderr();
    let mount = "aifo-node-cache:/home/coder/.cache".to_string();
    let script = format!(
        "set -e; d=\"/home/coder/.cache\"; if [ -f \"$d/.aifo-init-done\" ]; then exit 0; fi; mkdir -p \"$d\"; chown -R {uid}:{gid} \"$d\" || true; printf '%s\\n' '{uid}:{gid}' > \"$d/.aifo-init-done\" || true",
        uid = uid,
        gid = gid
    );
    let args: Vec<String> = vec![
        "docker".into(),
        "run".into(),
        "--rm".into(),
        "-v".into(),
        mount,
        image.into(),
        "sh".into(),
        "-lc".into(),
        script,
    ];
    if verbose {
        aifo_coder::log_info_stderr(
            use_err,
            &format!("aifo-coder: docker: {}", shell_join(&args)),
        );
    }
    let mut cmd = Command::new(runtime);
    for a in &args[1..] {
        cmd.arg(a);
    }
    if !verbose {
        cmd.stdout(Stdio::null()).stderr(Stdio::null());
    }
    let _ = cmd.status();
}

/// Inspect run-args and initialize the node cache volume when selected.
pub(crate) fn init_node_cache_volume_if_needed(
    runtime: &Path,
    image: &str,
    run_args: &[String],
    uidgid: Option<(u32, u32)>,
    verbose: bool,
) {
    let mut need_node_cache = false;
    let mut i = 0usize;
    while i + 1 < run_args.len() {
        if run_args[i] == "-v" {
            let mnt = &run_args[i + 1];
            if mnt.starts_with("aifo-node-cache:") && mnt.ends_with("/home/coder/.cache") {
                need_node_cache = true;
            }
            i += 2;
        } else {
            i += 1;
        }
    }
    if !need_node_cache {
        return;
    }
    let (uid, gid) = uidgid.unwrap_or((0u32, 0u32));
    init_node_cache_volume(runtime, image, uid, gid, verbose);
}
