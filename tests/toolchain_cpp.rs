#[test]
fn test_toolchain_dry_run_c_cpp_ok() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }
    let args = vec!["cmake".to_string(), "--version".to_string()];
    let res = aifo_coder::toolchain_run("c-cpp", &args, None, true, true, true);
    assert!(res.is_ok(), "toolchain_run returned error: {:?}", res.err());
    assert_eq!(res.unwrap(), 0);
}

#[ignore]
#[test]
fn test_toolchain_live_c_cpp_cmake_ok() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }
    // Start a c-cpp sidecar and run cmake --version inside it.
    let args = vec!["cmake".to_string(), "--version".to_string()];
    let res = aifo_coder::toolchain_run("c-cpp", &args, None, false, false, false);
    assert!(res.is_ok(), "toolchain_run returned error: {:?}", res.err());
    assert_eq!(res.unwrap(), 0);
}
