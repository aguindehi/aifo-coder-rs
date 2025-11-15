use std::process::Command;

#[test]
fn int_test_doctor_succeeds_without_docker() {
    let bin = env!("CARGO_BIN_EXE_aifo-coder");
    // Clear PATH to hide docker/colima for this subprocess
    let out = Command::new(bin)
        .arg("doctor")
        .env("PATH", "")
        .output()
        .expect("run aifo-coder doctor");
    assert!(out.status.success(), "doctor should succeed without docker");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("docker command:  (not found)"),
        "doctor should report docker '(not found)'; stderr:\n{}",
        err
    );
}
