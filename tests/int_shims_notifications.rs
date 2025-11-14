#[test]
fn int_say_shim_missing_env_exit86() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    aifo_coder::toolchain_write_shims(tmp.path()).expect("write shims");
    let shim = tmp.path().join("say");
    assert!(shim.exists(), "say shim must exist");

    // Exec shim without proxy env, expect exit 86
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755));
    }
    let status = std::process::Command::new(&shim)
        .env_remove("AIFO_TOOLEEXEC_URL")
        .env_remove("AIFO_TOOLEEXEC_TOKEN")
        .status()
        .expect("exec shim");
    assert_eq!(status.code().unwrap_or(0), 86, "expected exit 86");
}
