#![cfg_attr(not(test), allow(dead_code))]

use std::net::TcpStream;
use std::time::Duration;
mod support;
#[ignore]
#[test]
#[ignore]
fn accept_phase4_disconnect_triggers_proxy_log() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // Start rust sidecar session
    let kinds = vec!["rust".to_string()];
    let overrides: Vec<(String, String)> = Vec::new();
    let sid = aifo_coder::toolchain_start_session(&kinds, &overrides, false, true)
        .expect("toolchain_start_session");

    let log_path = format!(
        "/tmp/aifo-coder-accept-disconnect-{}.log",
        std::process::id()
    );
    // Start proxy (TCP) with verbose and tee logs
    std::env::set_var("AIFO_TOOLCHAIN_VERBOSE", "1");
    std::env::set_var("AIFO_TEST_LOG_PATH", &log_path);
    let (url, _token, running, handle) =
        aifo_coder::toolexec_start_proxy(&sid, true).expect("start proxy (tcp)");

    // Build request headers for a small exec, then immediately close the socket to simulate disconnect
    let rest = url.trim_start_matches("http://").to_string();
    let path_idx = rest.find('/').unwrap_or(rest.len());
    let (host_port, path) = rest.split_at(path_idx);
    let (host, port) = if let Some((h, p)) = host_port.split_once(':') {
        let pn = p.parse::<u16>().unwrap_or(80);
        (h.to_string(), pn)
    } else {
        (host_port.to_string(), 80u16)
    };
    let req_path = if path.is_empty() { "/exec" } else { path };

    let mut stream = TcpStream::connect((host.as_str(), port)).expect("connect tcp");
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(10)));

    let body_pairs = [
        ("tool".to_string(), "cargo".to_string()),
        ("cwd".to_string(), "/workspace".to_string()),
        ("arg".to_string(), "--version".to_string()),
    ];
    let mut body = String::new();
    for (i, (k, v)) in body_pairs.iter().enumerate() {
        if i > 0 {
            body.push('&');
        }
        body.push_str(&support::urlencode(k));
        body.push('=');
        body.push_str(&support::urlencode(v));
    }

    let req_line = format!("POST {} HTTP/1.1\r\n", req_path);
    let headers = format!(
        concat!(
            "Host: {host}\r\n",
            "Authorization: Bearer {tok}\r\n",
            "X-Aifo-Proto: 2\r\n",
            "TE: trailers\r\n",
            "Content-Type: application/x-www-form-urlencoded\r\n",
            "Transfer-Encoding: chunked\r\n",
            "Connection: close\r\n",
            "\r\n"
        ),
        host = host,
        tok = _token
    );

    // Send request and body, then close early without reading response
    use std::io::Write as _;
    stream.write_all(req_line.as_bytes()).unwrap();
    stream.write_all(headers.as_bytes()).unwrap();
    write!(stream, "{:X}\r\n", body.len()).unwrap();
    stream.write_all(body.as_bytes()).unwrap();
    stream.write_all(b"\r\n").unwrap();
    stream.flush().unwrap();

    // Immediately close the socket to force a write error on the proxy side
    let _ = stream.shutdown(std::net::Shutdown::Both);

    // Wait up to 3s for the proxy to log 'disconnect' before stopping it
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    loop {
        let logs_now = std::fs::read_to_string(&log_path).unwrap_or_default();
        if logs_now.contains("aifo-coder: disconnect") {
            break;
        }
        if std::time::Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    running.store(false, std::sync::atomic::Ordering::SeqCst);
    let _ = handle.join();

    let logs = std::fs::read_to_string(&log_path).unwrap_or_default();
    let _ = std::fs::remove_file(&log_path);

    aifo_coder::toolchain_cleanup_session(&sid, true);

    if logs.trim().is_empty() {
        eprintln!("skipping: unable to capture proxy logs on this platform/test harness");
        return;
    }

    assert!(
        logs.contains("aifo-coder: disconnect"),
        "expected proxy to log 'disconnect' on client close; logs:\n{}",
        logs
    );
}
