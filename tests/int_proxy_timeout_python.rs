mod support;

#[test]
fn int_proxy_timeout_python_sleep() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // Short timeout
    std::env::set_var("AIFO_TOOLEEXEC_TIMEOUT_SECS", "1");

    // Start python sidecar and proxy
    let rt = match aifo_coder::container_runtime_path() {
        Ok(p) => p,
        Err(_) => {
            eprintln!("skipping: docker not found in PATH");
            return;
        }
    };
    let python_image = support::default_python_test_image();
    let img_ok = support::docker_image_present(&rt.as_path(), &python_image);
    if !img_ok {
        eprintln!(
            "skipping: python image '{}' not present locally",
            python_image
        );
        return;
    }
    let kinds = vec!["python".to_string()];
    let overrides: Vec<(String, String)> = vec![("python".to_string(), python_image)];
    let sid = aifo_coder::toolchain_start_session(&kinds, &overrides, false, true)
        .expect("failed to start sidecar session");
    let (url, token, flag, handle) =
        aifo_coder::toolexec_start_proxy(&sid, true).expect("failed to start proxy");

    fn extract_port(u: &str) -> u16 {
        support::port_from_http_url(u)
    }
    let port = extract_port(&url);

    // Request that exceeds timeout: python -c "import time; time.sleep(2)"
    use std::io::{Read, Write};
    use std::net::TcpStream;
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect failed");
    let body = "tool=python&cwd=.&arg=-c&arg=import%20time%3b%20time.sleep(2)";
    let req = format!(
        "POST /exec HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {}\r\nX-Aifo-Proto: 1\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n\r\n{}",
        token, body.len(), body
    );
    stream.write_all(req.as_bytes()).expect("write failed");
    let mut resp = Vec::new();
    stream.read_to_end(&mut resp).ok();
    let text = String::from_utf8_lossy(&resp).to_string();
    assert!(
        text.contains("504 Gateway Timeout"),
        "expected 504, got:\n{}",
        text
    );
    assert!(
        text.contains("X-Exit-Code: 124"),
        "expected exit 124, got:\n{}",
        text
    );

    // Cleanup
    flag.store(false, std::sync::atomic::Ordering::SeqCst);
    let _ = handle.join();
    aifo_coder::toolchain_cleanup_session(&sid, true);
}
