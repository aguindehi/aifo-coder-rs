/*!
Toolchain kind normalization and image selection.

- normalize_toolchain_kind: apply canonical aliases
- default_toolchain_image: choose default image with overrides and official fallback
- default_toolchain_image_for_version: versioned image selectors
- is_official_rust_image / official_rust_image_for_version: helpers for rust
*/
use crate::container_runtime_path;
use std::env;
use std::process::{Command, Stdio};

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

/// Helper: read an env var, trim, and return Some when non-empty.
fn env_trim(k: &str) -> Option<String> {
    env::var(k)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Resolve a tag override for a toolchain kind with precedence:
/// per-kind tag -> AIFO_TOOLCHAIN_TAG -> AIFO_TAG.
fn tag_override_for_kind(kind: &str) -> Option<String> {
    match kind {
        "rust" => env_trim("RUST_TOOLCHAIN_TAG")
            .or_else(|| env_trim("AIFO_TOOLCHAIN_TAG"))
            .or_else(|| env_trim("AIFO_TAG")),
        "node" => env_trim("NODE_TOOLCHAIN_TAG")
            .or_else(|| env_trim("AIFO_TOOLCHAIN_TAG"))
            .or_else(|| env_trim("AIFO_TAG")),
        "c-cpp" => env_trim("CPP_TOOLCHAIN_TAG")
            .or_else(|| env_trim("AIFO_TOOLCHAIN_TAG"))
            .or_else(|| env_trim("AIFO_TAG")),
        _ => env_trim("AIFO_TOOLCHAIN_TAG").or_else(|| env_trim("AIFO_TAG")),
    }
}

/// First-party toolchain images are named "aifo-coder-toolchain-<kind>:<tag>".
/// This remains true even when prefixed with an internal registry.
fn is_first_party(image: &str) -> bool {
    image.contains("aifo-coder-toolchain-")
}

/// Replace the tag component of an image reference (last ':' split).
fn replace_tag(image: &str, tag: &str) -> String {
    let mut parts = image.rsplitn(2, ':');
    let _old = parts.next().unwrap_or("");
    let repo = parts.next().unwrap_or(image);
    format!("{repo}:{tag}")
}

/// Structured mappings for toolchain normalization and default images
/// Canonical kind aliases (lhs -> rhs)
const TOOLCHAIN_ALIASES: &[(&str, &str)] = &[
    ("rust", "rust"),
    ("node", "node"),
    // Keep TypeScript as its own canonical kind so we can use a dedicated toolchain image
    ("ts", "typescript"),
    ("typescript", "typescript"),
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
    ("rust", "aifo-coder-toolchain-rust:latest"),
    ("node", "aifo-coder-toolchain-node:latest"),
    ("typescript", "aifo-coder-toolchain-ts:latest"),
    ("python", "python:3.12-slim"),
    ("c-cpp", "aifo-coder-toolchain-cpp:latest"),
    ("go", "golang:1.22-bookworm"),
];

/// Default image templates for kind@version (use {version} placeholder)
const DEFAULT_IMAGE_FMT_BY_KIND: &[(&str, &str)] = &[
    ("rust", "aifo-coder-toolchain-rust:{version}"),
    ("node", "aifo-coder-toolchain-node:{version}"),
    ("typescript", "aifo-coder-toolchain-ts:{version}"),
    ("python", "python:{version}-slim"),
    ("go", "golang:{version}-bookworm"),
    ("c-cpp", "aifo-coder-toolchain-cpp:{version}"),
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
                return format!("aifo-coder-toolchain-rust:{v}");
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
                return format!("aifo-coder-toolchain-node:{v}");
            }
        }
    }
    let mut base = default_image_for_kind_const(&k)
        .unwrap_or("node:22-bookworm-slim")
        .to_string();
    // Apply tag overrides first and prefer local unqualified images for our first-party toolchains.
    if is_first_party(&base) {
        if let Some(tag) = tag_override_for_kind(&k) {
            base = replace_tag(&base, &tag);
        }
        // Prefer local unqualified image unless internal registry is explicitly set via env.
        if !base.contains('/') {
            let explicit_ir = crate::preferred_internal_registry_source() == "env";
            if !explicit_ir && docker_image_exists_local(&base) {
                return base;
            }
        }
        // If internal registry prefix is available, prepend it; otherwise resolve via autodetect/env when unqualified.
        if !is_official_rust_image(&base) && base.starts_with("aifo-coder-toolchain-") {
            let ir = crate::preferred_internal_registry_prefix_quiet();
            if !ir.is_empty() {
                base = format!("{ir}{base}");
            } else if !base.contains('/') {
                base = crate::resolve_image(&base);
            }
        }
    }
    base
}

/// Compute default image from kind@version (best-effort).
pub fn default_toolchain_image_for_version(kind: &str, version: &str) -> String {
    let k = normalize_toolchain_kind(kind);
    if let Some(fmt) = default_image_fmt_for_kind_const(&k) {
        // For explicit version mappings, do not alter/qualify the image:
        // keep the exact "aifo-coder-toolchain-<kind>:<version>" (or upstream fmt) unprefixed.
        let base = fmt.replace("{version}", version);
        return base;
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
