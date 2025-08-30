#[ignore]
#[test]
fn test_toolchain_live_rust_version_ok() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }
    // This will start a rust sidecar and run cargo --version inside it.
    // We only assert exit code 0 to keep the test robust across environments.
    let args = vec!["cargo".to_string(), "--version".to_string()];
    let res = aifo_coder::toolchain_run("rust", &args, None, false, false, false);
    assert!(res.is_ok(), "toolchain_run returned error: {:?}", res.err());
    assert_eq!(res.unwrap(), 0);
}

#[ignore]
#[test]
fn test_toolchain_live_node_npx_ok() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }
    // Start a node sidecar and run npx --version inside it.
    let args = vec!["npx".to_string(), "--version".to_string()];
    let res = aifo_coder::toolchain_run("node", &args, None, false, false, false);
    assert!(res.is_ok(), "toolchain_run returned error: {:?}", res.err());
    assert_eq!(res.unwrap(), 0);
}
