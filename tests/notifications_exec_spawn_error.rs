#[test]
fn test_notifications_exec_spawn_error_500() {
    // Skip if docker isn't available on this host (proxy requires docker CLI path for runtime)
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // Enable noauth notifications mode
    std::env::set_var("AIFO_NOTIFICATIONS_NOAUTH", "1");
    let sid = "ut-notify-spawnerr";
    let (url, _token, flag, handle) =
        aifo_coder::toolexec_start_proxy(sid, true).expect("start proxy");

    // Connect and send a /notify request with an absolute non-existent 'say' path
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

    use std::io::{Read, Write};
    use std::net::TcpStream;

    let body = "cmd=/no/such/say&arg=hello";
    let req = format!(
        "POST /notify HTTP/1.1\r\n\
         Host: localhost\r\n\
         X-Aifo-Proto: 2\r\n\
         Content-Type: application/x-www-form-urlencoded\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\r\n{}",
        body.len(),
        body
    );

    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    stream.write_all(req.as_bytes()).expect("write");

    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).ok();
    let resp = String::from_utf8_lossy(&buf).to_string();

    assert!(
        resp.contains("500 Internal Server Error"),
        "expected 500, got:\n{}",
        resp
    );
    assert!(
        resp.to_ascii_lowercase().contains("x-exit-code: 86"),
        "expected X-Exit-Code: 86 header, got:\n{}",
        resp
    );
    assert!(
        resp.to_ascii_lowercase().contains("failed"),
        "expected error message to mention spawn failure, got:\n{}",
        resp
    );

    // Cleanup
    flag.store(false, std::sync::atomic::Ordering::SeqCst);
    let _ = handle.join();
    std::env::remove_var("AIFO_NOTIFICATIONS_NOAUTH");
}
