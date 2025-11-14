use std::io::{Read, Write};
use std::net::TcpStream;

fn connect_and_roundtrip(url: &str, req: &str) -> String {
    // Parse http://host:port/path
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
        "127.0.0.1"
    } else {
        host
    };
    let mut stream = TcpStream::connect((connect_host, port)).expect("connect failed");

    let rendered = req.replace("{PATH}", &path).replace("{HOST}", host);
    stream.write_all(rendered.as_bytes()).expect("write");
    let mut buf = String::new();
    let mut tmp = [0u8; 1024];
    loop {
        match stream.read(&mut tmp) {
            Ok(0) => break,
            Ok(n) => buf.push_str(&String::from_utf8_lossy(&tmp[..n])),
            Err(_) => break,
        }
    }
    buf
}

#[test]
fn int_proxy_missing_auth_is_401() {
    if aifo_coder::container_runtime_path().is_err() {
        // proxy can run without docker, but we test with or without; no skip
    }
    let sid = "unit-test-session";
    let (url, token, flag, handle) =
        aifo_coder::toolexec_start_proxy(sid, true).expect("start proxy");

    let req =
        "POST {PATH} HTTP/1.1\r\nhost: {HOST}\r\ncontent-length: 0\r\nconnection: close\r\n\r\n";
    let resp = connect_and_roundtrip(&url, req);
    assert!(
        resp.starts_with("HTTP/1.1 401"),
        "expected 401, got:\n{}",
        resp
    );

    flag.store(false, std::sync::atomic::Ordering::SeqCst);
    let _ = handle.join();
    let _ = token;
}

#[test]
fn int_proxy_header_case_and_bad_proto_yields_426() {
    let sid = "unit-test-session";
    let (url, token, flag, handle) =
        aifo_coder::toolexec_start_proxy(sid, true).expect("start proxy");

    // Lower-case header names; correct auth, wrong proto
    let req = format!(
        "POST {{PATH}} HTTP/1.1\r\nHost: {{HOST}}\r\nAuthorization: Bearer {}\r\nX-Aifo-Proto: 0\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        token
    );
    let resp = connect_and_roundtrip(&url, &req);
    assert!(
        resp.starts_with("HTTP/1.1 426"),
        "expected 426 Upgrade Required, got:\n{}",
        resp
    );
    assert!(
        resp.contains("X-Exit-Code: 86"),
        "expected X-Exit-Code: 86 header, got:\n{}",
        resp
    );

    flag.store(false, std::sync::atomic::Ordering::SeqCst);
    let _ = handle.join();
}
