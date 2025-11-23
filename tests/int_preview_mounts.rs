#[test]
fn int_test_preview_includes_gnupg_host_mount_and_aifo_config_host_dir() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // Use a deterministic temp host config dir so preview includes the ro mount
    let td = tempfile::tempdir().expect("tmpdir");
    std::env::set_var("AIFO_CONFIG_HOST_DIR", td.path());

    // Optional: schema subdirs to mirror spec (not required for preview)
    let _ = std::fs::create_dir(td.path().join("global"));
    let _ = std::fs::create_dir(td.path().join("aider"));

    let args = vec!["--help".to_string()];
    let (_cmd, preview) = aifo_coder::build_docker_cmd("aider", &args, "alpine:3.20", None)
        .expect("build_docker_cmd failed");

    // GnuPG host mount (read-only) path is stable on container side
    assert!(
        preview.contains("/home/coder/.gnupg-host:ro"),
        "missing gnupg host mount: {preview}"
    );

    // New Phase 1: config host dir mount (read-only)
    let cfg_mount = format!("{}:/home/coder/.aifo-config-host:ro", td.path().display());
    assert!(
        preview.contains(&cfg_mount),
        "missing aifo config host dir mount: expected {}, got preview:\n{}",
        cfg_mount,
        preview
    );

    // Clean up override
    std::env::remove_var("AIFO_CONFIG_HOST_DIR");
}
