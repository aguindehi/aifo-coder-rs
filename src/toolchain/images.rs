/*!
Toolchain kind normalization and image selection.

- normalize_toolchain_kind: apply canonical aliases
- default_toolchain_image: choose default image with overrides and official fallback
- default_toolchain_image_for_version: versioned image selectors
- is_official_rust_image / official_rust_image_for_version: helpers for rust
*/
use std::env;

/// Structured mappings for toolchain normalization and default images
/// Canonical kind aliases (lhs -> rhs)
const TOOLCHAIN_ALIASES: &[(&str, &str)] = &[
    ("rust", "rust"),
    ("node", "node"),
    ("ts", "node"),
    ("typescript", "node"),
    ("python", "python"),
    ("py", "python"),
    ("c", "c-cpp"),
    ("cpp", "c-cpp"),
    ("c-cpp", "c-cpp"),
    ("c_cpp", "c-cpp"),
    ("c++", "c-cpp"),
    ("go", "go"),
    ("golang", "go"),
];

/// Default images by normalized kind
const DEFAULT_IMAGE_BY_KIND: &[(&str, &str)] = &[
    ("rust", "aifo-rust-toolchain:latest"),
    ("node", "aifo-node-toolchain:latest"),
    ("python", "python:3.12-slim"),
    ("c-cpp", "aifo-cpp-toolchain:latest"),
    ("go", "golang:1.22-bookworm"),
];

/// Default image templates for kind@version (use {version} placeholder)
const DEFAULT_IMAGE_FMT_BY_KIND: &[(&str, &str)] = &[
    ("rust", "aifo-rust-toolchain:{version}"),
    ("node", "aifo-node-toolchain:{version}"),
    ("python", "python:{version}-slim"),
    ("go", "golang:{version}-bookworm"),
    // c-cpp has no versioned mapping; falls back to non-versioned default
];

fn default_image_for_kind_const(kind: &str) -> Option<&'static str> {
    for (k, v) in DEFAULT_IMAGE_BY_KIND.iter() {
        if *k == kind {
            return Some(*v);
        }
    }
    None
}

fn default_image_fmt_for_kind_const(kind: &str) -> Option<&'static str> {
    for (k, v) in DEFAULT_IMAGE_FMT_BY_KIND.iter() {
        if *k == kind {
            return Some(*v);
        }
    }
    None
}

/// Normalize toolchain kind names to canonical identifiers
pub fn normalize_toolchain_kind(kind: &str) -> String {
    let lower = kind.to_ascii_lowercase();
    for (alias, canon) in TOOLCHAIN_ALIASES.iter() {
        if alias.eq(&lower.as_str()) {
            return (*canon).to_string();
        }
    }
    lower
}

pub fn default_toolchain_image(kind: &str) -> String {
    let k = normalize_toolchain_kind(kind);
    if k == "rust" {
        // Explicit override takes precedence
        if let Ok(img) = env::var("AIFO_RUST_TOOLCHAIN_IMAGE") {
            let img = img.trim();
            if !img.is_empty() {
                return img.to_string();
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
                return format!("aifo-rust-toolchain:{v}");
            }
        }
        // fall through to default constant
    }
    if k == "node" {
        // Symmetric overrides for Node toolchain image and version
        if let Ok(img) = env::var("AIFO_NODE_TOOLCHAIN_IMAGE") {
            let img = img.trim();
            if !img.is_empty() {
                return img.to_string();
            }
        }
        if let Ok(ver) = env::var("AIFO_NODE_TOOLCHAIN_VERSION") {
            let v = ver.trim();
            if !v.is_empty() {
                return format!("aifo-node-toolchain:{v}");
            }
        }
    }
    default_image_for_kind_const(&k)
        .unwrap_or("node:20-bookworm-slim")
        .to_string()
}

/// Compute default image from kind@version (best-effort).
pub fn default_toolchain_image_for_version(kind: &str, version: &str) -> String {
    let k = normalize_toolchain_kind(kind);
    if let Some(fmt) = default_image_fmt_for_kind_const(&k) {
        return fmt.replace("{version}", version);
    }
    // No version mapping for this kind; fall back to non-versioned default
    default_toolchain_image(&k)
}

// Heuristic to detect official rust images like "rust:<tag>" (with or without a registry prefix)
pub fn is_official_rust_image(image: &str) -> bool {
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

pub fn official_rust_image_for_version(version_opt: Option<&str>) -> String {
    let v = match version_opt {
        Some(s) if !s.is_empty() => s,
        _ => "1.80",
    };
    format!("rust:{v}-bookworm")
}
