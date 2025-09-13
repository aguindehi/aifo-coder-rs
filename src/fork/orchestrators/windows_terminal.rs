#![cfg(windows)]

use std::process::Command;

use super::super::types::{ForkSession, Pane};
use super::Orchestrator;

/// Windows Terminal orchestrator (non-waitable). This struct provides a minimal
/// implementation so the module is usable during refactoring. It intentionally
/// does not alter user-visible behavior yet; main.rs continues to drive WT flows.
///
/// Notes per spec:
/// - Previews should use wt_* helpers with argv[0] ("wt") included.
/// - When executing via Command::new(wt_path), drop argv[0] from helper vectors.
/// - Post-merge is not supported directly (non-waitable); callers must handle guidance.
pub struct WindowsTerminal;

impl Orchestrator for WindowsTerminal {
    fn launch(&self, _session: &ForkSession, _panes: &[Pane], _child_args: &[String]) -> Result<(), String> {
        // Defer to existing main.rs logic for now.
        Ok(())
    }

    fn supports_post_merge(&self) -> bool {
        false
    }
}

// Helper: execute a wt command built by wt_* helpers, dropping argv[0] ("wt") when running.
#[allow(dead_code)]
fn exec_wt_dropping_argv0(wt_path: &std::path::Path, args_with_argv0: &[String]) -> std::io::Result<std::process::ExitStatus> {
    let mut cmd = Command::new(wt_path);
    for a in args_with_argv0.iter().skip(1) {
        cmd.arg(a);
    }
    cmd.status()
}
