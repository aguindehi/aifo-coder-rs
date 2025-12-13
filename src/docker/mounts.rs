#![allow(clippy::module_name_repetitions)]
//! Mount policy helpers for docker runs (agent containers).

use std::fs;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use nix::unistd::getuid;

/// Validate an env-controlled mount source directory.
/// Returns a canonicalized absolute path when safe to mount, otherwise None.
pub(crate) fn validate_mount_source_dir(path_str: &str, purpose: &str) -> Option<PathBuf> {
    let p = PathBuf::from(path_str.trim());
    if p.as_os_str().is_empty() {
        return None;
    }
    if !p.is_absolute() {
        crate::warn_print(&format!(
            "aifo-coder: warning: refusing to mount non-absolute path for {}: {}",
            purpose,
            p.display()
        ));
        return None;
    }
    let canon = match fs::canonicalize(&p) {
        Ok(c) => c,
        Err(e) => {
            crate::warn_print(&format!(
                "aifo-coder: warning: refusing to mount {}: cannot canonicalize {}: {}",
                purpose,
                p.display(),
                e
            ));
            return None;
        }
    };
    if !canon.exists() {
        crate::warn_print(&format!(
            "aifo-coder: warning: refusing to mount {}: path does not exist: {}",
            purpose,
            canon.display()
        ));
        return None;
    }
    if !canon.is_dir() {
        crate::warn_print(&format!(
            "aifo-coder: warning: refusing to mount {}: not a directory: {}",
            purpose,
            canon.display()
        ));
        return None;
    }
    Some(canon)
}

#[cfg(unix)]
pub(crate) fn validate_unix_socket_dir_owner_mode(dir: &Path, purpose: &str) -> bool {
    use std::os::unix::fs::MetadataExt;
    use std::os::unix::fs::PermissionsExt;

    let use_err = crate::color_enabled_stderr();
    let uid = u32::from(getuid());

    let md = match fs::metadata(dir) {
        Ok(m) => m,
        Err(_) => return false,
    };
    if md.uid() != uid {
        crate::log_warn_stderr(
            use_err,
            &format!(
                "aifo-coder: warning: refusing to mount {}: not owned by current user: {}",
                purpose,
                dir.display()
            ),
        );
        return false;
    }
    let mode = md.permissions().mode() & 0o777;
    // Accept 0700 or 0750. (Reject group/other writable.)
    let ok = mode == 0o700 || mode == 0o750;
    if !ok {
        crate::log_warn_stderr(
            use_err,
            &format!(
                "aifo-coder: warning: refusing to mount {}: directory mode must be 0700 or 0750 (got {:o}): {}",
                purpose,
                mode,
                dir.display()
            ),
        );
    }
    ok
}
