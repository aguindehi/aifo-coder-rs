/*!
Mounts and ownership initialization for sidecars.

- push_mount: add -v mounts
- init_rust_named_volumes_if_needed: one-shot chown for rust named volumes (registry/git)
*/
use std::path::Path;
use std::process::{Command, Stdio};

use crate::{shell_escape, shell_join, ShellScript};

/// Push a volume mount (-v host:container) into docker args.
pub(crate) fn push_mount(args: &mut Vec<String>, spec: &str) {
    args.push("-v".to_string());
    args.push(spec.to_string());
}

fn init_named_volume_with_stamp(
    runtime: &Path,
    image: &str,
    mount_spec: &str,
    dir_in_container: &str,
    uid: u32,
    gid: u32,
    verbose: bool,
) {
    let use_err = crate::color_enabled_stderr();

    // dir_in_container is constant for our use-cases; keep script fixed-shape to avoid injection.
    let mut sh = ShellScript::new();
    sh.push("set -e".to_string());
    sh.push(format!("d={}", shell_escape(dir_in_container)));
    sh.push(r#"if [ -f "$d/.aifo-init-done" ]; then exit 0; fi"#.to_string());
    sh.push(r#"mkdir -p "$d""#.to_string());
    sh.push(format!(r#"chown -R {uid}:{gid} "$d" || true"#));
    sh.push(format!(
        r#"printf '%s\n' '{uid}:{gid}' > "$d/.aifo-init-done" || true"#,
    ));
    let script = match sh.build() {
        Ok(s) => s,
        Err(e) => {
            if verbose {
                crate::log_warn_stderr(
                    use_err,
                    &format!(
                        "aifo-coder: warning: refusing to run invalid shell init script: {}",
                        e
                    ),
                );
            }
            return;
        }
    };

    let args: Vec<String> = vec![
        "docker".into(),
        "run".into(),
        "--rm".into(),
        "-v".into(),
        mount_spec.to_string(),
        image.into(),
        "sh".into(),
        "-c".into(),
        script,
    ];

    if verbose {
        crate::log_info_stderr(
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
    let mount = format!("aifo-cargo-{subdir}:/home/coder/.cargo/{subdir}");
    let dir = format!("/home/coder/.cargo/{subdir}");
    init_named_volume_with_stamp(runtime, image, &mount, &dir, uid, gid, verbose);
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
                && (mnt.ends_with("/home/coder/.cargo/registry")
                    || mnt.ends_with("/usr/local/cargo/registry"))
            {
                need_registry = true;
            } else if mnt.starts_with("aifo-cargo-git:")
                && (mnt.ends_with("/home/coder/.cargo/git")
                    || mnt.ends_with("/usr/local/cargo/git"))
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

///// Best-effort ownership initialization for the consolidated Node cache volume.
///// Runs a short helper container that ensures /home/coder/.cache exists, chowns to uid:gid,
///// and stamps the directory to avoid repeated work.
fn init_node_cache_volume(runtime: &Path, image: &str, uid: u32, gid: u32, verbose: bool) {
    init_named_volume_with_stamp(
        runtime,
        image,
        "aifo-node-cache:/home/coder/.cache",
        "/home/coder/.cache",
        uid,
        gid,
        verbose,
    );
}

/// Ensure that the host .pnpm-store directory under the given workspace is present
/// and writable by the sidecar user. Best-effort: errors are ignored.
pub(crate) fn ensure_pnpm_store_host_writable(
    pwd: &Path,
    uidgid: Option<(u32, u32)>,
    verbose: bool,
) {
    let use_err = crate::color_enabled_stderr();
    let dir = pwd.join(".pnpm-store");
    let mut changed = false;

    if !dir.exists() {
        if let Err(e) = std::fs::create_dir_all(&dir) {
            if verbose {
                crate::log_warn_stderr(
                    use_err,
                    &format!(
                        "aifo-coder: warning: failed to create .pnpm-store directory: {}",
                        e
                    ),
                );
            }
            return;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o775));
        }
        changed = true;
    } else if !dir.is_dir() {
        if verbose {
            crate::log_warn_stderr(
                use_err,
                "aifo-coder: warning: .pnpm-store exists but is not a directory; skipping ownership fix.",
            );
        }
        return;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(&dir) {
            let mode = meta.permissions().mode() & 0o777;
            if mode & 0o200 == 0 {
                let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o775));
                changed = true;
            }
        }
    }

    #[cfg(unix)]
    if let Some((uid, gid)) = uidgid {
        use std::os::unix::fs::MetadataExt;
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(&dir) {
            if meta.uid() != uid || meta.gid() != gid {
                // Best-effort recursive chown; fall back silently on error.
                let _ = nix::unistd::chown(
                    &dir,
                    Some(nix::unistd::Uid::from_raw(uid)),
                    Some(nix::unistd::Gid::from_raw(gid)),
                );
                if let Ok(iter) = walkdir::WalkDir::new(&dir)
                    .into_iter()
                    .collect::<Result<Vec<_>, _>>()
                {
                    for entry in iter {
                        let _ = nix::unistd::chown(
                            entry.path(),
                            Some(nix::unistd::Uid::from_raw(uid)),
                            Some(nix::unistd::Gid::from_raw(gid)),
                        );
                    }
                }
                changed = true;
            }
        }
        // Ensure group-writable bit remains set after chown (best-effort)
        if let Ok(meta) = std::fs::metadata(&dir) {
            let mode = meta.permissions().mode() & 0o777;
            if mode & 0o20 == 0 {
                let _ =
                    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(mode | 0o20));
            }
        }
    }

    if verbose && changed {
        crate::log_info_stderr(
            use_err,
            &format!(
                "aifo-coder: ensured writable .pnpm-store at {} for node toolchain",
                dir.display()
            ),
        );
    }
}

///// Best-effort ownership initialization for the consolidated Node cache volume.
///// Runs a short helper container that ensures /home/coder/.cache exists, chowns to uid:gid,
///// and stamps the directory to avoid repeated work.
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

/// Best-effort ownership initialization for the node_modules overlay volume.
/// Ensures /workspace/node_modules exists, is owned by uid:gid, and is stamped
/// to avoid repeated work.
fn init_node_modules_volume(runtime: &Path, image: &str, uid: u32, gid: u32, verbose: bool) {
    init_named_volume_with_stamp(
        runtime,
        image,
        "aifo-node-modules:/workspace/node_modules",
        "/workspace/node_modules",
        uid,
        gid,
        verbose,
    );
}

/// Inspect run-args and initialize the node_modules overlay volume when selected.
pub(crate) fn init_node_modules_volume_if_needed(
    runtime: &Path,
    image: &str,
    run_args: &[String],
    uidgid: Option<(u32, u32)>,
    verbose: bool,
) {
    let mut need_node_modules = false;
    let mut i = 0usize;
    while i + 1 < run_args.len() {
        if run_args[i] == "-v" {
            let mnt = &run_args[i + 1];
            if mnt.starts_with("aifo-node-modules:") && mnt.ends_with("/workspace/node_modules") {
                need_node_modules = true;
            }
            i += 2;
        } else {
            i += 1;
        }
    }
    if !need_node_modules {
        return;
    }
    let (uid, gid) = uidgid.unwrap_or((0u32, 0u32));
    init_node_modules_volume(runtime, image, uid, gid, verbose);
}
