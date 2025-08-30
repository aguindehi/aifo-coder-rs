#[ignore]
#[test]
fn test_proxy_unauthorized_and_unknown_tool() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // Start rust sidecar (enough for this negative test) and the proxy
    let kinds = vec!["rust".to_string()];
    let overrides: Vec<(String, String)> = Vec::new();
    let sid = aifo_coder::toolchain_start_session(&kinds, &overrides, false, true)
        .expect("failed to start sidecar session");
    let (url, token, flag, handle) = aifo_coder::toolexec_start_proxy(&sid, true).expect("failed to start proxy");

    // Helper to extract host:port from url "http://host.docker.internal:PORT/exec"
    fn extract_port(u: &str) -> u16 {
        let after_scheme = u.split("://").nth(1).unwrap_or(u);
        let host_port = after_scheme.split('/').next().unwrap_or(after_scheme);
        let port_str = host_port.rsplit(':').next().unwrap_or("0");
        port_str.parse::<u16>().unwrap_or(0)
    }
    let port = extract_port(&url);

    // No Authorization header -> expect 401
    use std::io::{Read, Write};
    use std::net::TcpStream;
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect failed");
    let body = "tool=cargo&cwd=.";
    let req = format!(
        "POST /exec HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(req.as_bytes()).expect("write failed");
    let mut resp = Vec::new();
    stream.read_to_end(&mut resp).ok();
    let text = String::from_utf8_lossy(&resp).to_string();
    assert!(text.contains("401 Unauthorized"), "expected 401, got:\n{}", text);

    // Unknown tool name with valid token -> expect 403
    let mut stream2 = TcpStream::connect(("127.0.0.1", port)).expect("connect2 failed");
    let body2 = "tool=h4x0r&cwd=.";
    let req2 = format!(
        "POST /exec HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {}\r\nX-Aifo-Proto: 1\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n\r\n{}",
        token,
        body2.len(),
        body2
    );
    stream2.write_all(req2.as_bytes()).expect("write2 failed");
    let mut resp2 = Vec::new();
    stream2.read_to_end(&mut resp2).ok();
    let text2 = String::from_utf8_lossy(&resp2).to_string();
    assert!(text2.contains("403 Forbidden"), "expected 403, got:\n{}", text2);

    // Cleanup
    flag.store(false, std::sync::atomic::Ordering::SeqCst);
    let _ = handle.join();
    aifo_coder::toolchain_cleanup_session(&sid, true);
}
