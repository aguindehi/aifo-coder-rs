#[test]
fn int_test_proxy_allowlist_rejects_disallowed_tool() {
    // Skip if docker isn't available on this host (proxy still needs to bind)
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // Start proxy without launching sidecars; allowlist check happens before docker exec
    let sid = format!("allowlist-{}", std::process::id());
    let (url, _token, flag, handle) =
        aifo_coder::toolexec_start_proxy(&sid, true).expect("failed to start proxy");

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

    // Try to execute disallowed tool 'bash' via query → expect 403 (allowlist fast-path, auth-independent)
    use std::io::{Read, Write};
    use std::net::TcpStream;
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect failed");
    let req = "POST /exec?tool=bash&cwd=. HTTP/1.1\r\nHost: localhost\r\nX-Aifo-Proto: 1\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string();
    stream.write_all(req.as_bytes()).expect("write failed");
    let mut resp = Vec::new();
    stream.read_to_end(&mut resp).ok();
    let text = String::from_utf8_lossy(&resp).to_string();
    assert!(
        text.contains("403 Forbidden"),
        "expected 403, got:\n{}",
        text
    );

    // Cleanup
    flag.store(false, std::sync::atomic::Ordering::SeqCst);
    let _ = handle.join();
    aifo_coder::toolchain_cleanup_session(&sid, true);
}
