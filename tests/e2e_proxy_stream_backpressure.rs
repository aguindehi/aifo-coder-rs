/*
// ignore-tidy-linelength
Acceptance test (ignored by default): simulate client stall to trigger v2 backpressure.
Ensures a single drop warning line is emitted and a verbose dropped counter appears.
*/
mod support;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;
#[ignore]
#[test]
fn e2e_proxy_v2_backpressure_emits_drop_warning_and_counter() {
    // Skip if docker isn't available on this host
    let _runtime = match aifo_coder::container_runtime_path() {
        Ok(p) => p,
        Err(_) => {
            eprintln!("skipping: docker not found in PATH");
            return;
        }
    };

    // Start a toolchain session with a node sidecar
    let kinds = vec!["node".to_string()];
    // Ensure Docker daemon reachable and node image present locally to avoid pulls
    let runtime = aifo_coder::container_runtime_path().expect("runtime");
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
    let node_image = support::default_node_test_image();
    if !support::docker_image_present(&runtime.as_path(), &node_image) {
        eprintln!("skipping: node image '{}' not present locally", node_image);
        return;
    }
    let overrides: Vec<(String, String)> = vec![("node".to_string(), node_image)];
    let sid = aifo_coder::toolchain_start_session(&kinds, &overrides, false, false)
        .expect("toolchain_start_session");
    // Force proxy to use the container's default user (avoid odd host UID/GID failures)
    std::env::set_var("AIFO_TOOLEEXEC_DISABLE_USER", "1");
    // Disable TTY to avoid PTY quirks and ensure pipe behavior
    std::env::set_var("AIFO_TOOLEEXEC_TTY", "0");
    // Shrink channel capacity to accentuate backpressure
    std::env::set_var("AIFO_PROXY_CHANNEL_CAP", "1");
    // Start proxy in verbose mode
    let (url, token, running, handle) =
        aifo_coder::toolexec_start_proxy(&sid, true).expect("start proxy");

    // Capture proxy logs to a temp file
    let td = tempfile::tempdir().expect("tmpdir");
    let log_path = td.path().join("proxy.log");
    std::env::set_var("AIFO_TEST_LOG_PATH", &log_path);
    // Speed up disconnect handling for the test
    std::env::set_var("AIFO_PROXY_SIGNAL_GRACE_MS", "0");

    // Extract port from http://127.0.0.1:<port>/exec
    let port = support::port_from_http_url(&url);

    // Build a request body that streams a lot of output quickly
    // Use node to generate an effectively infinite stream using blocking writes (no shell)
    let script = "const fs=require('fs');const b='x\\n'.repeat(65536);for(;;){try{fs.writeSync(1,b);}catch(e){process.exit(0);}}";
    let body = format!(
        "tool={}&cwd={}&arg={}&arg={}",
        support::urlencode("node"),
        support::urlencode("/workspace"),
        support::urlencode("-e"),
        support::urlencode(script)
    );

    // Compose raw HTTP/1.1 request
    let mut req = format!(
        concat!(
            "POST /exec HTTP/1.1\r\n",
            "Host: localhost\r\n",
            "Authorization: Bearer {tok}\r\n",
            "X-Aifo-Proto: 2\r\n",
            "Content-Type: application/x-www-form-urlencoded\r\n",
            "Content-Length: {len}\r\n",
            "Connection: close\r\n",
            "\r\n"
        ),
        tok = token,
        len = body.len()
    );
    req.push_str(&body);

    // Connect and send request
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    stream.write_all(req.as_bytes()).expect("write request");

    // Read only until header end to cause server write backpressure later
    let mut buf = Vec::<u8>::new();
    let mut tmp = [0u8; 1024];
    loop {
        if aifo_coder::find_header_end(&buf).is_some() {
            break;
        }
        match stream.read(&mut tmp) {
            Ok(0) => break,
            Ok(n) => buf.extend_from_slice(&tmp[..n]),
            Err(_) => break,
        }
    }

    // Close connection immediately to force proxy-side write failure/backpressure
    use std::net::Shutdown;
    let _ = stream.shutdown(Shutdown::Both);
    drop(stream);

    // Allow proxy threads to process cleanup and emit logs
    std::thread::sleep(Duration::from_millis(500));

    // Stop proxy and cleanup session
    running.store(false, std::sync::atomic::Ordering::SeqCst);
    let _ = handle.join();
    aifo_coder::toolchain_cleanup_session(&sid, false);

    // Assert drop warning and verbose counter presence
    let log_text = std::fs::read_to_string(&log_path).unwrap_or_default();
    assert!(
        log_text.contains("proxy stream: dropping output (backpressure)"),
        "expected single drop warning in proxy logs; logs:\n{}",
        log_text
    );
    assert!(
        log_text.contains("proxy stream: dropped "),
        "expected verbose dropped counter in proxy logs; logs:\n{}",
        log_text
    );

    // Cleanup env
    std::env::remove_var("AIFO_TEST_LOG_PATH");
    std::env::remove_var("AIFO_PROXY_SIGNAL_GRACE_MS");
    std::env::remove_var("AIFO_TOOLEEXEC_DISABLE_USER");
    std::env::remove_var("AIFO_TOOLEEXEC_TTY");
    std::env::remove_var("AIFO_PROXY_CHANNEL_CAP");
}
