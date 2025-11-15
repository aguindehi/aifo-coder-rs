use std::env;

#[test]
fn int_test_build_docker_cmd_respects_hostname_env_separately_from_name() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    let old_name = env::var("AIFO_CODER_CONTAINER_NAME").ok();
    let old_host = env::var("AIFO_CODER_HOSTNAME").ok();
    env::set_var("AIFO_CODER_CONTAINER_NAME", "unit-test-cn");
    env::set_var("AIFO_CODER_HOSTNAME", "unit-test-hn");

    let args = vec!["--help".to_string()];
    let (_cmd, preview) = aifo_coder::build_docker_cmd("aider", &args, "alpine:3.20", None)
        .expect("build_docker_cmd");

    // Restore env
    if let Some(v) = old_name {
        env::set_var("AIFO_CODER_CONTAINER_NAME", v);
    } else {
        env::remove_var("AIFO_CODER_CONTAINER_NAME");
    }
    if let Some(v) = old_host {
        env::set_var("AIFO_CODER_HOSTNAME", v);
    } else {
        env::remove_var("AIFO_CODER_HOSTNAME");
    }

    assert!(
        preview.contains("--name unit-test-cn"),
        "expected --name unit-test-cn in preview: {}",
        preview
    );
    assert!(
        preview.contains("--hostname unit-test-hn"),
        "expected --hostname unit-test-hn in preview: {}",
        preview
    );
}
