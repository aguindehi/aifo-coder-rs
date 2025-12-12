mod support;

#[test]
fn int_proxy_unauthorized_without_sidecars_and_missing_tool_body() {
    // Need docker present to start proxy, no sidecars required
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found");
        return;
    }

    let sid = format!("lightneg-{}", std::process::id());
    let (url, token, flag, handle) =
        aifo_coder::toolexec_start_proxy(&sid, false).expect("start proxy");

    let port = support::port_from_http_url(&url);

    // 401 Unauthorized (no Authorization header)
    {
        let (status, _headers, _body) =
            support::http_post_tcp(port, &[], &[("tool", "cargo"), ("cwd", ".")]);
        assert_eq!(status, 401, "expected 401, got status={}", status);
    }

    // 400 Bad Request (missing tool param in body)
    {
        let auth = format!("Bearer {}", token);
        let (status, _headers, _body) = support::http_post_tcp(
            port,
            &[("Authorization", auth.as_str()), ("X-Aifo-Proto", "1")],
            &[("cwd", ".")],
        );
        assert_eq!(status, 400, "expected 400, got status={}", status);
    }

    // Cleanup
    flag.store(false, std::sync::atomic::Ordering::SeqCst);
    let _ = handle.join();
    aifo_coder::toolchain_cleanup_session(&sid, false);
}
