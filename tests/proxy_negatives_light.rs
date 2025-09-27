use std::io::{Read, Write};

#[test]
fn test_proxy_unauthorized_without_sidecars_and_missing_tool_body() {
    // Need docker present to start proxy, no sidecars required
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found");
        return;
    }

    let sid = format!("lightneg-{}", std::process::id());
    let (url, token, flag, handle) =
        aifo_coder::toolexec_start_proxy(&sid, false).expect("start proxy");

    fn extract_port(u: &str) -> u16 {
        support::port_from_http_url(u)
    }
    let port = extract_port(&url);

    // 401 Unauthorized (no Authorization header)
    {
        let (status, _headers, _body) =
            support::http_post_tcp(port, &[], &[("tool", "cargo"), ("cwd", ".")]);
        assert_eq!(status, 401, "expected 401, got status={}", status);
    }

    // 400 Bad Request (missing tool param in body)
    {
        let (status, _headers, _body) = support::http_post_tcp(
            port,
            &[("Authorization", &format!("Bearer {}", token)), ("X-Aifo-Proto", "1")],
            &[("cwd", ".")],
        );
        assert_eq!(status, 400, "expected 400, got status={}", status);
    }

    // Cleanup
    flag.store(false, std::sync::atomic::Ordering::SeqCst);
    let _ = handle.join();
    aifo_coder::toolchain_cleanup_session(&sid, false);
}
