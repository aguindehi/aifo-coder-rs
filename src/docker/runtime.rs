#![allow(clippy::module_name_repetitions)]
//! Docker runtime discovery.

use std::env;
use std::io;
use std::path::PathBuf;

use which::which;

pub fn container_runtime_path() -> io::Result<PathBuf> {
    // Allow tests or callers to explicitly disable Docker detection to avoid hard failures
    if env::var("AIFO_CODER_TEST_DISABLE_DOCKER").ok().as_deref() == Some("1")
        || env::var("AIFO_CODER_SKIP_DOCKER").ok().as_deref() == Some("1")
    {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Docker disabled by environment override.",
        ));
    }

    if let Ok(p) = which("docker") {
        return Ok(p);
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "Docker is required but was not found in PATH.",
    ))
}
