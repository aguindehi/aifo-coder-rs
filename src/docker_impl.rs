#![allow(clippy::module_name_repetitions)]
//! Compatibility docker shims.
//!
//! This crate has moved Docker functionality into `src/docker/*` and re-exports
//! stable entry points via `docker_mod` and `docker`.
//!
//! `docker_impl.rs` remains as a *compatibility shim only* to avoid churn in
//! callers that may still `mod docker_impl;` (it is included by src/lib.rs).
//!
//! IMPORTANT: New code must not be added here.

#[allow(dead_code)]
const _DOCKER_IMPL_IS_SHIM_ONLY: () = ();
