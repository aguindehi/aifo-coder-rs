#[cfg(unix)]
#[test]
fn int_test_build_docker_cmd_includes_user_flag_on_unix() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    let args = vec!["--help".to_string()];
    let (_cmd, preview) = aifo_coder::build_docker_cmd("aider", &args, "alpine:3.20", None)
        .expect("build_docker_cmd");

    let uid = nix::unistd::getuid().as_raw();
    let gid = nix::unistd::getgid().as_raw();
    let needle = format!("--user {}:{}", uid, gid);
    assert!(
        preview.contains(&needle),
        "expected {} in preview: {}",
        needle,
        preview
    );
}
