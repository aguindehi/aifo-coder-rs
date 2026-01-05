#![allow(clippy::module_name_repetitions)]
//! Docker image helpers.

use std::collections::HashMap;
use std::path::Path;
use std::process::{Command, Stdio};

use serde::Deserialize;

/// Return true if a docker image exists locally (without pulling).
pub fn image_exists(runtime: &Path, image: &str) -> bool {
    if crate::cli_ignore_local_images() {
        return false;
    }
    Command::new(runtime)
        .arg("image")
        .arg("inspect")
        .arg(image)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[derive(Debug, Clone)]
pub struct ImageMetadata {
    pub created: Option<String>,
    pub id: Option<String>,
    pub digest: Option<String>,
    pub tag: Option<String>,
    pub title: Option<String>,
    pub version: Option<String>,
    pub revision: Option<String>,
}

#[derive(Default, Deserialize)]
struct InspectConfig {
    #[serde(default, rename = "Labels")]
    labels: HashMap<String, String>,
}

#[derive(Deserialize)]
struct InspectImage {
    #[serde(default, rename = "ContainerConfig")]
    container_config: InspectConfig,
    #[serde(default, rename = "Config")]
    config: InspectConfig,
    #[serde(default, rename = "Created")]
    created: Option<String>,
    #[serde(default, rename = "Id")]
    id: Option<String>,
    #[serde(default, rename = "RepoDigests")]
    repo_digests: Option<Vec<String>>,
    #[serde(default, rename = "RepoTags")]
    repo_tags: Option<Vec<String>>,
}

/// Inspect a docker image and return key metadata (labels, creation time, id).
pub fn image_metadata(runtime: &Path, image: &str) -> Option<ImageMetadata> {
    let output = Command::new(runtime)
        .arg("image")
        .arg("inspect")
        .arg(image)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let mut items: Vec<InspectImage> = serde_json::from_slice(&output.stdout).ok()?;
    let first = items.pop()?;
    let labels = if !first.container_config.labels.is_empty() {
        first.container_config.labels
    } else {
        first.config.labels
    };
    let id = first
        .id
        .map(|s| s.trim_start_matches("sha256:").to_string());
    let version = labels.get("org.opencontainers.image.version").cloned();
    let revision = labels.get("org.opencontainers.image.revision").cloned();
    let title = labels.get("org.opencontainers.image.title").cloned();
    let digest = first
        .repo_digests
        .and_then(|v| v.into_iter().next())
        .map(|d| d.trim().to_string());
    let tag = first
        .repo_tags
        .and_then(|v| v.into_iter().next())
        .map(|t| t.trim().to_string());

    Some(ImageMetadata {
        created: first.created,
        id,
        digest,
        tag,
        title,
        version,
        revision,
    })
}

/// Format image metadata for concise verbose logging.
pub fn format_image_metadata(meta: &ImageMetadata) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(created) = &meta.created {
        parts.push(format!("build={}", created));
    }
    if let Some(version) = &meta.version {
        parts.push(format!("version={}", version));
    }
    if let Some(revision) = &meta.revision {
        parts.push(format!("rev={}", revision));
    }
    if let Some(id) = &meta.id {
        let short = id.chars().take(12).collect::<String>();
        parts.push(format!("id={}", short));
    }
    if let Some(tag) = &meta.tag {
        parts.push(format!("tag={}", tag));
    }
    if let Some(digest) = &meta.digest {
        parts.push(format!("digest={}", digest));
    }
    if let Some(title) = &meta.title {
        parts.push(format!("title={}", title));
    }
    parts.join(" ")
}
