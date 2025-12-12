#![allow(clippy::module_name_repetitions)]
//! Docker command construction and runtime detection.

mod docker_impl;

pub use docker_impl::{
    build_docker_cmd, build_docker_preview_only, cleanup_aider_staging_from_env,
    compute_effective_agent_image_for_run, container_runtime_path, image_exists,
};
