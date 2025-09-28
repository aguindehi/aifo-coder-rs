mod support;

#[test]
fn test_get_exec_yields_405() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // Start proxy without launching sidecars; method enforcement is independent of sidecars
    let sid = format!("ut-endpoint-405-{}", std::process::id());
    let (url, _token, flag, handle) =
        aifo_coder::toolexec_start_proxy(&sid, true).expect("start proxy");

    // Extract port and send a GET request to /exec (only POST is allowed)
    let port = support::port_from_http_url(&url);
    let resp = support::http_send_raw(port, "GET /exec HTTP/1.1\r\nHost: localhost\r\n\r\n");

    assert!(
        resp.contains("405 Method Not Allowed"),
        "expected 405, got:\n{}",
        resp
    );
    assert!(
        resp.to_ascii_lowercase().contains("x-exit-code: 86"),
        "expected X-Exit-Code: 86 header, got:\n{}",
        resp
    );

    // Cleanup proxy
    flag.store(false, std::sync::atomic::Ordering::SeqCst);
    let _ = handle.join();
}

#[test]
fn test_unknown_path_yields_404() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    let sid = format!("ut-endpoint-404-{}", std::process::id());
    let (url, _token, flag, handle) =
        aifo_coder::toolexec_start_proxy(&sid, true).expect("start proxy");

    let port = support::port_from_http_url(&url);
    let resp = support::http_send_raw(port, "GET /nope HTTP/1.1\r\nHost: localhost\r\n\r\n");

    assert!(
        resp.contains("404 Not Found"),
        "expected 404, got:\n{}",
        resp
    );
    assert!(
        resp.to_ascii_lowercase().contains("x-exit-code: 86"),
        "expected X-Exit-Code: 86 header, got:\n{}",
        resp
    );

    flag.store(false, std::sync::atomic::Ordering::SeqCst);
    let _ = handle.join();
}
