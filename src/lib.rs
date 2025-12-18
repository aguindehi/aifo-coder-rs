#![allow(clippy::doc_overindented_list_items)]
//! AIFO Coder crate: architecture overview, environment invariants and module map.
//!
//! Architecture
//! - Binary glue (src/main.rs) orchestrates CLI, banner/doctor, fork lifecycle and toolchain session.
//! - Library exports are stable and used across modules and tests; most helpers live under fork::* and toolchain::*.
//!
//! Key modules
//! - fork::*: repo detection, snapshot/clone/merge/cleanup, orchestrators, guidance and summaries.
//! - toolchain::*: sidecar lifecycle, proxy/shim, routing/allowlists, notifications and HTTP helpers.
//! - util::*: small helpers (shell/json escaping, URL decoding, Docker security parsing, fs utilities).
//! - color.rs: color mode and paint/log wrappers (exact strings preserved).
//! - apparmor.rs: host AppArmor detection and profile selection helpers.
//!
//! Environment invariants (documented for contributors)
//! - AIFO_TOOLEEXEC_URL/TOKEN: exported by proxy start; injected into agent env; respected by shims.
//! - AIFO_SESSION_NETWORK: session network name (aifo-net-<id>) to join; removed on cleanup.
//! - AIFO_TOOLEEXEC_ADD_HOST (Linux): when "1", add host-gateway entry; used for troubleshooting.
//! - AIFO_CODER_CONTAINER_NAME/HOSTNAME: stable container name/hostname per pane/session.
//! - AIFO_CODER_FORK_*: pane/session metadata exported to orchestrated shells/sessions.
//! - AIFO_CODER_COLOR / NO_COLOR: crate-wide color control; wrappers always preserve message text.
//!
//! Style guidance
//! - Prefer lines <= 100 chars where feasible in non-golden code; never change user-visible strings.
//! - Module-level docs should summarize purpose and invariants to aid contributors.
mod apparmor;
#[allow(clippy::doc_overindented_list_items)]
mod color;
mod docker;
mod docker_mod;
mod errors;
mod fork;
#[path = "fork/meta.rs"]
pub mod fork_meta;
#[path = "fork/strategy.rs"]
mod fork_strategy;
#[cfg(windows)]
#[path = "fork/windows/helpers.rs"]
mod fork_windows_helpers;
mod lock;
mod registry;
#[cfg(feature = "otel")]
mod telemetry;
mod toolchain;
mod ui;
mod util;

pub mod shim;
pub use apparmor::*;
pub use color::*;
pub use docker::*;
pub use errors::exit_code_for_io_error;
pub use errors::{display_for_fork_error, display_for_toolchain_error};
pub use errors::{exit_code_for_fork_error, exit_code_for_toolchain_error};
pub use errors::{ForkError, ToolchainError};
pub use fork::*;
pub use fork_strategy::MergingStrategy;
#[cfg(windows)]
pub use fork_windows_helpers::{
    fork_bash_inner_string, fork_ps_inner_string, ps_wait_process_cmd, wt_build_new_tab_args,
    wt_build_split_args, wt_orient_for_layout,
};
pub use lock::*;
pub use registry::*;
pub use toolchain::*;
pub use ui::warn::{warn_print, warn_prompt_continue_or_quit};
pub use util::docker_security::{docker_security_options_parse, DockerSecurityOptions};
pub use util::fs::{ensure_file_exists, path_pair};
pub use util::id::create_session_id;
pub use util::reject_newlines;
pub use util::*;

#[cfg(feature = "otel")]
pub use telemetry::{record_run_end, record_run_start, telemetry_init, TelemetryGuard};

#[cfg(not(feature = "otel"))]
pub fn telemetry_init() -> Option<()> {
    None
}
