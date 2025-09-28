/*
// ignore-tidy-linelength
Acceptance test (ignored by default): simulate client stall to trigger v2 backpressure.
Ensures a single drop warning line is emitted and a verbose dropped counter appears.
*/
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

fn find_header_end(buf: &[u8]) -> Option<usize> {
    if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
        Some(i + 4)
    } else {
        buf.windows(2).position(|w| w == b"\n\n").map(|i| i + 2)
    }
}

fn urlencode_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b' ' => out.push('+'),
            b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b'0'..=b'9' | b'a'..=b'z' | b'A'..=b'Z' => out.push(b as char),
            _ => {
                out.push('%');
                out.push_str(&format!("{:02X}", b));
            }
        }
    }
    out
}

#[test]
#[ignore]
fn test_proxy_v2_backpressure_emits_drop_warning_and_counter() {
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
    let overrides: Vec<(String, String)> = Vec::new();
    let sid = aifo_coder::toolchain_start_session(&kinds, &overrides, false, false)
        .expect("toolchain_start_session");
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
    assert!(
        url.starts_with("http://127.0.0.1:"),
        "expected tcp proxy url, got: {}",
        url
    );
    let rest = url.trim_start_matches("http://127.0.0.1:");
    let port_str = rest.split('/').next().unwrap_or(rest);
    let port = port_str.parse::<u16>().expect("port parse");

    // Build a request body that streams a lot of output quickly
    // Script: generate many lines rapidly to fill the bounded channel
    let script = "i=0; while [ $i -lt 8000 ]; do echo x; i=$((i+1)); done";
    let body = format!(
        "tool={}&cwd={}&arg={}&arg={}",
        urlencode_component("sh"),
        urlencode_component("/workspace"),
        urlencode_component("-lc"),
        urlencode_component(script)
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
        if find_header_end(&buf).is_some() {
            break;
        }
        match stream.read(&mut tmp) {
            Ok(0) => break,
            Ok(n) => buf.extend_from_slice(&tmp[..n]),
            Err(_) => break,
        }
    }

    // Stall client reads to induce server-side backpressure
    std::thread::sleep(Duration::from_millis(600));
    // Close connection now (server will detect write failure and escalate)
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
}
