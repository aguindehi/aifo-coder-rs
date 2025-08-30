use std::process::Command;

#[test]
fn test_toolchain_image_override_takes_precedence_over_version_spec() {
    // Skip if docker isn't available on this host (dry-run still builds preview using docker path)
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    let bin = env!("CARGO_BIN_EXE_aifo-coder");

    // Provide a conflicting version spec and explicit image override; expect the override to win.
    let out = Command::new(bin)
        .args([
            "--verbose",
            "--dry-run",
            "--toolchain-spec",
            "rust@1.70",
            "--toolchain-image",
            "rust=rust:1.80-slim",
            "aider",
            "--",
            "--version",
        ])
        .output()
        .expect("failed to run aifo-coder with toolchain overrides");

    assert!(
        out.status.success(),
        "aifo-coder --dry-run with overrides exited non-zero: {:?}\nstdout:\n{}\nstderr:\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    // The verbose dry-run logs include a debug print of image overrides; assert the chosen image appears.
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("rust:1.80-slim"),
        "expected explicit image override to appear in verbose output, got:\n{}",
        err
    );
}
