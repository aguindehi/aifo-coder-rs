#![allow(clippy::module_name_repetitions)]
//! Image selection helpers and staging cleanup for agent runs.

use std::env;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use once_cell::sync::Lazy;

use crate::docker_mod::docker::images::image_exists;
use crate::docker_mod::docker::runtime::container_runtime_path;
use crate::util::{ExecOutput, ExecRequest, ExecService};

static DOCKER_EXEC: Lazy<ExecService> = Lazy::new(|| ExecService::new(Duration::from_secs(300)));

fn run_runtime_command<I>(
    runtime: &Path,
    args: I,
    capture: bool,
    timeout: Duration,
) -> io::Result<ExecOutput>
where
    I: IntoIterator<Item = OsString>,
{
    let request = ExecRequest::new(runtime.as_os_str().to_os_string())
        .args(args)
        .inherit_env(true)
        .timeout(timeout)
        .capture_output(capture);
    DOCKER_EXEC.run(request).map_err(io::Error::other)
}

/// Derive registry host from an image reference (first component if qualified).
fn parse_registry_host(image: &str) -> Option<String> {
    if let Some((first, _rest)) = image.split_once('/') {
        if first.contains('.') || first.contains(':') || first == "localhost" {
            return Some(first.to_string());
        }
    }
    None
}

