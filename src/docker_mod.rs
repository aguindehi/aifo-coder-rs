#![allow(clippy::module_name_repetitions)]
//! Docker command construction and runtime detection.
//!
//! This module is a thin façade that re-exports stable public APIs while the
//! implementation is split into focused submodules under `docker/`.
//!
//! Structure (issue 5):
//! - docker/runtime.rs: runtime detection / availability
//! - docker/images.rs: image existence and pull helpers
//! - docker/env.rs: env forwarding helpers
//! - docker/mounts.rs: mount policy / validation helpers
//! - docker/run.rs: docker run command construction & previews

#[path = "docker/docker.rs"]
pub(crate) mod docker;

pub use docker::images::image_exists;
pub use docker::run::{build_docker_cmd, build_docker_preview_only};
pub use docker::runtime::container_runtime_path;
pub use docker::staging::{
    cleanup_aider_staging_from_env, compute_effective_agent_image_for_run,
    pull_image_with_autologin,
};
