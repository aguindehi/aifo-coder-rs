use std::io::{Read, Write};

#[test]
fn test_http_parsing_tolerates_lf_only_terminator() {
    // Start proxy
    let sid = format!("lfparse-{}", std::process::id());
    let (url, _token, flag, handle) =
        aifo_coder::toolexec_start_proxy(&sid, false).expect("failed to start proxy");

    fn extract_port(u: &str) -> u16 {
        let after_scheme = u.split("://").nth(1).unwrap_or(u);
        let host_port = after_scheme.split('/').next().unwrap_or(after_scheme);
        host_port
            .rsplit(':')
            .next()
            .unwrap_or("0")
            .parse::<u16>()
            .unwrap_or(0)
    }
    let port = extract_port(&url);

    // Send LF-only header termination; expect parser to accept and route to /exec -> 405
    {
        use std::net::TcpStream;
        let mut s = TcpStream::connect(("127.0.0.1", port)).expect("connect");
        let req = "GET /exec HTTP/1.1\nHost: localhost\n\n";
        s.write_all(req.as_bytes()).expect("write");
        let mut resp = Vec::new();
        let _ = s.read_to_end(&mut resp);
        let txt = String::from_utf8_lossy(&resp);
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
