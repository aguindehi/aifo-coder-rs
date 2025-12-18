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

#[test]
fn unit_shim_tool_names_include_uv_and_uvx() {
    let tools = aifo_coder::shim_tool_names();
    assert!(tools.contains(&"uv"), "shim_tool_names must include 'uv'");
    assert!(tools.contains(&"uvx"), "shim_tool_names must include 'uvx'");
}
