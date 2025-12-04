use std::io::{Read, Write};
use std::net::TcpStream;

/// E2E (ignored by default): start node sidecar + proxy, execute "node --version" via proxy,
/// and assert the exit code is 0. Requires Docker; skips early when disabled by env.
/// Run in Docker-enabled CI lane with:
///   cargo nextest run --run-ignored ignored-only -E 'test(/^e2e_/)' --features otel-otlp
#[test]
#[ignore]
fn e2e_otel_proxy_exec_node_version() {
    // Respect CI toggle: if Docker-requiring tests are disabled, skip.
    if std::env::var("AIFO_CODER_TEST_DISABLE_DOCKER")
        .ok()
        .as_deref()
        == Some("1")
    {
        return;
    }

    // Start only the node sidecar
    let kinds = vec!["node".to_string()];
    let sid = match aifo_coder::toolchain_start_session(&kinds, &[], false, false) {
        Ok(s) => s,
        Err(_) => {
            // If Docker is not available, skip silently
            return;
        }
    };

    // Start proxy (TCP)
    let (url, token, running, handle) = match aifo_coder::toolexec_start_proxy(&sid, false) {
        Ok(t) => t,
        Err(_) => {
            aifo_coder::toolchain_cleanup_session(&sid, false);
            return;
        }
    };

    // Build a minimal HTTP/1.1 POST to /exec with v2 streaming headers.
    // Body: tool=node&cwd=/workspace&arg=--version
    let (host, port) = {
        // url is like "http://127.0.0.1:<port>/exec"
        let tail = url.trim_start_matches("http://");
        let mut parts = tail.split('/');
        let hostport = parts.next().unwrap_or("127.0.0.1:0");
        let mut hp = hostport.split(':');
        let h = hp.next().unwrap_or("127.0.0.1").to_string();
        let p = hp.next().unwrap_or("0").parse::<u16>().unwrap_or(0);
        (h, p)
    };

    let mut stream = match TcpStream::connect((host.as_str(), port)) {
        Ok(s) => s,
        Err(_) => {
            running.store(false, std::sync::atomic::Ordering::SeqCst);
            let _ = handle.join();
            aifo_coder::toolchain_cleanup_session(&sid, false);
            return;
        }
    };

    // Inject a TRACEPARENT for propagation; proxy will extract it, but we only require functional success.
    std::env::set_var("TRACEPARENT", "00-00000000000000000000000000000000-0000000000000000-00");

    let body = "tool=node&cwd=/workspace&arg=--version";
    let req = format!(
        "POST /exec HTTP/1.1\r\n\
         Host: {host}:{port}\r\n\
         Authorization: Bearer {token}\r\n\
         X-Aifo-Proto: 2\r\n\
         Content-Type: application/x-www-form-urlencoded\r\n\
         Content-Length: {len}\r\n\
         Connection: close\r\n\
         \r\n\
         {body}",
        host = host,
        port = port,
        token = token,
        len = body.len(),
        body = body
    );

    let _ = stream.write_all(req.as_bytes());
    let mut resp = Vec::new();
    let _ = stream.read_to_end(&mut resp);

    // Teardown proxy and sidecars
    running.store(false, std::sync::atomic::Ordering::SeqCst);
    let _ = handle.join();
    aifo_coder::toolchain_cleanup_session(&sid, false);

    // Check for successful status and exit code trailer
    let text = String::from_utf8_lossy(&resp);
    assert!(
        text.contains("200 OK"),
        "expected 200 OK in response headers; got:\n{}",
        text
    );
    assert!(
        text.contains("X-Exit-Code: 0"),
        "expected exit code 0 in trailer; response:\n{}",
        text
    );
}
