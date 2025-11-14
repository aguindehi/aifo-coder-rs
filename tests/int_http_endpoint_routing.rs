mod support;

#[test]
fn test_http_endpoint_routing() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }
    // Start proxy
    let sid = format!("rt-{}", std::process::id());
    let (url, token, flag, handle) =
        aifo_coder::toolexec_start_proxy(&sid, false).expect("failed to start proxy");

    fn extract_port(u: &str) -> u16 {
        support::port_from_http_url(u)
    }
    let port = extract_port(&url);

    // Note: legacy /notifications endpoint is no longer supported and not tested here.

    // Note: legacy /notifications-cmd endpoint is no longer supported and not tested here.

    // /exec is recognized: GET should yield 405 (not 404)
    {
        let txt = support::http_send_raw(port, "GET /exec HTTP/1.1\r\nHost: localhost\r\n\r\n");
        assert!(
            txt.contains("405 Method Not Allowed"),
            "expected 405 for GET /exec: {}",
            txt
        );
    }

    // /notify is recognized: POST should not be 404 (auth/proto/policy may still apply)
    {
        let body = "arg=--x";
        let req = format!(
            "POST /notify HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {}\r\nX-Aifo-Proto: 1\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n\r\n{}",
            token, body.len(), body
        );
        let txt = support::http_send_raw(port, &req);
        assert!(
            !txt.contains("404 Not Found"),
            "did not expect 404 for /notify: {}",
            txt
        );
    }

    // Cleanup proxy/session
    flag.store(false, std::sync::atomic::Ordering::SeqCst);
    let _ = handle.join();
    aifo_coder::toolchain_cleanup_session(&sid, false);
}
