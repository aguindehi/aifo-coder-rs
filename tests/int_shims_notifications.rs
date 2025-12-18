#[test]
fn int_say_shim_missing_env_exit86() {
    let shim = env!("CARGO_BIN_EXE_aifo-shim");
    let status = std::process::Command::new(shim)
        .arg("hi")
        .env_remove("AIFO_TOOLEEXEC_URL")
        .env_remove("AIFO_TOOLEEXEC_TOKEN")
        .status()
        .expect("exec aifo-shim");
    assert_eq!(status.code().unwrap_or(0), 86, "expected exit 86");
}
