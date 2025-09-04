#[test]
fn test_toolchain_write_shims_creates_files() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    aifo_coder::toolchain_write_shims(tmp.path()).expect("write shims");
    for t in [
        "aifo-shim",
        "cargo",
        "rustc",
        "node",
        "npm",
        "npx",
        "tsc",
        "ts-node",
        "python",
        "pip",
        "pip3",
        "gcc",
        "g++",
        "clang",
        "clang++",
        "make",
        "cmake",
        "ninja",
        "pkg-config",
        "go",
        "gofmt",
        "notifications-cmd",
    ] {
        assert!(tmp.path().join(t).exists(), "missing shim: {}", t);
    }
}

#[cfg(unix)]
#[test]
fn test_aifo_shim_exec_without_env_exits_86() {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;

    let tmp = tempfile::tempdir().expect("tmpdir");
    aifo_coder::toolchain_write_shims(tmp.path()).expect("write shims");

    // Ensure executable bit (already set in implementation, but keep explicit)
    let shim = tmp.path().join("cargo");
    let _ = fs::set_permissions(&shim, fs::Permissions::from_mode(0o755));

    // Run without AIFO_TOOLEEXEC_* env → expect exit 86
    let status = Command::new(&shim)
        .env_remove("AIFO_TOOLEEXEC_URL")
        .env_remove("AIFO_TOOLEEXEC_TOKEN")
        .status()
        .expect("failed to exec shim");
    let code = status.code().unwrap_or(0);
    assert_eq!(
        code, 86,
        "expected exit 86 when proxy env missing, got {}",
        code
    );
}
