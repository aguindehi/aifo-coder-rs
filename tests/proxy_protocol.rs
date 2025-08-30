use std::io::{Read, Write};

#[test]
fn test_proxy_missing_or_wrong_proto_header() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // Start proxy without launching sidecars; protocol guard is independent of sidecars
    let sid = format!("proto-{}", std::process::id());
    let (url, token, flag, handle) = aifo_coder::toolexec_start_proxy(&sid, false)
        .expect("failed to start proxy");

    fn extract_port(u: &str) -> u16 {
        let after_scheme = u.split("://").nth(1).unwrap_or(u);
        let host_port = after_scheme.split('/').next().unwrap_or(after_scheme);
        host_port.rsplit(':').next().unwrap_or("0").parse::<u16>().unwrap_or(0)
    }
    let port = extract_port(&url);

    // 1) Missing X-Aifo-Proto header -> expect 426
    {
        use std::net::TcpStream;
        let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect failed");
        let body = "tool=cargo&cwd=.";
        let req = format!(
            "POST /exec HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {}\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n\r\n{}",
            token, body.len(), body
        );
        stream.write_all(req.as_bytes()).expect("write failed");
        let mut resp = Vec::new();
        stream.read_to_end(&mut resp).ok();
        let text = String::from_utf8_lossy(&resp).to_string();
        assert!(text.contains("426 Upgrade Required"), "expected 426, got:\n{}", text);
        assert!(text.contains("X-Exit-Code: 86"), "expected exit 86 header, got:\n{}", text);
    }

    // 2) Wrong protocol version -> expect 426
    {
        use std::net::TcpStream;
        let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect2 failed");
        let body = "tool=cargo&cwd=.";
        let req = format!(
            "POST /exec HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {}\r\nX-Aifo-Proto: 99\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n\r\n{}",
            token, body.len(), body
        );
        stream.write_all(req.as_bytes()).expect("write2 failed");
        let mut resp = Vec::new();
        stream.read_to_end(&mut resp).ok();
        let text = String::from_utf8_lossy(&resp).to_string();
        assert!(text.contains("426 Upgrade Required"), "expected 426, got:\n{}", text);
        assert!(text.contains("X-Exit-Code: 86"), "expected exit 86 header, got:\n{}", text);
    }

    // Cleanup
    flag.store(false, std::sync::atomic::Ordering::SeqCst);
    let _ = handle.join();
    aifo_coder::toolchain_cleanup_session(&sid, false);
}
