use std::process::Command;

#[test]
fn test_cli_doctor_prints_registry_and_security_options_when_docker_present() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    let bin = env!("CARGO_BIN_EXE_aifo-coder");
    let out = Command::new(bin)
        .arg("doctor")
        .output()
        .expect("failed to run aifo-coder doctor");
    assert!(
        out.status.success(),
        "aifo-coder doctor exited non-zero: {:?}\nstdout:\n{}\nstderr:\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("docker registry:"),
        "expected 'docker registry:' line in doctor output; stderr:\n{}",
        err
    );
    assert!(
        err.contains("docker security options:"),
        "expected 'docker security options:' line in doctor output; stderr:\n{}",
        err
    );
}
