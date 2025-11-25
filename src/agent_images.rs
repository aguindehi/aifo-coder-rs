//! Agent image selection helpers.
//!
//! Rules
//! - Environment overrides take precedence: AIFO_CODER_IMAGE, AIFO_CODER_IMAGE_PREFIX/TAG/FLAVOR.
//! - Flavor slim/full is expressed via "-slim" suffix and affects defaults only.
//! - Internal registry prefix comes from preferred_internal_registry_prefix[_quiet]; when non-empty, prepend it to our images.
//!
//! These functions do not pull images or perform network I/O; they only compose strings.

use std::env;
use std::process::{Command, Stdio};

use aifo_coder::container_runtime_path;

/// Trimmed env getter returning Some when non-empty.
fn env_trim(k: &str) -> Option<String> {
    env::var(k)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Local image existence check via docker inspect.
fn docker_image_exists_local(image: &str) -> bool {
    if let Ok(rt) = container_runtime_path() {
        return Command::new(&rt)
            .arg("image")
            .arg("inspect")
            .arg(image)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
    }
    false
}

/// Derive a ":latest" candidate for a given image reference by replacing the tag.
/// E.g., "aifo-coder-aider:release-0.6.3" -> "aifo-coder-aider:latest".
fn local_latest_candidate(image: &str) -> String {
    let base = image.split_once('@').map(|(n, _)| n).unwrap_or(image);
    let last_slash = base.rfind('/');
    let last_colon = base.rfind(':');
    let without_tag = match (last_slash, last_colon) {
        (Some(slash), Some(colon)) if colon > slash => &base[..colon],
        (None, Some(colon)) => &base[..colon],
        _ => base,
    };
    format!("{without_tag}:latest")
}

pub(crate) fn default_image_for(agent: &str) -> String {
    // Full override wins.
    if let Some(img) = env_trim("AIFO_CODER_IMAGE") {
        return img;
    }

    // Compose unqualified image name with tag precedence:
    // AIFO_CODER_IMAGE_TAG -> AIFO_GLOBAL_TAG -> release-<pkg-version>
    let name_prefix =
        env::var("AIFO_CODER_IMAGE_PREFIX").unwrap_or_else(|_| "aifo-coder".to_string());
    let tag = env_trim("AIFO_CODER_IMAGE_TAG")
        .or_else(|| env_trim("AIFO_TAG"))
        .unwrap_or_else(|| format!("release-{}", env!("CARGO_PKG_VERSION")));
    let suffix = match env::var("AIFO_CODER_IMAGE_FLAVOR") {
        Ok(v) if v.trim().eq_ignore_ascii_case("slim") => "-slim",
        _ => "",
    };
    let mut image = format!("{name_prefix}-{agent}{suffix}:{tag}");

    // If the image already contains a registry path (prefix in name_prefix), return as-is.
    if image.contains('/') {
        return image;
    }

    // Prefer a local image if present and no explicit internal registry env is set.
    let explicit_ir = aifo_coder::preferred_internal_registry_source() == "env";
    if !explicit_ir {
        if docker_image_exists_local(&image) {
            return image;
        }
        // Fallback: try a local ":latest" tag for the same repository name.
        let latest = local_latest_candidate(&image);
        if docker_image_exists_local(&latest) {
            return latest;
        }
    }

    // Otherwise, qualify with our internal registry prefix if known (env or mirror probe).
    let ir = aifo_coder::preferred_internal_registry_prefix_autodetect();
    if !ir.is_empty() {
        image = format!("{ir}{image}");
    }
    image
}

pub(crate) fn default_image_for_quiet(agent: &str) -> String {
    // Full override wins.
    if let Some(img) = env_trim("AIFO_CODER_IMAGE") {
        return img;
    }

    // Compose unqualified image with tag precedence:
    let name_prefix =
        env::var("AIFO_CODER_IMAGE_PREFIX").unwrap_or_else(|_| "aifo-coder".to_string());
    let tag = env_trim("AIFO_CODER_IMAGE_TAG")
        .or_else(|| env_trim("AIFO_TAG"))
        .unwrap_or_else(|| format!("release-{}", env!("CARGO_PKG_VERSION")));
    let suffix = match env::var("AIFO_CODER_IMAGE_FLAVOR") {
        Ok(v) if v.trim().eq_ignore_ascii_case("slim") => "-slim",
        _ => "",
    };
    let mut image = format!("{name_prefix}-{agent}{suffix}:{tag}");

    if image.contains('/') {
        return image;
    }

    // Same local-first and qualification policy as default_image_for()
    let explicit_ir = aifo_coder::preferred_internal_registry_source() == "env";
    if !explicit_ir {
        if docker_image_exists_local(&image) {
            return image;
        }
        // Fallback: try a local ":latest" tag for the same repository name.
        let latest = local_latest_candidate(&image);
        if docker_image_exists_local(&latest) {
            return latest;
        }
    }

    let ir = aifo_coder::preferred_internal_registry_prefix_autodetect();
    if !ir.is_empty() {
        image = format!("{ir}{image}");
    }
    image
}
