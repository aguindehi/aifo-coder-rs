#[test]
fn int_toolchain_cleanup_removes_containers_and_network() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // Use rust sidecar (skip if image not present locally to avoid pulling)
    let kinds = vec!["rust".to_string()];
    let image = std::env::var("AIFO_CODER_TEST_RUST_IMAGE")
        .unwrap_or_else(|_| "rust:1.80-slim".to_string());
    let present = std::process::Command::new("docker")
        .args(["image", "inspect", &image])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !present {
        eprintln!("skipping: test image not present locally: {}", image);
        return;
    }
    let overrides: Vec<(String, String)> = vec![("rust".to_string(), image.clone())];

    // Force a deterministic session id + network to exercise creation/removal.
    let sid = "net-cleanup-test";
    std::env::set_var("AIFO_CODER_FORK_SESSION", sid);
    aifo_coder::set_session_network_env(&format!("aifo-net-{}", sid), true, true, "test");

    let sid = aifo_coder::toolchain_start_session(&kinds, &overrides, true, true)
        .expect("failed to start sidecar session");
    let net = format!("aifo-net-{}", sid);
    let cname = format!("aifo-tc-rust-{}", sid);

    // Assert container exists
    let st_c = std::process::Command::new("docker")
        .args(["ps", "-a", "--format", "{{.Names}}"])
        .output()
        .expect("docker ps failed");
    let list = String::from_utf8_lossy(&st_c.stdout);
    assert!(
        list.contains(&cname),
        "expected container {} to exist, got: {}",
        cname,
        list
    );

    // Cleanup session
    aifo_coder::toolchain_cleanup_session(&sid, true);

    // Assert container is gone (inspect should fail)
    let st_inspect = std::process::Command::new("docker")
        .args(["inspect", &cname])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("docker inspect failed to run");
    assert!(
        !st_inspect.success(),
        "container {} still exists after cleanup",
        cname
    );

    // Assert network removed (network inspect should fail)
    let st_net = std::process::Command::new("docker")
        .args(["network", "inspect", &net])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("docker network inspect failed to run");
    assert!(
        !st_net.success(),
        "network {} still exists after cleanup",
        net
    );

    // Clean env for other tests
    std::env::remove_var("AIFO_CODER_FORK_SESSION");
    std::env::remove_var("AIFO_SESSION_NETWORK");
    std::env::remove_var("AIFO_SESSION_NETWORK_SOURCE");
    std::env::remove_var("AIFO_SESSION_NETWORK_MANAGED");
    std::env::remove_var("AIFO_SESSION_NETWORK_CREATE");
}
