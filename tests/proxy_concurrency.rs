use std::io::{Read, Write};
use std::net::TcpStream;
use std::thread;

fn parse_http_url(url: &str) -> (String, u16, String) {
    let u = url.strip_prefix("http://").expect("only http supported");
    let mut parts = u.splitn(2, '/');
    let hostport = parts.next().unwrap_or_default();
    let path_rest = parts.next().unwrap_or_default();
    let mut hp = hostport.rsplitn(2, ':');
    let port_str = hp.next().unwrap_or("80");
    let host = hp.next().unwrap_or(hostport);
    let port: u16 = port_str.parse().expect("port parse");
    let path = format!("/{}", path_rest);
    (host.to_string(), port, path)
}

fn send_raw(host: &str, port: u16, req: &str) -> String {
    let connect_host = if host == "host.docker.internal" { "127.0.0.1" } else { host };
    let mut stream = TcpStream::connect((connect_host, port)).expect("connect failed");
    stream.write_all(req.as_bytes()).expect("write");
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
fn test_proxy_concurrency_mixed_requests() {
    let sid = "unit-test-session";
    let (url, token, flag, handle) = aifo_coder::toolexec_start_proxy(sid, true).expect("start proxy");
    let (host, port, path) = parse_http_url(&url);

    // Build requests
    let no_auth_req = format!("POST {} HTTP/1.1\r\nHost: {}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n", path, host);
    let bad_proto_req = format!(
        "POST {} HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {}\r\nX-Aifo-Proto: 0\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        path, host, token
    );

    let mut threads = Vec::new();
    for i in 0..20 {
        let host_c = host.clone();
        let no_auth = no_auth_req.clone();
        let bad_proto = bad_proto_req.clone();
        threads.push(thread::spawn(move || {
            if i % 2 == 0 {
                send_raw(&host_c, port, &no_auth)
            } else {
                send_raw(&host_c, port, &bad_proto)
            }
        }));
    }

    for (i, t) in threads.into_iter().enumerate() {
        let resp = t.join().expect("thread join");
        if i % 2 == 0 {
            assert!(resp.starts_with("HTTP/1.1 401"), "expected 401 for missing auth, got:\n{}", resp);
        } else {
            assert!(resp.starts_with("HTTP/1.1 426"), "expected 426 for bad proto, got:\n{}", resp);
        }
    }

    flag.store(false, std::sync::atomic::Ordering::SeqCst);
    let _ = handle.join();
}
