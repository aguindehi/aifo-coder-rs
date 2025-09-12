#[cfg(target_os = "linux")]
#[test]
fn test_unix_socket_url_includes_session_dir() {
    // Skip if docker isn't available on this host (proxy requires docker CLI path for runtime)
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // Enable unix socket transport
    std::env::set_var("AIFO_TOOLEEXEC_USE_UNIX", "1");

    let session = "unx12345";
    let (url, _token, running, handle) =
        aifo_coder::toolexec_start_proxy(session, /*verbose=*/ false).expect("start proxy");

    // Expect exact path: unix:///run/aifo/aifo-<sid>/toolexec.sock
    let expected = format!("unix:///run/aifo/aifo-{}/toolexec.sock", session);
    assert_eq!(
        url, expected,
        "unexpected unix socket url: got {} expected {}",
        url, expected
    );

    // Stop proxy and cleanup
    use std::sync::atomic::{AtomicBool, Ordering};
    let flag: &AtomicBool = &running;
    flag.store(false, Ordering::SeqCst);
    let _ = handle.join();

    // Remove socket dir left by proxy (best-effort)
    let dir = format!("/run/aifo/aifo-{}", session);
    let _ = std::fs::remove_file(format!("{}/toolexec.sock", dir));
    let _ = std::fs::remove_dir_all(&dir);

    // Unset env
    std::env::remove_var("AIFO_TOOLEEXEC_USE_UNIX");
}