/// Pull image and on auth failure interactively run `docker login` then retry once.
/// Verbose runs stream docker pull output; non-verbose prints a short notice before quiet pull.
pub fn pull_image_with_autologin(
    runtime: &Path,
    image: &str,
    verbose: bool,
    agent_label: Option<&str>,
) -> io::Result<()> {
    // Effective verbosity: honor explicit flag or env set by CLI --verbose.
    let eff_verbose = verbose || env::var("AIFO_CODER_VERBOSE").ok().as_deref() == Some("1");
    let use_err = crate::color_enabled_stderr();

    // Helper to do a pull with inherited stdio so progress is visible.
    let pull_inherit = |rt: &Path, img: &str| -> io::Result<bool> {
        let out = run_runtime_command(
            rt,
            ["pull", img].into_iter().map(OsString::from),
            false,
            Duration::from_secs(300),
        )?;

        Ok(out.status.success())
    };

    // Helper to do a pull with captured output so we can parse error text.
    let pull_captured = |rt: &Path, img: &str| -> io::Result<(bool, String)> {
        let out = run_runtime_command(
            rt,
            ["pull", img].into_iter().map(OsString::from),
            true,
            Duration::from_secs(300),
        )?;
        let combined = format!("{}\n{}", out.stdout, out.stderr).to_ascii_lowercase();
        Ok((out.status.success(), combined))
    };

    let auth_patterns = [
        "pull access denied",
        "permission denied",
        "authentication required",
        "unauthorized",
        "requested access to the resource is denied",
        "may require 'docker login'",
        "requires 'docker login'",
    ];

    if eff_verbose {
        crate::log_info_stderr(
            use_err,
            &format!("aifo-coder: docker: docker pull {}", image),
        );
        if pull_inherit(runtime, image)? {
            return Ok(());
        }
        let (_ok2, combined) = pull_captured(runtime, image)?;
        let looks_auth_error = auth_patterns.iter().any(|p| combined.contains(p));
        let auto_enabled = env::var("AIFO_CODER_AUTO_LOGIN").ok().as_deref() != Some("0");
        let interactive = atty::is(atty::Stream::Stdin);

        if auto_enabled && interactive && looks_auth_error {
            let host = parse_registry_host(image);
            let mut login_args = vec![OsString::from("login")];
            if let Some(h) = host.as_deref() {
                crate::log_info_stderr(use_err, &format!("aifo-coder: docker: docker login {}", h));
                login_args.push(OsString::from(h));
            } else {
                crate::log_info_stderr(use_err, "aifo-coder: docker: docker login");
            }
            let login_out = run_runtime_command(
                runtime,
                login_args.into_iter(),
                false,
                Duration::from_secs(120),
            )?;
            if !login_out.status.success() {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "docker login failed",
                ));
            }
            if pull_inherit(runtime, image)? {
                return Ok(());
            }
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "docker pull failed after login",
            ));
        }

        // Fallback: try pulling unqualified tail repo:tag from Docker Hub when image is qualified
        if parse_registry_host(image).is_some() {
            let tag = image
                .split_once('@')
                .map(|(n, _)| n)
                .unwrap_or(image)
                .rsplit_once(':')
                .map(|(_, t)| t.to_string())
                .unwrap_or_else(|| "latest".to_string());
            let tail = image.rsplit('/').next().unwrap_or(image);
            let unqual = format!(
                "{}:{}",
                tail.split_once(':').map(|(n, _)| n).unwrap_or(tail),
                tag
            );
            crate::log_info_stderr(
                use_err,
                &format!("aifo-coder: docker: docker pull {}", unqual),
            );
            if pull_inherit(runtime, &unqual)? {
                return Ok(());
            }
            return Err(io::Error::other(format!(
                "docker pull failed; tried: {}, {}",
                image, unqual
            )));
        }

        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "docker pull failed",
        ))
    } else {
        // Non-verbose: print a short notice before quiet pull so users get feedback.
        let msg = if let Some(name) = agent_label {
            format!("aifo-coder: pulling agent image [{}]: {}", name, image)
        } else {
            format!("aifo-coder: pulling agent image: {}", image)
        };
        crate::log_info_stderr(use_err, &msg);

        let out = run_runtime_command(
            runtime,
            ["pull", image].into_iter().map(OsString::from),
            true,
            Duration::from_secs(300),
        )?;
        if out.status.success() {
            return Ok(());
        }

        let auto_enabled = env::var("AIFO_CODER_AUTO_LOGIN").ok().as_deref() != Some("0");
        let interactive = atty::is(atty::Stream::Stdin);
        let combined = format!("{}\n{}", out.stdout, out.stderr).to_ascii_lowercase();
        let looks_auth_error = auth_patterns.iter().any(|p| combined.contains(p));

        if auto_enabled && interactive && looks_auth_error {
            let host = parse_registry_host(image);
            if let Some(h) = host.as_deref() {
                crate::log_info_stderr(use_err, &format!("aifo-coder: docker login {}", h));
            } else {
                crate::log_info_stderr(use_err, "aifo-coder: docker login");
            }
            let mut login_args = vec![OsString::from("login")];
            if let Some(h) = host.as_deref() {
                login_args.push(OsString::from(h));
            }
            let st = run_runtime_command(
                runtime,
                login_args.into_iter(),
                false,
                Duration::from_secs(120),
            )?;
            if !st.status.success() {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "docker login failed",
                ));
            }

            crate::log_info_stderr(use_err, "aifo-coder: retrying pull after login");
            let out2 = run_runtime_command(
                runtime,
                ["pull", image].into_iter().map(OsString::from),
                true,
                Duration::from_secs(300),
            )?;
            if out2.status.success() {
                return Ok(());
            }
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "docker pull failed after login",
            ));
        }

        // Fallback: try pulling unqualified tail repo:tag from Docker Hub when image is qualified
        if parse_registry_host(image).is_some() {
            let tag = image
                .split_once('@')
                .map(|(n, _)| n)
                .unwrap_or(image)
                .rsplit_once(':')
                .map(|(_, t)| t.to_string())
                .unwrap_or_else(|| "latest".to_string());
            let tail = image.rsplit('/').next().unwrap_or(image);
            let unqual = format!(
                "{}:{}",
                tail.split_once(':').map(|(n, _)| n).unwrap_or(tail),
                tag
            );
            let out_hub = run_runtime_command(
                runtime,
                ["pull", unqual.as_str()].into_iter().map(OsString::from),
                true,
                Duration::from_secs(300),
            )?;
            if out_hub.status.success() {
                Ok(())
            } else {
                Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    format!("docker pull failed; tried: {}, {}", image, unqual),
                ))
            }
        } else {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!(
                    "docker pull failed (status {:?})",
                    out.status.code().unwrap_or(-1)
                ),
            ))
        }
    }
}

/// Helper: set/replace tag on an image reference (strip any digest, replace last tag after '/').
fn set_image_tag(image: &str, new_tag: &str) -> String {
    let base = image.split_once('@').map(|(n, _)| n).unwrap_or(image);
    let last_slash = base.rfind('/');
    let last_colon = base.rfind(':');
    let without_tag = match (last_slash, last_colon) {
        (Some(slash), Some(colon)) if colon > slash => &base[..colon],
        (None, Some(_colon)) => base.split(':').next().unwrap_or(base),
        _ => base,
    };
    format!("{}:{}", without_tag, new_tag)
}

