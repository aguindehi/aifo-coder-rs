#![allow(clippy::module_name_repetitions)]
//! Image selection helpers and staging cleanup for agent runs.

use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;

use crate::docker_mod::docker::images::image_exists;
use crate::docker_mod::docker::runtime::container_runtime_path;

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

    // Prefer local images in this order:
    // 1) unqualified :latest, 2) unqualified :release-<pkg>,
    // 3) internal-qualified :latest, 4) internal-qualified :release-<pkg>.
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
        for c in candidates.iter().filter(|s| !s.is_empty()) {
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

/// Remove per-run staged config directories recorded in AIFO_CONFIG_STAGING_DIRS and
/// the legacy AIFO_AIDER_STAGING_DIR (best-effort).
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
