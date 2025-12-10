#[test]
fn unit_route_tool_to_sidecar_mapping() {
    assert_eq!(aifo_coder::route_tool_to_sidecar("cargo"), "rust");
    assert_eq!(aifo_coder::route_tool_to_sidecar("rustc"), "rust");

    assert_eq!(aifo_coder::route_tool_to_sidecar("node"), "node");
    assert_eq!(aifo_coder::route_tool_to_sidecar("npm"), "node");
    assert_eq!(aifo_coder::route_tool_to_sidecar("npx"), "node");
    assert_eq!(aifo_coder::route_tool_to_sidecar("yarn"), "node");
    assert_eq!(aifo_coder::route_tool_to_sidecar("pnpm"), "node");
    assert_eq!(aifo_coder::route_tool_to_sidecar("deno"), "node");
    assert_eq!(aifo_coder::route_tool_to_sidecar("tsc"), "node");
    assert_eq!(aifo_coder::route_tool_to_sidecar("ts-node"), "node");

    assert_eq!(aifo_coder::route_tool_to_sidecar("python"), "python");
    assert_eq!(aifo_coder::route_tool_to_sidecar("python3"), "python");
    assert_eq!(aifo_coder::route_tool_to_sidecar("pip"), "python");
    assert_eq!(aifo_coder::route_tool_to_sidecar("pip3"), "python");
    assert_eq!(aifo_coder::route_tool_to_sidecar("uv"), "python");
    assert_eq!(aifo_coder::route_tool_to_sidecar("uvx"), "python");

    assert_eq!(aifo_coder::route_tool_to_sidecar("gcc"), "c-cpp");
    assert_eq!(aifo_coder::route_tool_to_sidecar("g++"), "c-cpp");
    assert_eq!(aifo_coder::route_tool_to_sidecar("clang"), "c-cpp");
    assert_eq!(aifo_coder::route_tool_to_sidecar("clang++"), "c-cpp");
    assert_eq!(aifo_coder::route_tool_to_sidecar("make"), "c-cpp");
    assert_eq!(aifo_coder::route_tool_to_sidecar("cmake"), "c-cpp");
    assert_eq!(aifo_coder::route_tool_to_sidecar("ninja"), "c-cpp");
    assert_eq!(aifo_coder::route_tool_to_sidecar("pkg-config"), "c-cpp");

    assert_eq!(aifo_coder::route_tool_to_sidecar("go"), "go");
    assert_eq!(aifo_coder::route_tool_to_sidecar("gofmt"), "go");

    // Unknown tools default to node sidecar (defense-in-depth allowlist applies)
    assert_eq!(aifo_coder::route_tool_to_sidecar("unknown-tool"), "node");
}
