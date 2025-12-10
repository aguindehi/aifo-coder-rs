#[test]
fn unit_test_aifo_shim_uses_data_urlencode_and_bearer_only() {
    let td = tempfile::tempdir().expect("tmpdir");
    aifo_coder::toolchain_write_shims(td.path()).expect("write shims");

    let shim_path = td.path().join("aifo-shim");
    let content = std::fs::read_to_string(&shim_path).expect("read aifo-shim");

    assert!(
        content.contains("--data-urlencode"),
        "aifo-shim should use --data-urlencode for form fields; content:\n{}",
        content
    );
    assert!(
        content.contains("Authorization: Bearer "),
        "aifo-shim must send Authorization: Bearer header; content:\n{}",
        content
    );
    assert!(
        !content.contains("Proxy-Authorization:"),
        "aifo-shim must not send Proxy-Authorization; content:\n{}",
        content
    );
    assert!(
        !content.contains("X-Aifo-Token:"),
        "aifo-shim must not send X-Aifo-Token; content:\n{}",
        content
    );
    assert!(
        content.contains("traceparent: $TRACEPARENT"),
        "aifo-shim must propagate TRACEPARENT as traceparent header when set; content:\n{}",
        content
    );
}

#[test]
fn unit_shim_tool_names_include_uv_and_uvx() {
    let tools = aifo_coder::shim_tool_names();
    assert!(tools.contains(&"uv"), "shim_tool_names must include 'uv'");
    assert!(tools.contains(&"uvx"), "shim_tool_names must include 'uvx'");
}
