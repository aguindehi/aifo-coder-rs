mod support;
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
#[ignore]
#[test]
fn proxy_streaming_slow_consumer_disconnect() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // Use node sidecar (skip if image not present locally to avoid pulling)
    let rt = aifo_coder::container_runtime_path().expect("runtime");
    let node_image = std::env::var("AIFO_CODER_TEST_NODE_IMAGE")
        .unwrap_or_else(|_| "node:20-bookworm-slim".into());
    let img_ok = std::process::Command::new(&rt)
        .args(["image", "inspect", &node_image])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !img_ok {
        eprintln!("skipping: node image '{}' not present locally", node_image);
        return;
    }

    // Temp log file for proxy logs
    let td = tempfile::tempdir().expect("tmpdir");
    let log_path = td.path().join("proxy.log");
    std::env::set_var("AIFO_TEST_LOG_PATH", &log_path);

    let kinds = vec!["node".to_string()];
    let overrides = vec![("node".to_string(), node_image.clone())];
    let sid = aifo_coder::toolchain_start_session(&kinds, &overrides, false, true)
        .expect("start sidecar");

    let (url, token, flag, handle) =
        aifo_coder::toolexec_start_proxy(&sid, true).expect("start proxy");

    // Extract port
    fn port_from_url(url: &str) -> u16 {
        support::port_from_http_url(url)
    }
    let port = port_from_url(&url);

    // Issue a v2 streaming request that writes steadily; do not read much, then close.
    use std::io::Write;
    use std::net::TcpStream;
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    let body = "tool=node&cwd=.&arg=-e&arg=setInterval(()=>console.log('x'.repeat(8192)),50)";
    let req = format!(
        "POST /exec HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {}\r\nX-Aifo-Proto: 2\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n\r\n{}",
        token,
        body.len(),
        body
    );
    stream.write_all(req.as_bytes()).expect("write");
    // Sleep a bit to let output accumulate and proxy try to write
    std::thread::sleep(std::time::Duration::from_millis(400));
    // Drop the connection without reading much
    drop(stream);

    // Wait for proxy to record disconnect and escalation sequence
    std::thread::sleep(std::time::Duration::from_millis(1600));

    // Check logs for disconnect and escalation messages
    let log_content = std::fs::read_to_string(&log_path).unwrap_or_default();
    assert!(
        log_content.contains("aifo-coder: disconnect"),
        "expected disconnect log; got:\n{}",
        log_content
    );
    assert!(
        log_content.contains("disconnect escalate: sending INT"),
        "expected INT escalation log; got:\n{}",
        log_content
    );
    assert!(
        log_content.contains("disconnect escalate: sending TERM"),
        "expected TERM escalation log; got:\n{}",
        log_content
    );
    assert!(
        log_content.contains("disconnect escalate: sending KILL"),
        "expected KILL escalation log; got:\n{}",
        log_content
    );

    // Cleanup
    flag.store(false, std::sync::atomic::Ordering::SeqCst);
    let _ = handle.join();
    aifo_coder::toolchain_cleanup_session(&sid, true);
    std::env::remove_var("AIFO_TEST_LOG_PATH");
}
