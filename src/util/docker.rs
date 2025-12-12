/*!
Docker helpers shared across modules (no I/O other than invoking docker runtime).

Currently provides:
- image_exists: check if a Docker image is present locally via `docker image inspect`.
*/
use std::path::Path;
use std::process::{Command, Stdio};

/// Return true if a docker image exists locally (without pulling).
pub fn image_exists(runtime: &Path, image: &str) -> bool {
    Command::new(runtime)
        .arg("image")
        .arg("inspect")
        .arg(image)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
