use std::io::{Read, Write};

#[test]
fn test_http_endpoint_routing() {
    // Start proxy
    let sid = format!("rt-{}", std::process::id());
    let (url, token, flag, handle) =
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

    // Deprecated alias: /notifications -> 404
    {
        use std::net::TcpStream;
        let mut s = TcpStream::connect(("127.0.0.1", port)).expect("connect notifications");
        let body = "";
        let req = format!(
            "POST /notifications HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {}\r\nX-Aifo-Proto: 1\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n\r\n{}",
            token, body.len(), body
        );
        s.write_all(req.as_bytes()).expect("write");
        let mut resp = Vec::new();
        let _ = s.read_to_end(&mut resp);
        let txt = String::from_utf8_lossy(&resp);
        assert!(
            txt.contains("404 Not Found"),
            "expected 404 for /notifications: {}",
            txt
        );
    }

    // Deprecated alias: /notifications-cmd -> 404
    {
        use std::net::TcpStream;
        let mut s = TcpStream::connect(("127.0.0.1", port)).expect("connect notifications-cmd");
        let body = "";
        let req = format!(
            "POST /notifications-cmd HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {}\r\nX-Aifo-Proto: 1\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n\r\n{}",
            token, body.len(), body
        );
        s.write_all(req.as_bytes()).expect("write");
        let mut resp = Vec::new();
        let _ = s.read_to_end(&mut resp);
        let txt = String::from_utf8_lossy(&resp);
        assert!(
            txt.contains("404 Not Found"),
            "expected 404 for /notifications-cmd: {}",
            txt
        );
    }

    // /exec is recognized: GET should yield 405 (not 404)
    {
        use std::net::TcpStream;
        let mut s = TcpStream::connect(("127.0.0.1", port)).expect("connect exec");
        let req = "GET /exec HTTP/1.1\r\nHost: localhost\r\n\r\n";
        s.write_all(req.as_bytes()).expect("write");
        let mut resp = Vec::new();
        let _ = s.read_to_end(&mut resp);
        let txt = String::from_utf8_lossy(&resp);
        assert!(
            txt.contains("405 Method Not Allowed"),
            "expected 405 for GET /exec: {}",
            txt
        );
    }

    // /notify is recognized: POST should not be 404 (auth/proto/policy may still apply)
    {
        use std::net::TcpStream;
        let mut s = TcpStream::connect(("127.0.0.1", port)).expect("connect notify");
        let body = "arg=--x";
        let req = format!(
            "POST /notify HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {}\r\nX-Aifo-Proto: 1\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n\r\n{}",
            token, body.len(), body
        );
        s.write_all(req.as_bytes()).expect("write");
        let mut resp = Vec::new();
        let _ = s.read_to_end(&mut resp);
        let txt = String::from_utf8_lossy(&resp);
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
