fn docker_image_present(runtime: &std::path::Path, image: &str) -> bool {
    std::process::Command::new(runtime)
        .args(["image", "inspect", image])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

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
    // Resolve the image to use: prefer CI/MR-provided toolchain image; else require local default.
    let runtime = aifo_coder::container_runtime_path().expect("runtime");
    let override_img = std::env::var("AIFO_CODER_TEST_CPP_IMAGE").ok();
    if override_img.is_none()
        && !docker_image_present(&runtime, "aifo-coder-toolchain-cpp:latest")
    {
        eprintln!("skipping: aifo-coder-toolchain-cpp:latest not present locally");
        return;
    }
    // Start a c-cpp sidecar and run cmake --version inside it.
    let args = vec!["cmake".to_string(), "--version".to_string()];
    let res = aifo_coder::toolchain_run(
        "c-cpp",
        &args,
        override_img.as_deref(),
        false,
        false,
        false,
    );
    assert!(res.is_ok(), "toolchain_run returned error: {:?}", res.err());
    assert_eq!(res.unwrap(), 0);
}
