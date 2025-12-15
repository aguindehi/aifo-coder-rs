#![doc = include_str!("lib_docs.rs")]

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
