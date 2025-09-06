#[test]
fn test_build_docker_cmd_includes_unix_socket_mount_when_set() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    let tmp = tempfile::tempdir().expect("tmpdir");
    let dir = tmp.path().to_string_lossy().to_string();

    // Save and set env
    let old = std::env::var("AIFO_TOOLEEXEC_UNIX_DIR").ok();
    std::env::set_var("AIFO_TOOLEEXEC_UNIX_DIR", &dir);

    let args = vec!["--help".to_string()];
    let (_cmd, preview) = aifo_coder::build_docker_cmd("aider", &args, "alpine:3.20", None)
        .expect("build_docker_cmd failed");

    let expected = format!("-v {}:/run/aifo", dir);
    assert!(
        preview.contains(&expected),
        "preview missing unix socket mount '{}': {preview}",
        expected
    );

    // Restore env
    if let Some(v) = old {
        std::env::set_var("AIFO_TOOLEEXEC_UNIX_DIR", v);
    } else {
        std::env::remove_var("AIFO_TOOLEEXEC_UNIX_DIR");
    }
}
