use std::process::Command;

#[test]
fn test_cli_dry_run_aider_previews_docker_cmd() {
    // Skip if docker isn't available on this host (dry-run still builds preview using docker path)
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    let bin = env!("CARGO_BIN_EXE_aifo-coder");
    let out = Command::new(bin)
        .args(["--verbose", "--dry-run", "aider", "--", "--version"])
        .output()
        .expect("failed to run aifo-coder --dry-run aider");

    assert!(
        out.status.success(),
        "aifo-coder --dry-run aider exited non-zero: {:?}\nstdout:\n{}\nstderr:\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("aifo-coder: docker:"),
        "expected docker preview in stderr, got:\n{}",
        err
    );
}
