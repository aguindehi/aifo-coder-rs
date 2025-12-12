use std::io::{Read, Write};

#[test]
fn int_test_streaming_timeout_kills_child_and_trailers_exit_124() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // Short timeout
    std::env::set_var("AIFO_TOOLEEXEC_TIMEOUT_SECS", "1");

    // Start python sidecar (for a sleep command) and the proxy
    let rt = match aifo_coder::container_runtime_path() {
        Ok(p) => p,
        Err(_) => {
            eprintln!("skipping: docker not found in PATH");
            return;
        }
    };
    let python_image = std::env::var("AIFO_CODER_TEST_PYTHON_IMAGE")
        .unwrap_or_else(|_| "python:3.12-slim".to_string());
    let img_ok = std::process::Command::new(&rt)
        .args(["image", "inspect", &python_image])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
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

    let port = support::port_from_http_url(&url);

    // Request that exceeds timeout: python -c "import time; time.sleep(2)" with proto v2 (streaming)
    use std::net::TcpStream;
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect failed");
    let body = "tool=python&cwd=.&arg=-c&arg=import%20time%3b%20time.sleep(2)";
    let req = format!(
        "POST /exec HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {}\r\nX-Aifo-Proto: 2\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n\r\n{}",
        token, body.len(), body
    );
    stream.write_all(req.as_bytes()).expect("write failed");
    let mut resp = Vec::new();
    stream.read_to_end(&mut resp).ok();
    let text = String::from_utf8_lossy(&resp).to_string();

    // Must be chunked
    assert!(
        text.contains("Transfer-Encoding: chunked"),
        "expected chunked transfer; got:\n{}",
        text
    );
    // Timeout chunk must appear
    assert!(
        text.contains("aifo-coder proxy timeout"),
        "expected timeout chunk; got:\n{}",
        text
    );
    // Trailer exit code 124
    assert!(
        text.contains("X-Exit-Code: 124"),
        "expected trailer exit 124; got:\n{}",
        text
    );

    // Cleanup
    flag.store(false, std::sync::atomic::Ordering::SeqCst);
    let _ = handle.join();
    aifo_coder::toolchain_cleanup_session(&sid, true);
}
