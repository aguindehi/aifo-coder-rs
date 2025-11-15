use aifo_coder as aifo;

#[test]
fn int_build_docker_cmd_preview_contains() {
    // Skip if docker isn't available on this host
    if aifo::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }
    let args = vec!["--version".to_string()];
    let (_cmd, preview) = aifo::build_docker_cmd("aider", &args, "alpine:3.20", None)
        .expect("build_docker_cmd failed");
    assert!(
        preview.starts_with("docker run"),
        "preview didn't start with docker run: {preview}"
    );
    assert!(
        preview.contains("alpine:3.20"),
        "preview missing image name: {preview}"
    );
    assert!(
        preview.contains("aider"),
        "preview missing agent invocation: {preview}"
    );
    assert!(
        preview.contains("/bin/sh"),
        "preview missing shell wrapper: {preview}"
    );
}
