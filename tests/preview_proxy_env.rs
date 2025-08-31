#[test]
fn test_build_docker_cmd_includes_proxy_env() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // Save and set env
    let old_url = std::env::var("AIFO_TOOLEEXEC_URL").ok();
    let old_tok = std::env::var("AIFO_TOOLEEXEC_TOKEN").ok();

    std::env::set_var("AIFO_TOOLEEXEC_URL", "http://host.docker.internal:54321/exec");
    std::env::set_var("AIFO_TOOLEEXEC_TOKEN", "t0k");

    let args = vec!["--help".to_string()];
    let (_cmd, preview) = aifo_coder::build_docker_cmd("aider", &args, "alpine:3.20", None)
        .expect("build_docker_cmd failed");

    assert!(
        preview.contains("-e AIFO_TOOLEEXEC_URL=http://host.docker.internal:54321/exec"),
        "missing AIFO_TOOLEEXEC_URL mapping: {preview}"
    );
    assert!(
        preview.contains("-e AIFO_TOOLEEXEC_TOKEN=t0k"),
        "missing AIFO_TOOLEEXEC_TOKEN mapping: {preview}"
    );

    // Restore env
    if let Some(v) = old_url { std::env::set_var("AIFO_TOOLEEXEC_URL", v); } else { std::env::remove_var("AIFO_TOOLEEXEC_URL"); }
    if let Some(v) = old_tok { std::env::set_var("AIFO_TOOLEEXEC_TOKEN", v); } else { std::env::remove_var("AIFO_TOOLEEXEC_TOKEN"); }
}
