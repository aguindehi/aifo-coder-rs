use std::io;
use aifo_coder as aifo;

#[test]
fn test_build_docker_cmd_preview_contains() {
    // Skip if docker isn't available on this host
    if aifo::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }
    let args = vec!["--version".to_string()];
    let (_cmd, preview) = aifo::build_docker_cmd("aider", &args, "alpine:3.20", None)
        .expect("build_docker_cmd failed");
    assert!(preview.starts_with("docker run"), "preview didn't start with docker run: {preview}");
    assert!(preview.contains("alpine:3.20"), "preview missing image name: {preview}");
    assert!(preview.contains("aider"), "preview missing agent invocation: {preview}");
    assert!(preview.contains("/bin/sh"), "preview missing shell wrapper: {preview}");
}

#[test]
fn test_acquire_lock_at_exclusive_and_release() {
    let mut p = std::env::temp_dir();
    p.push(format!("aifo-coder-lock-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    // First lock should succeed
    let f1 = aifo::acquire_lock_at(&p).expect("first acquire_lock_at failed");
    // Second lock on same path should fail
    let e = aifo::acquire_lock_at(&p).expect_err("second acquire_lock_at unexpectedly succeeded");
    assert_eq!(e.kind(), io::ErrorKind::Other);
    assert!(e.to_string().contains("already running"), "unexpected error message: {e}");
    drop(f1);
    // After releasing, should succeed again
    let _f2 = aifo::acquire_lock_at(&p).expect("acquire_lock_at after release failed");
    // cleanup
    let _ = std::fs::remove_file(&p);
}
