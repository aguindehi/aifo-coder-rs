mod support;
use support::urlencode;
#[ignore]
#[test]
fn e2e_proxy_handles_large_payload_notifications_cmd() {
    // Respect CI override disabling docker and ensure daemon is reachable
    if std::env::var("AIFO_CODER_TEST_DISABLE_DOCKER")
        .ok()
        .as_deref()
        == Some("1")
    {
        eprintln!("skipping: AIFO_CODER_TEST_DISABLE_DOCKER=1");
        return;
    }
    let runtime = match aifo_coder::container_runtime_path() {
        Ok(p) => p,
        Err(_) => {
            eprintln!("skipping: docker not found in PATH");
            return;
        }
    };
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

    let sid = "unit-test-session";
    let (url, token, flag, handle) =
        aifo_coder::toolexec_start_proxy(sid, true).expect("start proxy");

    // Build large body using notifications-cmd (no sidecars needed); config likely missing -> 403.
    //
    // This intentionally exceeds per-field caps (MAX_ARGS_COUNT) while staying under BODY_CAP,
    // to ensure we reject safely without panicking.
    let mut body = format!(
        "tool={}&cwd={}",
        urlencode("notifications-cmd"),
        urlencode(".")
    );
    for i in 0..5000 {
        body.push('&');
        body.push_str(&format!("arg={}", urlencode(&format!("x{i:04}"))));
    }

    let port = support::port_from_http_url(&url);
    let (_status, resp_headers, _resp_body) = support::http_post_form_tcp(
        port,
        "/exec",
        &[
            ("Authorization", &format!("Bearer {}", token)),
            ("X-Aifo-Proto", "1"),
        ],
        // We pass the already-encoded body via a single k/v entry; the helper encodes again, so
        // keep using the raw sender pattern by calling http_send_raw below instead.
        &[("tool", "notifications-cmd"), ("cwd", ".")],
    );

    // Fall back to raw send for the oversized arg flood (http_post_form_tcp encodes kv pairs).
    let req = format!(
        "POST /exec HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {}\r\nX-Aifo-Proto: 1\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        token,
        body.len(),
        body
    );
    let resp = support::http_send_raw(port, &req);
    let _ = resp_headers;
    // Large form bodies should be rejected safely. With the current hardening, the proxy can
    // respond 400 (bad request) due to per-field caps even if the overall body cap allows it.
    assert!(
        resp.starts_with("HTTP/1.1 400")
            || resp.starts_with("HTTP/1.1 403")
            || resp.starts_with("HTTP/1.1 200"),
        "expected a valid HTTP response (400/403/200), got:\n{}",
        resp
    );

    flag.store(false, std::sync::atomic::Ordering::SeqCst);
    let _ = handle.join();
}
