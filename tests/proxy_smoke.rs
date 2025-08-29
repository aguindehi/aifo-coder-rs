#[ignore]
#[test]
fn test_proxy_shim_route_rust_and_node() {
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

    let (url, token, flag, handle) = aifo_coder::toolexec_start_proxy(&sid, verbose)
        .expect("failed to start proxy");

    // Helper to extract host:port from url "http://host.docker.internal:PORT/exec"
    fn extract_port(u: &str) -> u16 {
        let after_scheme = u.split("://").nth(1).unwrap_or(u);
        let host_port = after_scheme.split('/').next().unwrap_or(after_scheme);
        let port_str = host_port.rsplit(':').next().unwrap_or("0");
        port_str.parse::<u16>().unwrap_or(0)
    }

    fn post_exec(port: u16, token: &str, tool: &str, args: &[&str]) -> (i32, String) {
        use std::io::{Read, Write};
        use std::net::TcpStream;

        let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect failed");

        let mut body = format!("tool={}&cwd={}", urlencoding::Encoded::new(tool), urlencoding::Encoded::new("."));
        for a in args {
            body.push('&');
            body.push_str(&format!("arg={}", urlencoding::Encoded::new(a)));
        }

        let req = format!(
            "POST /exec HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {}\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n\r\n{}",
            token,
            body.len(),
            body
        );
        stream.write_all(req.as_bytes()).expect("write failed");

        let mut resp = Vec::new();
        stream.read_to_end(&mut resp).ok();

        let text = String::from_utf8_lossy(&resp).to_string();
        let mut code: i32 = 1;
        for line in text.lines() {
            if let Some(v) = line.strip_prefix("X-Exit-Code: ") {
                code = v.trim().parse::<i32>().unwrap_or(1);
            }
            if line.trim().is_empty() {
                break;
            }
        }
        (code, text)
    }

    let port = extract_port(&url);

    // rust: cargo --version
    let (code_rust, _out_rust) = post_exec(port, &token, "cargo", &["--version"]);
    assert_eq!(code_rust, 0, "cargo --version failed via proxy");

    // node: npx --version
    let (code_node, _out_node) = post_exec(port, &token, "npx", &["--version"]);
    assert_eq!(code_node, 0, "npx --version failed via proxy");

    // Cleanup
    flag.store(false, std::sync::atomic::Ordering::SeqCst);
    let _ = handle.join();
    aifo_coder::toolchain_cleanup_session(&sid, verbose);
}
