#[test]
fn test_build_docker_cmd_passes_editor_env_flag() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // Save and set env EDITOR (passed by name-only -e EDITOR)
    let old_editor = std::env::var("EDITOR").ok();
    std::env::set_var("EDITOR", "vim");

    let args = vec!["--help".to_string()];
    let (_cmd, preview) =
        aifo_coder::build_docker_cmd("aider", &args, "alpine:3.20", None).expect("build_docker_cmd failed");

    assert!(
        preview.contains("-e EDITOR"),
        "missing -e EDITOR pass-through flag: {preview}"
    );

    // Restore env
    if let Some(v) = old_editor {
        std::env::set_var("EDITOR", v);
    } else {
        std::env::remove_var("EDITOR");
    }
}

#[test]
fn test_build_docker_cmd_no_git_sign_injection_by_default() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // Ensure AIFO_CODER_GIT_SIGN is not set
    let old = std::env::var("AIFO_CODER_GIT_SIGN").ok();
    std::env::remove_var("AIFO_CODER_GIT_SIGN");

    let args = vec!["--help".to_string()];
    let (_cmd, preview) =
        aifo_coder::build_docker_cmd("aider", &args, "alpine:3.20", None).expect("build_docker_cmd failed");

    assert!(
        !preview.contains("GIT_CONFIG_KEY_0=commit.gpgsign"),
        "should not inject git-sign config when AIFO_CODER_GIT_SIGN is unset: {preview}"
    );

    // Restore
    if let Some(v) = old {
        std::env::set_var("AIFO_CODER_GIT_SIGN", v);
    }
}
