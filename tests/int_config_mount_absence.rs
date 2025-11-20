#[test]
fn int_config_mount_absent_when_no_host_dir() {
    // Use preview-only to avoid requiring docker runtime for this test
    // Prepare a temporary HOME without config directories
    let td = tempfile::tempdir().expect("tmpdir");
    std::env::set_var("HOME", td.path());
    // Ensure no host config override and no default dirs
    std::env::remove_var("AIFO_CONFIG_HOST_DIR");
    // Build preview and assert config-host mount is absent
    let args = vec!["--help".to_string()];
    let preview = aifo_coder::build_docker_preview_only("aider", &args, "alpine:3.20", None);
    assert!(
        !preview.contains(":/home/coder/.aifo-config-host:ro"),
        "config-host mount should be absent when no host dir is present; preview:\n{}",
        preview
    );
    // Clean up HOME override
    std::env::remove_var("HOME");
}
