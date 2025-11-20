#[test]
fn int_config_env_passthrough_flags_present_in_preview() {
    // Use preview-only to avoid requiring docker runtime for this test
    // Set policy knobs and confirm they appear in env flags
    std::env::set_var("AIFO_CONFIG_ALLOW_EXT", "json,ini");
    std::env::set_var("AIFO_CONFIG_COPY_ALWAYS", "1");
    std::env::set_var("AIFO_CONFIG_MAX_SIZE", "1024");

    let args = vec!["--help".to_string()];
    let preview = aifo_coder::build_docker_preview_only("aider", &args, "alpine:3.20", None);

    let needle_plain = "-e AIFO_CONFIG_ALLOW_EXT=json,ini";
    let needle_quoted = "-e 'AIFO_CONFIG_ALLOW_EXT=json,ini'";
    assert!(
        preview.contains(needle_plain) || preview.contains(needle_quoted),
        "missing AIFO_CONFIG_ALLOW_EXT passthrough in preview:\n{}",
        preview
    );
    assert!(
        preview.contains("-e AIFO_CONFIG_COPY_ALWAYS=1"),
        "missing AIFO_CONFIG_COPY_ALWAYS passthrough in preview:\n{}",
        preview
    );
    assert!(
        preview.contains("-e AIFO_CONFIG_MAX_SIZE=1024"),
        "missing AIFO_CONFIG_MAX_SIZE passthrough in preview:\n{}",
        preview
    );

    // Clean up overrides
    std::env::remove_var("AIFO_CONFIG_ALLOW_EXT");
    std::env::remove_var("AIFO_CONFIG_COPY_ALWAYS");
    std::env::remove_var("AIFO_CONFIG_MAX_SIZE");
}
