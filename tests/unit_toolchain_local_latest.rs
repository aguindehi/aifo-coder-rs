use std::env;
use std::process::Command;

/// Helper: check whether docker is available; skip tests when not.
fn have_docker() -> bool {
    Command::new("docker")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Ignored integration-style test: verifies that a local ':latest' toolchain image is preferred
/// over the default release-<VER> tag when present.
#[test]
#[ignore]
fn integration_toolchain_rust_prefers_local_latest_when_present() {
    if !have_docker() {
        eprintln!("skipping: docker not available");
        return;
    }

    // Allow docker usage in resolver.
    env::remove_var("AIFO_CODER_TEST_DISABLE_DOCKER");

    // Clear tag overrides so default behavior applies.
    env::remove_var("RUST_TOOLCHAIN_TAG");
    env::remove_var("AIFO_TOOLCHAIN_TAG");
    env::remove_var("AIFO_TAG");
    env::remove_var("AIFO_RUST_TOOLCHAIN_IMAGE");
    env::remove_var("AIFO_RUST_TOOLCHAIN_VERSION");

    // Ensure a local 'aifo-coder-toolchain-rust:latest' exists by tagging an existing image.
    let runtime = aifo_coder::container_runtime_path().expect("docker runtime");
    let local_name = "aifo-coder-toolchain-rust:latest";

    // Tag rust:1-bookworm (or pull it first) to our local latest name.
    let _ = Command::new(&runtime)
        .arg("pull")
        .arg("rust:1-bookworm")
        .status()
        .expect("docker pull rust:1-bookworm");
    let _ = Command::new(&runtime)
        .arg("tag")
        .arg("rust:1-bookworm")
        .arg(local_name)
        .status()
        .expect("docker tag rust:1-bookworm aifo-coder-toolchain-rust:latest");

    let img = aifo_coder::default_toolchain_image("rust");
    assert_eq!(
        img, local_name,
        "default_toolchain_image(rust) should prefer local :latest when present"
    );
}

/// Ignored integration-style test: verifies that a local ':latest' agent image is preferred
/// over the default release-<VER> tag when present.
#[test]
#[ignore]
fn integration_agent_prefers_local_latest_when_present() {
    if !have_docker() {
        eprintln!("skipping: docker not available");
        return;
    }

    env::remove_var("AIFO_CODER_TEST_DISABLE_DOCKER");

    // Clear agent overrides so default behavior applies.
    env::remove_var("AIFO_CODER_AGENT_IMAGE");
    env::remove_var("AIFO_CODER_AGENT_TAG");
    env::remove_var("AIFO_TAG");

    // Ensure a local 'aifo-coder-codex:latest' exists.
    let runtime = aifo_coder::container_runtime_path().expect("docker runtime");
    let local_name = "aifo-coder-codex:latest";

    // Tag a small base image to this name to avoid heavy pulls.
    let _ = Command::new(&runtime)
        .arg("pull")
        .arg("alpine:3.19")
        .status()
        .expect("docker pull alpine:3.19");
    let _ = Command::new(&runtime)
        .arg("tag")
        .arg("alpine:3.19")
        .arg(local_name)
        .status()
        .expect("docker tag alpine:3.19 aifo-coder-codex:latest");

    let img =
        aifo_coder::compute_effective_agent_image_for_run("aifo-coder-codex").expect("resolver");
    assert_eq!(
        img, local_name,
        "effective agent image should prefer local :latest when present"
    );
}
