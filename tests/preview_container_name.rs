#[test]
fn test_build_docker_cmd_respects_container_name_env() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    let old = std::env::var("AIFO_CODER_CONTAINER_NAME").ok();
    std::env::set_var("AIFO_CODER_CONTAINER_NAME", "unit-test-cn");

    let args = vec!["--help".to_string()];
    let (_cmd, preview) = aifo_coder::build_docker_cmd("aider", &args, "alpine:3.20", None)
        .expect("build_docker_cmd failed");

    assert!(
        preview.contains("--name unit-test-cn"),
        "preview missing custom container name: {preview}"
    );
    assert!(
        preview.contains("--hostname unit-test-cn"),
        "preview missing custom hostname: {preview}"
    );

    // Restore env
    if let Some(v) = old {
        std::env::set_var("AIFO_CODER_CONTAINER_NAME", v);
    } else {
        std::env::remove_var("AIFO_CODER_CONTAINER_NAME");
    }
}
