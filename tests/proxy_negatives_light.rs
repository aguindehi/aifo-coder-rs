use std::io::{Read, Write};

#[test]
fn test_proxy_unauthorized_without_sidecars_and_missing_tool_body() {
    // Need docker present to start proxy, no sidecars required
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found");
        return;
    }

    let sid = format!("lightneg-{}", std::process::id());
    let (url, token, flag, handle) =
        aifo_coder::toolexec_start_proxy(&sid, false).expect("start proxy");

    fn extract_port(u: &str) -> u16 {
        let after = u.split("://").nth(1).unwrap_or(u);
        let hp = after.split('/').next().unwrap_or(after);
        hp.rsplit(':').next().unwrap_or("0").parse().unwrap_or(0)
    }
    let port = extract_port(&url);

    // 401 Unauthorized (no Authorization header)
    {
        use std::net::TcpStream;
        let mut s = TcpStream::connect(("127.0.0.1", port)).expect("connect");
        let body = "tool=cargo&cwd=.";
        let req = format!(
            "POST /exec HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n\r\n{}",
            body.len(), body
        );
        s.write_all(req.as_bytes()).expect("write");
        let mut resp = Vec::new();
        s.read_to_end(&mut resp).ok();
        let txt = String::from_utf8_lossy(&resp);
        assert!(
            txt.contains("401 Unauthorized"),
            "expected 401, got:\n{}",
            txt
        );
    }

    // 400 Bad Request (missing tool param in body)
    {
        use std::net::TcpStream;
        let mut s = TcpStream::connect(("127.0.0.1", port)).expect("connect2");
        let body = "cwd=.";
        let req = format!(
            "POST /exec HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {}\r\nX-Aifo-Proto: 1\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n\r\n{}",
            token, body.len(), body
        );
        s.write_all(req.as_bytes()).expect("write2");
        let mut resp = Vec::new();
        s.read_to_end(&mut resp).ok();
        let txt = String::from_utf8_lossy(&resp);
        assert!(
            txt.contains("400 Bad Request"),
            "expected 400, got:\n{}",
            txt
        );
    }

    // Cleanup
    flag.store(false, std::sync::atomic::Ordering::SeqCst);
    let _ = handle.join();
    aifo_coder::toolchain_cleanup_session(&sid, false);
}
