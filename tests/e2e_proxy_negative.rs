mod support;
#[ignore]
#[test]
fn e2e_test_proxy_unauthorized_and_unknown_tool() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // Start rust sidecar (enough for this negative test) and the proxy
    let kinds = vec!["rust".to_string()];
    let overrides: Vec<(String, String)> = Vec::new();
    let sid = aifo_coder::toolchain_start_session(&kinds, &overrides, false, true)
        .expect("failed to start sidecar session");
    let (url, token, flag, handle) =
        aifo_coder::toolexec_start_proxy(&sid, true).expect("failed to start proxy");

    let port = support::port_from_http_url(&url);

    // No Authorization header -> expect 401
    let (status, _headers, _body) =
        support::http_post_tcp(port, &[], &[("tool", "cargo"), ("cwd", ".")]);
    assert_eq!(status, 401, "expected 401, got status={}", status);

    // Unknown tool name with valid token -> expect 403
    let auth = format!("Bearer {}", token);
    let (status2, _headers2, _body2) = support::http_post_tcp(
        port,
        &[("Authorization", auth.as_str()), ("X-Aifo-Proto", "1")],
        &[("tool", "h4x0r"), ("cwd", ".")],
    );
    assert_eq!(status2, 403, "expected 403, got status={}", status2);

    // Cleanup
    flag.store(false, std::sync::atomic::Ordering::SeqCst);
    let _ = handle.join();
    aifo_coder::toolchain_cleanup_session(&sid, true);
}
