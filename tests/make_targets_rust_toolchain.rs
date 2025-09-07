
use std::process::Command;

fn make_available() -> bool {
    Command::new("make").arg("--version").output().is_ok()
}

#[test]
fn test_make_help_lists_rust_toolchain_targets() {
    if !make_available() {
        eprintln!("skipping: make not found in PATH");
        return;
    }
    let out = Command::new("make")
        .args(["-n", "help"])
        .output()
        .expect("run make -n help");
    assert!(
        out.status.success(),
        "make -n help failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let s = String::from_utf8_lossy(&out.stdout);
    for needle in [
        "build-toolchain-rust",
        "rebuild-toolchain-rust",
        "publish-toolchain-rust",
    ] {
        assert!(
            s.contains(needle),
            "expected help to mention '{}', got:\n{}",
            needle,
            s
        );
    }
}
