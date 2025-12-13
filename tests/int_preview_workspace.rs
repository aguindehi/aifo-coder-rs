#[test]
fn int_test_build_docker_cmd_includes_workspace_mount_and_workdir() {
    let pwd = std::env::current_dir().unwrap();
    let args = vec!["--help".to_string()];
    let preview = aifo_coder::build_docker_preview_only("crush", &args, "alpine:3.20", None);

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
