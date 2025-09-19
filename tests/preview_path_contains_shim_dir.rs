#[test]
fn test_build_docker_cmd_exports_path_with_shim_dir() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    let args = vec!["--help".to_string()];
    let (_cmd, preview) = aifo_coder::build_docker_cmd("aider", &args, "alpine:3.20", None)
        .expect("build_docker_cmd");

    // PATH ordering is agent-specific; ensure shim dir is present somewhere in PATH.
    assert!(
        preview.contains("/opt/aifo/bin"),
        "exported PATH missing shim dir: {}",
        preview
    );
}