/// Helper: apply agent image overrides from environment.
fn maybe_override_agent_image(image: &str) -> String {
    // Highest precedence: explicit full image override
    if let Ok(v) = env::var("AIFO_CODER_AGENT_IMAGE") {
        let t = v.trim();
        if !t.is_empty() {
            return t.to_string();
        }
    }
    // Next: per-agent tag override
    if let Ok(tag) = env::var("AIFO_CODER_AGENT_TAG") {
        let t = tag.trim();
        if !t.is_empty() {
            return set_image_tag(image, t);
        }
    }
    // Global tag override applies when no agent-specific override is set
    if let Ok(tag) = env::var("AIFO_TAG") {
        let t = tag.trim();
        if !t.is_empty() {
            return set_image_tag(image, t);
        }
    }
    image.to_string()
}

/// Compute the effective agent image for real run:
/// - Apply env overrides (AIFO_CODER_AGENT_IMAGE/TAG),
/// - Resolve registry/namespace,
/// - Prefer local "<name>:latest" when present.
pub fn compute_effective_agent_image_for_run(image: &str) -> io::Result<String> {
    // Allow tests to exercise tag logic without requiring Docker by honoring
    // AIFO_CODER_TEST_DISABLE_DOCKER: when set, skip local existence checks.
    let runtime: Option<std::path::PathBuf> = match container_runtime_path() {
        Ok(p) => Some(p),
        Err(e) => {
            if env::var("AIFO_CODER_TEST_DISABLE_DOCKER").ok().as_deref() == Some("1") {
                None
            } else {
                return Err(e);
            }
        }
    };

    // Apply env overrides (same as build path)
    let base_image = maybe_override_agent_image(image);

    // Tail repository name (drop any registry/namespace and tag)
    let tail_repo = {
        let base = base_image
            .split_once('@')
            .map(|(n, _)| n)
            .unwrap_or(base_image.as_str());
        let last = base.rsplit('/').next().unwrap_or(base);
        last.split_once(':')
            .map(|(n, _)| n)
            .unwrap_or(last)
            .to_string()
    };
    let rel_tag = format!("release-{}", env!("CARGO_PKG_VERSION"));
    let internal = crate::preferred_internal_registry_prefix_quiet();

    // Prefer local images in this order unless CLI requested ignoring local images:
    // 1) unqualified :latest, 2) unqualified :release-<pkg>,
    // 3) internal-qualified :latest, 4) internal-qualified :release-<pkg>.
    let ignore_local = crate::cli_ignore_local_images();
    if let Some(rt) = runtime.as_ref() {
        let candidates = [
            format!("{tail_repo}:latest"),
            format!("{tail_repo}:{rel_tag}"),
            if internal.is_empty() {
                String::new()
            } else {
                format!("{internal}{tail_repo}:latest")
            },
            if internal.is_empty() {
                String::new()
            } else {
                format!("{internal}{tail_repo}:{rel_tag}")
            },
        ];
        for c in candidates
            .iter()
            .filter(|s| !s.is_empty())
            .filter(|_| !ignore_local)
        {
            if image_exists(rt.as_path(), c) {
                return Ok(c.to_string());
            }
        }
    }

    // Remote resolution: prefer internal registry via resolve_image (may qualify),
    // otherwise return unqualified (Docker Hub) reference.
    let resolved_image = crate::registry::resolve_image(&base_image);
    Ok(resolved_image)
}

fn split_paths_env(v: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if v.is_empty() {
        return out;
    }
    // Use ':' as separator across platforms; paths are under ~/.config/aifo-coder so this is safe.
    for part in v.split(':') {
        let trimmed = part.trim();
        if !trimmed.is_empty() {
            out.push(PathBuf::from(trimmed));
        }
    }
    out
}

pub fn cleanup_aider_staging_from_env() {
    // Legacy single-dir env (pre-multi-agent staging)
    if let Ok(p) = env::var("AIFO_AIDER_STAGING_DIR") {
        let path = PathBuf::from(p);
        let _ = fs::remove_dir_all(&path);
        std::env::remove_var("AIFO_AIDER_STAGING_DIR");
    }

    if let Ok(v) = env::var("AIFO_CONFIG_STAGING_DIRS") {
        for p in split_paths_env(&v) {
            let _ = fs::remove_dir_all(&p);
        }
        std::env::remove_var("AIFO_CONFIG_STAGING_DIRS");
    }
}
