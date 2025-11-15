#[test]
fn int_test_build_docker_cmd_disables_git_sign_for_aider() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // Save and set env
    let old = std::env::var("AIFO_CODER_GIT_SIGN").ok();
    std::env::set_var("AIFO_CODER_GIT_SIGN", "0");

    let args = vec!["--help".to_string()];
    let (_cmd, preview) = aifo_coder::build_docker_cmd("aider", &args, "alpine:3.20", None)
        .expect("build_docker_cmd failed");

    // Expect the GIT config disable env triplet
    assert!(
        preview.contains("-e GIT_CONFIG_COUNT=1"),
        "missing GIT_CONFIG_COUNT=1 mapping: {preview}"
    );
    assert!(
        preview.contains("-e GIT_CONFIG_KEY_0=commit.gpgsign"),
        "missing GIT_CONFIG_KEY_0=commit.gpgsign mapping: {preview}"
    );
    assert!(
        preview.contains("-e GIT_CONFIG_VALUE_0=false"),
        "missing GIT_CONFIG_VALUE_0=false mapping: {preview}"
    );

    // Restore env
    if let Some(v) = old {
        std::env::set_var("AIFO_CODER_GIT_SIGN", v);
    } else {
        std::env::remove_var("AIFO_CODER_GIT_SIGN");
    }
}
