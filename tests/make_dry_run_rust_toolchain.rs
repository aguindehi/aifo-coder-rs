use std::process::Command;

fn make_available() -> bool {
    Command::new("make").arg("--version").output().is_ok()
}

#[test]
fn test_make_dry_run_build_toolchain_rust() {
    if !make_available() {
        eprintln!("skipping: make not found in PATH");
        return;
    }
    let out = Command::new("make")
        .args(["-n", "build-toolchain-rust"])
        .output()
        .expect("run make -n build-toolchain-rust");
    assert!(
        out.status.success(),
        "make -n build-toolchain-rust failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(
        s.contains("toolchains/rust/Dockerfile"),
        "expected Dockerfile path in output, got:\n{}",
        s
    );
    assert!(
        s.contains("aifo-coder-toolchain-rust:"),
        "expected image tag in output, got:\n{}",
        s
    );
    assert!(
        s.contains("docker build"),
        "expected a docker build invocation in output, got:\n{}",
        s
    );
}
