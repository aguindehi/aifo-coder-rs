#[test]
fn int_preview_contains_no_git_mutations_and_aider_disable_signing_env_still_works() {
    // Isolate HOME so preview mount discovery stays fast and deterministic
    let td = tempfile::tempdir().expect("tmpdir");
    let old_home = std::env::var("HOME").ok();
    std::env::set_var("HOME", td.path());

    // Ensure a stable container name for reproducibility
    std::env::set_var("AIFO_CODER_CONTAINER_NAME", "aifo-coder-unit-test");

    let agents = ["aider", "codex", "crush"];
    for agent in agents {
        let args = vec!["--help".to_string()];
        let preview = aifo_coder::build_docker_preview_only(agent, &args, "alpine:3.20", None);

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
    let preview = aifo_coder::build_docker_preview_only("aider", &args, "alpine:3.20", None);
    assert!(
        preview.contains("GIT_CONFIG_KEY_0=commit.gpgsign"),
        "aider preview must include transient GIT_CONFIG_* for disabling signing; preview:\n{}",
        preview
    );

    // Restore HOME
    if let Some(v) = old_home {
        std::env::set_var("HOME", v);
    } else {
        std::env::remove_var("HOME");
    }
}
