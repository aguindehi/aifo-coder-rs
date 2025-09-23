#[test]
fn preview_contains_no_git_mutations_and_aider_disable_signing_env_still_works() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // Ensure a stable container name for reproducibility
    std::env::set_var("AIFO_CODER_CONTAINER_NAME", "aifo-coder-unit-test");

    let agents = ["aider", "codex", "crush"];
    for agent in agents {
        let args = vec!["--help".to_string()];
        let (_cmd, preview) = aifo_coder::build_docker_cmd(agent, &args, "alpine:3.20", None)
            .expect("build_docker_cmd");

        // No in-container git mutations
        assert!(
            !preview.contains("git -C /workspace config commit.gpgsign"),
            "preview must not set commit.gpgsign; preview:\n{}",
            preview
        );
        assert!(
            !preview.contains("git -C /workspace config gpg.program"),
            "preview must not set gpg.program; preview:\n{}",
            preview
        );
        assert!(
            !preview.contains("git -C /workspace config user.signingkey"),
            "preview must not set user.signingkey; preview:\n{}",
            preview
        );
        assert!(
            !preview.contains("GIT_AUTHOR_NAME=") && !preview.contains("GIT_AUTHOR_EMAIL="),
            "preview must not export GIT_AUTHOR_* automatically; preview:\n{}",
            preview
        );
    }

    // Aider-specific: disabling signing via env should still inject transient GIT_CONFIG_* env
    std::env::set_var("AIFO_CODER_GIT_SIGN", "0");
    let args = vec!["--help".to_string()];
    let (_cmd, preview) = aifo_coder::build_docker_cmd("aider", &args, "alpine:3.20", None)
        .expect("build_docker_cmd");
    assert!(
        preview.contains("GIT_CONFIG_KEY_0=commit.gpgsign"),
        "aider preview must include transient GIT_CONFIG_* for disabling signing; preview:\n{}",
        preview
    );
}
