#[test]
fn test_build_docker_cmd_includes_network_and_add_host() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // Save and set env for session network and add-host (Linux only)
    let old_net = std::env::var("AIFO_SESSION_NETWORK").ok();
    let _old_add = std::env::var("AIFO_TOOLEEXEC_ADD_HOST").ok();

    std::env::set_var("AIFO_SESSION_NETWORK", "aifo-net-test123");
    #[cfg(target_os = "linux")]
    std::env::set_var("AIFO_TOOLEEXEC_ADD_HOST", "1");

    let args = vec!["--version".to_string()];
    let (_cmd, preview) = aifo_coder::build_docker_cmd("aider", &args, "alpine:3.20", None)
        .expect("build_docker_cmd failed");

    assert!(
        preview.contains("--network aifo-net-test123"),
        "preview missing session network: {preview}"
    );
    #[cfg(target_os = "linux")]
    assert!(
        preview.contains("--add-host host.docker.internal:host-gateway"),
        "preview missing --add-host for host-gateway: {preview}"
    );

    // Restore env
    if let Some(v) = old_net { std::env::set_var("AIFO_SESSION_NETWORK", v); } else { std::env::remove_var("AIFO_SESSION_NETWORK"); }
    #[cfg(target_os = "linux")]
    {
        if let Some(v) = _old_add { std::env::set_var("AIFO_TOOLEEXEC_ADD_HOST", v); } else { std::env::remove_var("AIFO_TOOLEEXEC_ADD_HOST"); }
    }
}
