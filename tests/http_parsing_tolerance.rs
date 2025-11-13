mod support;

#[test]
fn test_http_parsing_tolerates_lf_only_terminator() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }
    // Start proxy
    let sid = format!("lfparse-{}", std::process::id());
    let (url, _token, flag, handle) =
        aifo_coder::toolexec_start_proxy(&sid, false).expect("failed to start proxy");

    fn extract_port(u: &str) -> u16 {
        support::port_from_http_url(u)
    }
    let port = extract_port(&url);

    // Send LF-only header termination; expect parser to accept and route to /exec -> 405
    {
        let req = "GET /exec HTTP/1.1\nHost: localhost\n\n";
        let txt = support::http_send_raw(port, req);
        assert!(
            txt.contains("405 Method Not Allowed"),
            "expected 405 for GET /exec with LF-only headers: {}",
            txt
        );
    }

    // Cleanup
    flag.store(false, std::sync::atomic::Ordering::SeqCst);
    let _ = handle.join();
    aifo_coder::toolchain_cleanup_session(&sid, false);
}
