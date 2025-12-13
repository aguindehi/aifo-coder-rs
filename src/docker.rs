#![allow(clippy::module_name_repetitions)]
//! Docker module entrypoint.
//!
//! This is a stable re-export boundary for crate users.
//! Implementation lives in `docker_mod.rs` and its submodules.

pub use crate::docker_mod::{
    build_docker_cmd, build_docker_preview_only, cleanup_aider_staging_from_env,
    compute_effective_agent_image_for_run, container_runtime_path, image_exists,
};
