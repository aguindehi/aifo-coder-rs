
//! Agent image selection helpers.
//!
//! Rules
//! - Environment overrides take precedence: AIFO_CODER_IMAGE, AIFO_CODER_IMAGE_PREFIX/TAG/FLAVOR.
//! - Flavor slim/full is expressed via "-slim" suffix and affects defaults only.
//! - Registry prefix comes from preferred_registry_prefix[_quiet] and is normalized to "<host>/".
//!
//! These functions do not pull images or perform network I/O; they only compose strings.

use std::env;

pub(crate) fn default_image_for(agent: &str) -> String {
    if let Ok(img) = env::var("AIFO_CODER_IMAGE") {
        if !img.trim().is_empty() {
            return img;
        }
    }
    let name_prefix =
        env::var("AIFO_CODER_IMAGE_PREFIX").unwrap_or_else(|_| "aifo-coder".to_string());
    let tag = env::var("AIFO_CODER_IMAGE_TAG").unwrap_or_else(|_| "latest".to_string());
    let suffix = match env::var("AIFO_CODER_IMAGE_FLAVOR") {
        Ok(v) if v.trim().eq_ignore_ascii_case("slim") => "-slim",
        _ => "",
    };
    let image_name = format!("{name_prefix}-{agent}{suffix}:{tag}");
    let registry = aifo_coder::preferred_registry_prefix();
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
    let tag = env::var("AIFO_CODER_IMAGE_TAG").unwrap_or_else(|_| "latest".to_string());
    let suffix = match env::var("AIFO_CODER_IMAGE_FLAVOR") {
        Ok(v) if v.trim().eq_ignore_ascii_case("slim") => "-slim",
        _ => "",
    };
    let image_name = format!("{name_prefix}-{agent}{suffix}:{tag}");
    let registry = aifo_coder::preferred_registry_prefix_quiet();
    if registry.is_empty() {
        image_name
    } else {
        format!("{registry}{image_name}")
    }
}
