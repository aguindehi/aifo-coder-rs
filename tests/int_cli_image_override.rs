use std::process::Command;

#[test]
fn int_test_cli_image_override_dry_run_uses_override() {
    // Skip if docker isn't available on this host (dry-run still needs docker path for preview)
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    let bin = env!("CARGO_BIN_EXE_aifo-coder");
    let out = Command::new(bin)
        .args([
            "--verbose",
            "--dry-run",
            "--image",
            "alpine:3.20",
            "aider",
            "--",
            "--version",
        ])
        .output()
        .expect("failed to run aifo-coder --image alpine:3.20 aider -- --version");
    assert!(
        out.status.success(),
        "aifo-coder dry-run with image override exited non-zero: {:?}\nstdout:\n{}\nstderr:\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("aifo-coder: agent image [aider]: alpine:3.20"),
        "stderr should show chosen image override in verbose output; stderr:\n{}",
        err
    );
    assert!(
        err.contains("docker:") && err.contains("alpine:3.20"),
        "docker preview should reference alpine:3.20; stderr:\n{}",
        err
    );
}
