use std::io::{Read, Write};
use std::net::TcpStream;
mod support;
use support::urlencode;

fn connect(url: &str) -> (TcpStream, String, u16, String) {
    let u = url.strip_prefix("http://").expect("only http supported");
    let mut parts = u.splitn(2, '/');
    let hostport = parts.next().unwrap_or_default();
    let path_rest = parts.next().unwrap_or_default();
    let mut hp = hostport.rsplitn(2, ':');
    let port_str = hp.next().unwrap_or("80");
    let host = hp.next().unwrap_or(hostport);
    let port: u16 = port_str.parse().expect("port parse");
    let path = format!("/{}", path_rest);
    let connect_host = if host == "host.docker.internal" {
        "127.0.0.1".to_string()
    } else {
        host.to_string()
    };
    let stream = TcpStream::connect((connect_host.as_str(), port)).expect("connect failed");
    (stream, host.to_string(), port, path)
}

#[test]
#[ignore]
fn test_proxy_handles_large_payload_notifications_cmd() {
    let sid = "unit-test-session";
    let (url, token, flag, handle) =
        aifo_coder::toolexec_start_proxy(sid, true).expect("start proxy");

    let (mut stream, host, _port, path) = connect(&url);

    // Build large body using notifications-cmd (no sidecars needed); config likely missing -> 403
    let mut body = format!("tool={}&cwd={}", urlencode("notifications-cmd"), urlencode("."));
    for i in 0..5000 {
        body.push('&');
        body.push_str(&format!("arg={}", urlencode(&format!("x{i:04}"))));
    }

    let req = format!(
        "POST {} HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {}\r\nX-Aifo-Proto: 1\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        path, host, token, body.len(), body
    );
    stream.write_all(req.as_bytes()).expect("write");
    let mut resp = String::new();
    let mut tmp = [0u8; 4096];
    loop {
        match stream.read(&mut tmp) {
            Ok(0) => break,
            Ok(n) => resp.push_str(&String::from_utf8_lossy(&tmp[..n])),
            Err(_) => break,
        }
    }
    assert!(
        resp.starts_with("HTTP/1.1 403") || resp.starts_with("HTTP/1.1 200"),
        "expected a valid HTTP response (403 or 200), got:\n{}",
        resp
    );

    flag.store(false, std::sync::atomic::Ordering::SeqCst);
    let _ = handle.join();
}
