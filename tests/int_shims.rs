#[cfg(unix)]
#[test]
fn int_aifo_shim_exec_without_env_exits_86() {
    use std::process::Command;

    let shim = env!("CARGO_BIN_EXE_aifo-shim");
    let status = Command::new(shim)
        .arg("--version")
        .env_remove("AIFO_TOOLEEXEC_URL")
        .env_remove("AIFO_TOOLEEXEC_TOKEN")
        .status()
        .expect("exec aifo-shim");
    assert_eq!(status.code().unwrap_or(0), 86, "expected exit 86");
}
