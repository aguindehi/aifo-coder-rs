#[test]
fn test_preview_includes_gnupg_host_mount_and_aider_configs() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    let args = vec!["--help".to_string()];
    let (_cmd, preview) =
        aifo_coder::build_docker_cmd("aider", &args, "alpine:3.20", None).expect("build_docker_cmd failed");

    // GnuPG host mount (read-only) path is stable on container side
    assert!(
        preview.contains("/home/coder/.gnupg-host:ro"),
        "missing gnupg host mount: {preview}"
    );

    // Aider root-level config files mounts (container side destinations are stable)
    for fname in [".aider.conf.yml", ".aider.model.metadata.json", ".aider.model.settings.yml"] {
        let needle = format!("/home/coder/{}", fname);
        assert!(
            preview.contains(&needle),
            "missing aider config mount for {}: {}",
            fname,
            preview
        );
    }
}
