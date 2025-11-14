#[test]
fn test_toolchain_dry_run_rust_ok() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }
    let args = vec!["cargo".to_string(), "--version".to_string()];
    let res = aifo_coder::toolchain_run("rust", &args, None, false, true, true);
    assert!(res.is_ok(), "toolchain_run returned error: {:?}", res.err());
    assert_eq!(res.unwrap(), 0);
}

#[test]
fn test_toolchain_dry_run_no_cache_node_ok() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }
    let args = vec!["npx".to_string(), "--version".to_string()];
    let res = aifo_coder::toolchain_run("node", &args, None, true, true, true);
    assert!(res.is_ok(), "toolchain_run returned error: {:?}", res.err());
    assert_eq!(res.unwrap(), 0);
}
