#[test]
fn test_build_docker_cmd_includes_workspace_mount_and_workdir() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    let pwd = std::env::current_dir().unwrap();
    let args = vec!["--help".to_string()];
    let (_cmd, preview) = aifo_coder::build_docker_cmd("crush", &args, "alpine:3.20", None)
        .expect("build_docker_cmd failed");

    let expected_mount = format!("-v {}:/workspace", pwd.display());
    assert!(
        preview.contains(&expected_mount),
        "preview missing workspace mount '{}': {preview}",
        expected_mount
    );

    assert!(
        preview.contains("-w /workspace"),
        "preview missing workdir flag: {preview}"
    );
}
