mod support;

#[test]
fn test_http_excessive_headers_yields_431() {
    // Skip if docker isn't available on this host (proxy requires docker CLI path for runtime)
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // Start proxy (no sidecar needed to test header parsing errors)
    let sid = "ut-http-headers-431";
    let (url, _token, flag, handle) =
        aifo_coder::toolexec_start_proxy(sid, true).expect("start proxy");

    // Extract port from http URL
    fn port_from_url(url: &str) -> u16 {
        let after = url.split("://").nth(1).unwrap_or(url);
        let host_port = after.split('/').next().unwrap_or(after);
        host_port
            .rsplit(':')
            .next()
            .unwrap_or("0")
            .parse()
            .unwrap_or(0)
    }
    let port = port_from_url(&url);

    // Build an HTTP request with excessive number of headers (>1024)
    use std::io::{Read, Write};
    use std::net::TcpStream;

    let mut req = String::new();
    req.push_str("POST /exec HTTP/1.1\r\nHost: localhost\r\n");
    for i in 0..1100 {
        req.push_str(&format!("X-Excess-{}: a\r\n", i));
    }
    req.push_str("Content-Length: 0\r\nConnection: close\r\n\r\n");

    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    stream.write_all(req.as_bytes()).expect("write");

    let mut resp = String::new();
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).ok();
    resp = String::from_utf8_lossy(&buf).to_string();

    assert!(
        resp.contains("431 Request Header Fields Too Large"),
        "expected 431, got:\n{}",
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
fn test_http_content_length_mismatch_yields_400() {
    // Skip if docker isn't available on this host (proxy requires docker CLI path for runtime)
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // Start proxy (no sidecar needed)
    let sid = "ut-http-cl-400";
    let (url, _token, flag, handle) =
        aifo_coder::toolexec_start_proxy(sid, true).expect("start proxy");

    fn port_from_url(url: &str) -> u16 {
        let after = url.split("://").nth(1).unwrap_or(url);
        let host_port = after.split('/').next().unwrap_or(after);
        host_port
            .rsplit(':')
            .next()
            .unwrap_or("0")
            .parse()
            .unwrap_or(0)
    }
    let port = port_from_url(&url);

    // Declare small Content-Length but send a larger body to trigger mismatch
    use std::io::{Read, Write};
    use std::net::TcpStream;

    let body = b"tool=cargo&cwd=.&arg=--help&extra=bytes";
    let req = format!(
        "POST /exec HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: 3\r\nConnection: close\r\n\r\n{}",
        String::from_utf8_lossy(body)
    );

    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    stream.write_all(req.as_bytes()).expect("write");

    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).ok();
    let resp = String::from_utf8_lossy(&buf).to_string();

    assert!(
        resp.contains("400 Bad Request"),
        "expected 400, got:\n{}",
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
