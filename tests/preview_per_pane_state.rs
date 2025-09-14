#[test]
fn test_build_docker_cmd_uses_per_pane_state_mounts() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    let td = tempfile::tempdir().expect("tmpdir");
    let state_dir = td.path().to_path_buf();

    // Save and set env
    let old = std::env::var("AIFO_CODER_FORK_STATE_DIR").ok();
    std::env::set_var("AIFO_CODER_FORK_STATE_DIR", &state_dir);

    let args = vec!["--help".to_string()];
    let (_cmd, preview) =
        aifo_coder::build_docker_cmd("aider", &args, "alpine:3.20", None).expect("build_docker_cmd");

    let sd_aider = format!("{}:/home/coder/.aider", state_dir.join(".aider").display());
    let sd_codex = format!("{}:/home/coder/.codex", state_dir.join(".codex").display());
    let sd_crush = format!("{}:/home/coder/.crush", state_dir.join(".crush").display());

    assert!(
        preview.contains(&sd_aider),
        "preview missing per-pane .aider mount: {}",
        preview
    );
    assert!(
        preview.contains(&sd_codex),
        "preview missing per-pane .codex mount: {}",
        preview
    );
    assert!(
        preview.contains(&sd_crush),
        "preview missing per-pane .crush mount: {}",
        preview
    );

    // Ensure home-based mounts for these dirs are not present when per-pane state is set
    if let Some(home) = home::home_dir() {
        let home_aider = format!("{}:/home/coder/.aider", home.join(".aider").display());
        let home_codex = format!("{}:/home/coder/.codex", home.join(".codex").display());
        let home_crush1 = format!(
            "{}:/home/coder/.local/share/crush",
            home.join(".local").join("share").join("crush").display()
        );
        let home_crush2 = format!("{}:/home/coder/.crush", home.join(".crush").display());
        assert!(
            !preview.contains(&home_aider),
            "preview should not include HOME .aider when per-pane state is set: {}",
            preview
        );
        assert!(
            !preview.contains(&home_codex),
            "preview should not include HOME .codex when per-pane state is set: {}",
            preview
        );
        assert!(
            !preview.contains(&home_crush1),
            "preview should not include HOME .local/share/crush when per-pane state is set: {}",
            preview
        );
        assert!(
            !preview.contains(&home_crush2),
            "preview should not include HOME .crush when per-pane state is set: {}",
            preview
        );
    }

    // Restore env
    if let Some(v) = old {
        std::env::set_var("AIFO_CODER_FORK_STATE_DIR", v);
    } else {
        std::env::remove_var("AIFO_CODER_FORK_STATE_DIR");
    }
}
