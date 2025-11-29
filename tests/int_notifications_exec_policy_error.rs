mod support;

#[test]
fn int_test_notifications_policy_error_403() {
    // Skip if docker isn't available on this host (proxy requires docker CLI path for runtime)
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // Enable noauth notifications mode
    std::env::set_var("AIFO_NOTIFICATIONS_NOAUTH", "1");

    // Write a minimal allowlist config; current policy path returns 403 "not found"
    // when notifications-command is not present/allowed by the configured allowlist.
    // Isolate HOME so we don't touch the user's real ~/.aider.conf.yml.
    let old_home = std::env::var("HOME").ok();
    let td_home = tempfile::tempdir().expect("tmpdir-home");
    let new_home = td_home.path().to_path_buf();
    std::env::set_var("HOME", &new_home);
    let cfg_path = new_home.join(".aider.conf.yml");
    let cfg = "notifications:\n  allowlist:\n    - say\n";
    let _ = std::fs::write(&cfg_path, cfg);

    let sid = "ut-notify-spawnerr";
    let (url, _token, flag, handle) =
        aifo_coder::toolexec_start_proxy(sid, true).expect("start proxy");

    // Connect and send a /notify request with an absolute non-existent 'say' path
    let port = support::port_from_http_url(&url);

    use std::io::{Read, Write};
    use std::net::TcpStream;

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
        resp.contains("403 Forbidden"),
        "expected 403, got:\n{}",
        resp
    );
    assert!(
        resp.to_ascii_lowercase().contains("x-exit-code: 86"),
        "expected X-Exit-Code: 86 header, got:\n{}",
        resp
    );
    assert!(
        resp.to_ascii_lowercase().contains("not found"),
        "expected error message to mention allowlist not found, got:\n{}",
        resp
    );

    // Cleanup
    let _ = std::fs::remove_file(&cfg_path);
    flag.store(false, std::sync::atomic::Ordering::SeqCst);
    let _ = handle.join();
    std::env::remove_var("AIFO_NOTIFICATIONS_NOAUTH");
    // Restore HOME
    if let Some(v) = old_home {
        std::env::set_var("HOME", v);
    } else {
        std::env::remove_var("HOME");
    }
}
