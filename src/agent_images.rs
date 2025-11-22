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

use crate::container_runtime_path;

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

pub(crate) fn default_image_for(agent: &str) -> String {
    if let Ok(img) = env::var("AIFO_CODER_IMAGE") {
        if !img.trim().is_empty() {
            return img;
        }
    }
    let name_prefix =
        env::var("AIFO_CODER_IMAGE_PREFIX").unwrap_or_else(|_| "aifo-coder".to_string());
    let tag = env::var("AIFO_CODER_IMAGE_TAG")
        .unwrap_or_else(|_| format!("release-{}", env!("CARGO_PKG_VERSION")));
    let suffix = match env::var("AIFO_CODER_IMAGE_FLAVOR") {
        Ok(v) if v.trim().eq_ignore_ascii_case("slim") => "-slim",
        _ => "",
    };
    let image_name = format!("{name_prefix}-{agent}{suffix}:{tag}");
    let registry = aifo_coder::preferred_internal_registry_prefix_autodetect();
    if registry.is_empty() {
        image_name
    } else {
        format!("{registry}{image_name}")
    }
}

pub(crate) fn default_image_for_quiet(agent: &str) -> String {
    if let Ok(img) = env::var("AIFO_CODER_IMAGE") {
        if !img.trim().is_empty() {
            return img;
        }
    }
    let name_prefix =
        env::var("AIFO_CODER_IMAGE_PREFIX").unwrap_or_else(|_| "aifo-coder".to_string());
    let tag = env::var("AIFO_CODER_IMAGE_TAG")
        .unwrap_or_else(|_| format!("release-{}", env!("CARGO_PKG_VERSION")));
    let suffix = match env::var("AIFO_CODER_IMAGE_FLAVOR") {
        Ok(v) if v.trim().eq_ignore_ascii_case("slim") => "-slim",
        _ => "",
    };
    let image_name = format!("{name_prefix}-{agent}{suffix}:{tag}");
    let registry = aifo_coder::preferred_internal_registry_prefix_autodetect();
    if registry.is_empty() {
        image_name
    } else {
        format!("{registry}{image_name}")
    }
}
