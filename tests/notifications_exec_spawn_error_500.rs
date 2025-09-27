#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
#[test]
fn test_notifications_exec_spawn_error_500() {
    // This test is gated by AIFO_CODER_TEST_ENABLE_NOTIFY_SPAWN_500=1 because it requires
    // a notifications config that maps 'say' to a non-existent or non-executable absolute path
    // to trigger ExecSpawn=500. Set AIFO_CODER_TEST_ENABLE_NOTIFY_SPAWN_500=1 to run it.

    // Skip if docker isn't available on this host (proxy requires docker CLI path for runtime)
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // Gate: only run when explicitly enabled
    if std::env::var("AIFO_CODER_TEST_ENABLE_NOTIFY_SPAWN_500")
        .ok()
        .as_deref()
        != Some("1")
    {
        eprintln!("skipping: AIFO_CODER_TEST_ENABLE_NOTIFY_SPAWN_500 not set to 1");
        return;
    }

    // Enable noauth notifications mode
    std::env::set_var("AIFO_NOTIFICATIONS_NOAUTH", "1");

    // Write a config with an allowlist and a mapping intended to force spawn failure.
    // Note: Adjust to your actual notifications config schema if different.
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let cfg_path = std::path::Path::new(&home).join(".aider.conf.yml");
    let cfg = r#"notifications:
  allowlist:
    - say
  mappings:
    say: /no/such/say
"#;
    let _ = std::fs::write(&cfg_path, cfg);

    let sid = "ut-notify-spawn-500";
    let (url, _token, flag, handle) =
        aifo_coder::toolexec_start_proxy(sid, true).expect("start proxy");

    // Extract port from http URL
    fn port_from_url(url: &str) -> u16 {
        let after = url.split("://").nth(1).unwrap_or(url);
        let host_port = after.split('/').next().unwrap_or(after);
        host_port
            .rsplit(':')
            .next()
            .unwrap_or("0")
            .parse()
            .unwrap_or(0)
    }
    let port = port_from_url(&url);

    use std::io::{Read, Write};
    use std::net::TcpStream;

    // Send cmd=say which should resolve to a non-existent path, causing ExecSpawn -> 500
    let body = "cmd=say&arg=hello";
    let req = format!(
        "POST /notify HTTP/1.1\r\n\
         Host: localhost\r\n\
         X-Aifo-Proto: 2\r\n\
         Content-Type: application/x-www-form-urlencoded\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\r\n{}",
        body.len(),
        body
    );

    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    stream.write_all(req.as_bytes()).expect("write");

    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).ok();
    let resp = String::from_utf8_lossy(&buf).to_string();

    assert!(
        resp.contains("500 Internal Server Error"),
        "expected 500, got:\n{}",
        resp
    );
    assert!(
        resp.to_ascii_lowercase().contains("x-exit-code: 86"),
        "expected X-Exit-Code: 86 header, got:\n{}",
        resp
    );

    // Cleanup
    flag.store(false, std::sync::atomic::Ordering::SeqCst);
    let _ = handle.join();
    std::env::remove_var("AIFO_NOTIFICATIONS_NOAUTH");
}
