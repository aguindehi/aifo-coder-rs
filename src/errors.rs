//! Error mapping guide:
//! - Map io::ErrorKind::NotFound to exit code 127; all others to 1.
//! - Prefer ForkError/ToolchainError for internal clarity while preserving user-visible strings via display_* helpers.
//! - Keep mapping/styling consistent with v1; no behavior changes.
use std::io;

/// Map an io::Error to a process exit code, preserving current behavior:
/// - 127 for NotFound (command not found)
/// - 1 for all other errors
pub fn exit_code_for_io_error(e: &io::Error) -> u8 {
    if e.kind() == io::ErrorKind::NotFound {
        127
    } else {
        1
    }
}

/// Lightweight error enums to improve internal error clarity without changing external messages.
#[derive(Debug)]
#[allow(dead_code)]
pub enum ForkError {
    Io(std::io::Error),
    Message(String),
}

#[derive(Debug)]
pub enum ToolchainError {
    Io(std::io::Error),
    Message(String),
}

impl From<std::io::Error> for ForkError {
    fn from(e: std::io::Error) -> Self {
        ForkError::Io(e)
    }
}

impl From<std::io::Error> for ToolchainError {
    fn from(e: std::io::Error) -> Self {
        ToolchainError::Io(e)
    }
}

/// Convert ForkError to exit code (parity with io::Error mapping).
#[allow(dead_code)]
pub fn exit_code_for_fork_error(e: &ForkError) -> u8 {
    match e {
        ForkError::Io(ioe) => exit_code_for_io_error(ioe),
        ForkError::Message(_) => 1,
    }
}

/// Convert ToolchainError to exit code (parity with io::Error mapping).
pub fn exit_code_for_toolchain_error(e: &ToolchainError) -> u8 {
    match e {
        ToolchainError::Io(ioe) => exit_code_for_io_error(ioe),
        ToolchainError::Message(_) => 1,
    }
}

/// Render a user-facing string for ForkError without changing existing texts.
#[allow(dead_code)]
pub fn display_for_fork_error(e: &ForkError) -> String {
    match e {
        ForkError::Io(ioe) => ioe.to_string(),
        ForkError::Message(s) => s.clone(),
    }
}

/// Render a user-facing string for ToolchainError without changing existing texts.
pub fn display_for_toolchain_error(e: &ToolchainError) -> String {
    match e {
        ToolchainError::Io(ioe) => ioe.to_string(),
        ToolchainError::Message(s) => s.clone(),
    }
}
