mod support;

#[ignore]
#[test]
fn e2e_test_proxy_shim_route_rust_and_node() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // Start sidecars for rust and node and the proxy
    let kinds = vec!["rust".to_string(), "node".to_string()];
    let overrides: Vec<(String, String)> = Vec::new();
    let no_cache = false;
    let verbose = true;

    let sid = aifo_coder::toolchain_start_session(&kinds, &overrides, no_cache, verbose)
        .expect("failed to start toolchain session");

    let (url, token, flag, handle) =
        aifo_coder::toolexec_start_proxy(&sid, verbose).expect("failed to start proxy");

    let port = support::port_from_http_url(&url);

    fn x_exit_code(headers: &str) -> i32 {
        for line in headers.lines() {
            if let Some(v) = line.strip_prefix("X-Exit-Code: ") {
                return v.trim().parse::<i32>().unwrap_or(1);
            }
        }
        1
    }

    // rust: cargo --version
    let (_status, headers, _body) = support::http_post_form_tcp(
        port,
        "/exec",
        &[("Authorization", &format!("Bearer {}", token)), ("X-Aifo-Proto", "1")],
        &[("tool", "cargo"), ("cwd", "."), ("arg", "--version")],
    );
    assert_eq!(x_exit_code(&headers), 0, "cargo --version failed via proxy");

    // node: npx --version
    let (_status, headers, _body) = support::http_post_form_tcp(
        port,
        "/exec",
        &[("Authorization", &format!("Bearer {}", token)), ("X-Aifo-Proto", "1")],
        &[("tool", "npx"), ("cwd", "."), ("arg", "--version")],
    );
    assert_eq!(x_exit_code(&headers), 0, "npx --version failed via proxy");

    // Cleanup
    flag.store(false, std::sync::atomic::Ordering::SeqCst);
    let _ = handle.join();
    aifo_coder::toolchain_cleanup_session(&sid, verbose);
}
