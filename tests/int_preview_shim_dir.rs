#[test]
fn int_test_build_docker_cmd_uses_embedded_shims_and_shim_first_path() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // Host override shim dir support was removed (v2+ embedded shims are built into the image).
    // Assert the new invariant: we do NOT mount a host shim dir, and we do set a shim-first PATH.
    let old = std::env::var("AIFO_SHIM_DIR").ok();
    std::env::set_var("AIFO_SHIM_DIR", "/tmp/should-not-be-used");

    let args = vec!["--version".to_string()];
    let (_cmd, preview) = aifo_coder::build_docker_cmd("codex", &args, "alpine:3.20", None)
        .expect("build_docker_cmd failed");

    assert!(
        !preview.contains(":/opt/aifo/bin:ro"),
        "preview unexpectedly contains a host shim dir mount: {preview}"
    );

    let expected_path_export = r#"export PATH="/opt/aifo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:$PATH""#;
    assert!(
        preview.contains(expected_path_export),
        "preview missing shim-first PATH export: {preview}"
    );

    // Restore env
    if let Some(v) = old {
        std::env::set_var("AIFO_SHIM_DIR", v);
    } else {
        std::env::remove_var("AIFO_SHIM_DIR");
    }
}
