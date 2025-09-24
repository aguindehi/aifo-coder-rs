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

    let (url, token, flag, handle) =
        aifo_coder::toolexec_start_proxy(&sid, verbose).expect("failed to start proxy");

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

        let mut body = format!("tool={}&cwd={}", urlencode(tool), urlencode("."));
        for a in args {
            body.push('&');
            body.push_str(&format!("arg={}", urlencode(a)));
        }

        let req = format!(
            "POST /exec HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {}\r\nX-Aifo-Proto: 1\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n\r\n{}",
            token,
            body.len(),
            body
        );
        stream.write_all(req.as_bytes()).expect("write failed");

        // Read headers until CRLFCRLF
        let mut buf = Vec::new();
        let mut tmp = [0u8; 1024];
        let header_end_pos = loop {
            let n = stream.read(&mut tmp).expect("read failed");
            if n == 0 {
                break None;
            }
            buf.extend_from_slice(&tmp[..n]);
            if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                break Some(pos + 4);
            }
            if buf.len() > 128 * 1024 {
                break None;
            }
        };
        let hend = header_end_pos.expect("no header terminator found");
        let header = String::from_utf8_lossy(&buf[..hend]).to_string();

        // Parse X-Exit-Code and Content-Length
        let mut code: i32 = 1;
        let mut content_len: usize = 0;
        for line in header.lines() {
            if let Some(v) = line.strip_prefix("X-Exit-Code: ") {
                code = v.trim().parse::<i32>().unwrap_or(1);
            } else if let Some(v) = line.strip_prefix("Content-Length: ") {
                content_len = v.trim().parse::<usize>().unwrap_or(0);
            }
        }

        // Read exactly content_len bytes of body (may already have some in buf)
        let mut body_bytes = buf[hend..].to_vec();
        while body_bytes.len() < content_len {
            let n = stream.read(&mut tmp).expect("read body failed");
            if n == 0 {
                break;
            }
            body_bytes.extend_from_slice(&tmp[..n]);
        }

        let text = String::from_utf8_lossy(&body_bytes).to_string();
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
