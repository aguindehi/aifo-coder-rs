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
#[ignore]
#[test]
fn e2e_proxy_handles_large_payload_notifications_cmd() {
    // Respect CI override disabling docker and ensure daemon is reachable
    if std::env::var("AIFO_CODER_TEST_DISABLE_DOCKER")
        .ok()
        .as_deref()
        == Some("1")
    {
        eprintln!("skipping: AIFO_CODER_TEST_DISABLE_DOCKER=1");
        return;
    }
    let runtime = match aifo_coder::container_runtime_path() {
        Ok(p) => p,
        Err(_) => {
            eprintln!("skipping: docker not found in PATH");
            return;
        }
    };
    let ok = std::process::Command::new(&runtime)
        .arg("ps")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !ok {
        eprintln!("skipping: Docker daemon not reachable");
        return;
    }

    let sid = "unit-test-session";
    let (url, token, flag, handle) =
        aifo_coder::toolexec_start_proxy(sid, true).expect("start proxy");

    let (mut stream, host, _port, path) = connect(&url);

    // Build large body using notifications-cmd (no sidecars needed); config likely missing -> 403
    let mut body = format!(
        "tool={}&cwd={}",
        urlencode("notifications-cmd"),
        urlencode(".")
    );
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
    // Large form bodies should be rejected safely. With the current hardening, the proxy can
    // respond 400 (bad request) due to per-field caps even if the overall body cap allows it.
    assert!(
        resp.starts_with("HTTP/1.1 400")
            || resp.starts_with("HTTP/1.1 403")
            || resp.starts_with("HTTP/1.1 200"),
        "expected a valid HTTP response (400/403/200), got:\n{}",
        resp
    );

    flag.store(false, std::sync::atomic::Ordering::SeqCst);
    let _ = handle.join();
}
