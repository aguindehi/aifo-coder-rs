#[test]
fn int_test_build_docker_cmd_includes_shim_dir_mount() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    let tmp = tempfile::tempdir().expect("tmpdir");
    let shim_dir = std::fs::canonicalize(tmp.path())
        .unwrap_or_else(|_| tmp.path().to_path_buf())
        .to_string_lossy()
        .to_string();

    // Save and set env
    let old = std::env::var("AIFO_SHIM_DIR").ok();
    std::env::set_var("AIFO_SHIM_DIR", &shim_dir);

    let args = vec!["--version".to_string()];
    let (_cmd, preview) = aifo_coder::build_docker_cmd("codex", &args, "alpine:3.20", None)
        .expect("build_docker_cmd failed");

    let expected = format!("-v {}:/opt/aifo/bin:ro", shim_dir);
    assert!(
        preview.contains(&expected),
        "preview missing shim dir mount '{}': {preview}",
        expected
    );

    // Restore env
    if let Some(v) = old {
        std::env::set_var("AIFO_SHIM_DIR", v);
    } else {
        std::env::remove_var("AIFO_SHIM_DIR");
    }
}
