#![allow(clippy::module_name_repetitions)]
//! Docker module entrypoint.
//
// This file exists for the "docker.rs is doing too much" refactor (issue 5).
// The implementation has been moved to `docker_mod.rs`; keep this module as a
// thin re-export layer to preserve the public crate API and avoid touching
// unrelated call sites.
//
// New code should go into `docker_mod.rs` (or future submodules under `docker/`).

pub use crate::docker_mod::*;
