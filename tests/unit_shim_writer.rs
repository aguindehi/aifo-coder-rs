#[cfg(unix)]
#[test]
fn unit_test_aifo_shim_exits_86_without_proxy_env() {
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

