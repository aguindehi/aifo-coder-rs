use std::process::Command;

fn make_available() -> bool {
    Command::new("make").arg("--version").output().is_ok()
}

#[test]
fn test_make_dry_run_publish_toolchain_rust_preview() {
    if !make_available() {
        eprintln!("skipping: make not found in PATH");
        return;
    }
    let out = Command::new("make")
        .args(["-n", "publish-toolchain-rust"])
        .env("PLATFORMS", "linux/amd64,linux/arm64")
        .env("PUSH", "0")
        .output()
        .expect("run make -n publish-toolchain-rust");
    assert!(
        out.status.success(),
        "make -n publish-toolchain-rust failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let s = String::from_utf8_lossy(&out.stdout);
    // Expect the dry-run script to contain our Dockerfile path and image name
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
    // Expect the non-push branch message to be present
    assert!(
        s.contains("PUSH=0: building locally"),
        "expected PUSH=0 local build message, got:\n{}",
        s
    );
}
