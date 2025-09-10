#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
#[ignore]
#[test]
fn test_proxy_tcp_streaming_rust_and_node() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // Ensure unix mode is disabled to force TCP
    std::env::remove_var("AIFO_TOOLEEXEC_USE_UNIX");

    // Start sidecars for rust and node and the proxy
    let kinds = vec!["rust".to_string(), "node".to_string()];
    let overrides: Vec<(String, String)> = Vec::new();
    let no_cache = false;
    let verbose = true;

    let sid = aifo_coder::toolchain_start_session(&kinds, &overrides, no_cache, verbose)
        .expect("failed to start toolchain session");

    let (url, token, flag, handle) =
        aifo_coder::toolexec_start_proxy(&sid, verbose).expect("failed to start proxy");

    // Expect a TCP URL like http://host.docker.internal:<port>/exec
    assert!(
        url.starts_with("http://"),
        "expected http:// URL, got: {url}"
    );

    // Extract port from URL; connect to localhost:<port> from the host
    let port: u16 = {
        let without_proto = url.trim_start_matches("http://");
        // host:port/rest
        let host_port = without_proto.split('/').next().unwrap_or(without_proto);
        let p = host_port
            .rsplitn(2, ':')
            .next()
            .and_then(|s| s.parse::<u16>().ok())
            .expect("failed to parse port from URL");
        p
    };

    fn post_exec_tcp_v2(port: u16, token: &str, tool: &str, args: &[&str]) -> (i32, String) {
        use std::io::{BufRead, BufReader, Read, Write};
        use std::net::TcpStream;

        let mut stream =
            TcpStream::connect(("127.0.0.1", port)).expect("connect 127.0.0.1:<port> failed");

        let mut body = format!(
            "tool={}&cwd={}",
            urlencoding::Encoded::new(tool),
            urlencoding::Encoded::new(".")
        );
        for a in args {
            body.push('&');
            body.push_str(&format!("arg={}", urlencoding::Encoded::new(a)));
        }

        let req = format!(
            "POST /exec HTTP/1.1\r\nHost: host.docker.internal\r\nAuthorization: Bearer {}\r\nX-Aifo-Proto: 2\r\nTE: trailers\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            token,
            body.len(),
            body
        );
        stream.write_all(req.as_bytes()).expect("write failed");

        // Read headers until CRLFCRLF
        let mut reader = BufReader::new(stream);
        let mut header = String::new();
        loop {
            let mut line = String::new();
            let n = reader
                .read_line(&mut line)
                .expect("read header line failed");
            if n == 0 {
                break;
            }
            header.push_str(&line);
            if header.ends_with("\r\n\r\n") || header.ends_with("\n\n") {
                break;
            }
            if header.len() > 128 * 1024 {
                break;
            }
        }

        // Ensure chunked transfer and Trailer header present
        assert!(
            header
                .to_ascii_lowercase()
                .contains("transfer-encoding: chunked"),
            "expected chunked transfer, header: {}",
            header
        );
        assert!(
            header.to_ascii_lowercase().contains("trailer: x-exit-code"),
            "expected Trailer: X-Exit-Code header, got: {}",
            header
        );

        // Read chunked body; we don't need to assemble it fully, but keep for return text
        let mut body_out = Vec::new();
        loop {
            // Read chunk size line
            let mut size_line = String::new();
            reader
                .read_line(&mut size_line)
                .expect("read chunk size failed");
            if size_line.is_empty() {
                break;
            }
            let size_str = size_line.trim();
            // Allow optional chunk extensions
            let size_only = size_str.split(';').next().unwrap_or(size_str);
            let Ok(sz) = usize::from_str_radix(size_only, 16) else {
                break;
            };
            if sz == 0 {
                // Now trailers follow, terminated by an empty line
                break;
            }
            // Read exactly sz bytes
            let mut chunk = vec![0u8; sz];
            reader
                .read_exact(&mut chunk)
                .expect("read chunk data failed");
            body_out.extend_from_slice(&chunk);
            // Consume CRLF after chunk
            let mut crlf = [0u8; 2];
            reader.read_exact(&mut crlf).expect("read CRLF failed");
        }

        // Read trailers until blank line; extract X-Exit-Code
        let mut code: i32 = 1;
        loop {
            let mut tline = String::new();
            let n = reader.read_line(&mut tline).unwrap_or(0);
            if n == 0 {
                break;
            }
            let tl = tline.trim_end_matches(|c| c == '\r' || c == '\n');
            if tl.is_empty() {
                break;
            }
            if let Some(v) = tl.strip_prefix("X-Exit-Code: ") {
                code = v.trim().parse::<i32>().unwrap_or(1);
            }
        }

        let text = String::from_utf8_lossy(&body_out).to_string();
        (code, text)
    }

    // rust: cargo --version
    let (code_rust_v2, _out_rust_v2) = post_exec_tcp_v2(port, &token, "cargo", &["--version"]);
    assert_eq!(code_rust_v2, 0, "cargo --version failed via tcp proxy (v2)");

    // node: npx --version
    let (code_node_v2, _out_node_v2) = post_exec_tcp_v2(port, &token, "npx", &["--version"]);
    assert_eq!(code_node_v2, 0, "npx --version failed via tcp proxy (v2)");

    // Cleanup
    flag.store(false, std::sync::atomic::Ordering::SeqCst);
    let _ = handle.join();
    aifo_coder::toolchain_cleanup_session(&sid, verbose);
}
