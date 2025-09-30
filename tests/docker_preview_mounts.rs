#[test]
fn test_preview_mounts_for_opencode_include_config_share_cache() {
    // Ensure HOME-based mounts
    std::env::remove_var("AIFO_CODER_FORK_STATE_DIR");
    let td = tempfile::tempdir().expect("tmpdir");
    std::env::set_var("HOME", td.path());

    let preview = aifo_coder::build_docker_preview_only(
        "opencode",
        &[],
        "example:latest",
        None,
    );

    let cfg = format!(
        "{}/.config/opencode:/home/coder/.config/opencode",
        td.path().display()
    );
    let share = format!(
        "{}/.local/share/opencode:/home/coder/.local/share/opencode",
        td.path().display()
    );
    let cache = format!(
        "{}/.cache/opencode:/home/coder/.cache/opencode",
        td.path().display()
    );

    assert!(
        preview.contains(&cfg),
        "expected config mount missing; preview:\n{}",
        preview
    );
    assert!(
        preview.contains(&share),
        "expected share mount missing; preview:\n{}",
        preview
    );
    assert!(
        preview.contains(&cache),
        "expected cache mount missing; preview:\n{}",
        preview
    );
}

#[test]
fn test_preview_mounts_for_openhands_include_user_dir() {
    std::env::remove_var("AIFO_CODER_FORK_STATE_DIR");
    let td = tempfile::tempdir().expect("tmpdir");
    std::env::set_var("HOME", td.path());

    let preview = aifo_coder::build_docker_preview_only(
        "openhands",
        &[],
        "example:latest",
        None,
    );

    let expected = format!(
        "{}/.openhands:/home/coder/.openhands",
        td.path().display()
    );
    assert!(
        preview.contains(&expected),
        "expected openhands mount missing; preview:\n{}",
        preview
    );
}

#[test]
fn test_preview_mounts_for_plandex_include_home_dir() {
    std::env::remove_var("AIFO_CODER_FORK_STATE_DIR");
    let td = tempfile::tempdir().expect("tmpdir");
    std::env::set_var("HOME", td.path());

    let preview = aifo_coder::build_docker_preview_only(
        "plandex",
        &[],
        "example:latest",
        None,
    );

    let expected = format!(
        "{}/.plandex-home:/home/coder/.plandex-home",
        td.path().display()
    );
    assert!(
        preview.contains(&expected),
        "expected plandex mount missing; preview:\n{}",
        preview
    );
}
