mod support;
use support::urlencode;
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
#[ignore]
#[test]
fn test_error_semantics_tcp_v1_and_v2() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // Helper: parse port from http://host.docker.internal:<port>/exec
    fn port_from_url(url: &str) -> u16 {
        assert!(
            url.starts_with("http://"),
            "expected http:// URL, got: {url}"
        );
        let without_proto = url.trim_start_matches("http://");
        let host_port = without_proto.split('/').next().unwrap_or(without_proto);
        host_port
            .rsplit(':')
            .next()
            .and_then(|s| s.parse::<u16>().ok())
            .expect("failed to parse port from URL")
    }

    // Helper: open TCP connection to localhost:<port> and send a request, return (status, headers, body)
    fn http_post_tcp(
        port: u16,
        headers: &[(&str, &str)],
        body_kv: &[(&str, &str)],
    ) -> (u16, String, Vec<u8>) {
        use std::io::{Read, Write};
        use std::net::TcpStream;

        let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect failed");

        let mut body = String::new();
        for (i, (k, v)) in body_kv.iter().enumerate() {
            if i > 0 {
                body.push('&');
            }
            body.push_str(&format!("{}={}", urlencode(k), urlencode(v)));
        }

        // Build headers
        let mut req = format!(
            "POST /exec HTTP/1.1\r\nHost: host.docker.internal\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\nConnection: close\r\n",
            body.len()
        );
        for (k, v) in headers {
            req.push_str(&format!("{k}: {v}\r\n"));
        }
        req.push_str("\r\n");
        req.push_str(&body);

        stream.write_all(req.as_bytes()).expect("write failed");

        let mut buf = Vec::new();
        let mut tmp = [0u8; 1024];
        loop {
            match stream.read(&mut tmp) {
                Ok(0) => break,
                Ok(n) => buf.extend_from_slice(&tmp[..n]),
                Err(_) => break,
            }
        }

        // Split headers/body
        let mut status: u16 = 0;
        let headers_s;
        let mut body_out: Vec<u8> = Vec::new();

        if let Some(pos) =
            aifo_coder::find_crlfcrlf(&buf).or_else(|| buf.windows(2).position(|w| w == b"\n\n"))
        {
            let h = &buf[..pos];
            headers_s = String::from_utf8_lossy(h).to_string();
            // Parse status
            if let Some(line) = headers_s.lines().next() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    status = parts[1].parse::<u16>().unwrap_or(0);
                }
            }
            // Body (best-effort; not handling chunked here)
            let mut body_bytes = buf[pos..].to_vec();
            // Drop leading CRLFCRLF or LF+LF
            while body_bytes.first() == Some(&b'\r') || body_bytes.first() == Some(&b'\n') {
                body_bytes.remove(0);
            }
            body_out = body_bytes;
        } else {
            headers_s = String::from_utf8_lossy(&buf).to_string();
        }
        (status, headers_s, body_out)
    }

    // 1) 401 Unauthorized (no Authorization header)
    {
        std::env::remove_var("AIFO_TOOLEEXEC_USE_UNIX");
        let sid = "ut-err-401";
        let (_url, _token, flag, handle) =
            aifo_coder::toolexec_start_proxy(sid, true).expect("start proxy");
        let port = port_from_url(&_url);
        let (status, headers, body) = http_post_tcp(port, &[], &[("tool", "cargo"), ("cwd", ".")]);
        assert_eq!(
            status, 401,
            "expected 401, got {status}\nheaders:\n{headers}"
        );
        assert!(
            headers.to_ascii_lowercase().contains("x-exit-code: 86"),
            "expected X-Exit-Code: 86 in headers:\n{}",
            headers
        );
        assert_eq!(String::from_utf8_lossy(&body), "unauthorized\n");
        flag.store(false, std::sync::atomic::Ordering::SeqCst);
        let _ = handle.join();
    }

    // 2) 426 Upgrade Required (Authorization valid but missing/unsupported X-Aifo-Proto)
    {
        std::env::remove_var("AIFO_TOOLEEXEC_USE_UNIX");
        let sid = "ut-err-426";
        let (url, token, flag, handle) =
            aifo_coder::toolexec_start_proxy(sid, true).expect("start proxy");
        let port = port_from_url(&url);
        // Send Authorization but no X-Aifo-Proto
        let (status, headers, body) = http_post_tcp(
            port,
            &[("Authorization", &format!("Bearer {}", token))],
            &[("tool", "cargo"), ("cwd", ".")],
        );
        assert_eq!(
            status, 426,
            "expected 426, got {status}\nheaders:\n{headers}"
        );
        assert!(
            String::from_utf8_lossy(&body).contains("expected 1 or 2"),
            "expected body to mention expected 1 or 2, got:\n{}",
            String::from_utf8_lossy(&body)
        );
        flag.store(false, std::sync::atomic::Ordering::SeqCst);
        let _ = handle.join();
    }

    // 3) 403 Forbidden (tool not allowed)
    {
        std::env::remove_var("AIFO_TOOLEEXEC_USE_UNIX");
        // Start rust sidecar (to pass routing), then proxy
        let kinds = vec!["rust".to_string()];
        let overrides: Vec<(String, String)> = Vec::new();
        let no_cache = false;
        let verbose = true;
        let sid = aifo_coder::toolchain_start_session(&kinds, &overrides, no_cache, verbose)
            .expect("start rust sidecar");
        let (url, token, flag, handle) =
            aifo_coder::toolexec_start_proxy(&sid, true).expect("start proxy");
        let port = port_from_url(&url);
        // Request "ls" which is not present in allowlists
        let (status, headers, body) = http_post_tcp(
            port,
            &[
                ("Authorization", &format!("Bearer {}", token)),
                ("X-Aifo-Proto", "1"),
            ],
            &[("tool", "ls"), ("cwd", ".")],
        );
        assert_eq!(
            status, 403,
            "expected 403, got {status}\nheaders:\n{headers}"
        );
        assert!(
            headers.to_ascii_lowercase().contains("x-exit-code: 86"),
            "expected X-Exit-Code: 86 in headers:\n{}",
            headers
        );
        assert_eq!(String::from_utf8_lossy(&body), "forbidden\n");
        flag.store(false, std::sync::atomic::Ordering::SeqCst);
        let _ = handle.join();
        aifo_coder::toolchain_cleanup_session(&sid, verbose);
    }

    // 4) 409 Conflict (dev-tool not available in any running sidecar)
    {
        std::env::remove_var("AIFO_TOOLEEXEC_USE_UNIX");
        let sid = "ut-err-409";
        let (url, token, flag, handle) =
            aifo_coder::toolexec_start_proxy(sid, true).expect("start proxy");
        let port = port_from_url(&url);
        let (status, _headers, body) = http_post_tcp(
            port,
            &[
                ("Authorization", &format!("Bearer {}", token)),
                ("X-Aifo-Proto", "1"),
            ],
            &[("tool", "make"), ("cwd", ".")],
        );
        assert_eq!(status, 409, "expected 409, got {status}");
        let body_s = String::from_utf8_lossy(&body);
        assert!(
            body_s.contains("tool 'make' not available")
                || body_s.contains("start an appropriate toolchain"),
            "expected helpful guidance in 409 body, got:\n{}",
            body_s
        );
        flag.store(false, std::sync::atomic::Ordering::SeqCst);
        let _ = handle.join();
    }

    // 5) 504 Gateway Timeout (child execution exceeded timeout)
    {
        std::env::remove_var("AIFO_TOOLEEXEC_USE_UNIX");
        std::env::set_var("AIFO_TOOLEEXEC_TIMEOUT_SECS", "1");
        let kinds = vec!["node".to_string()];
        let overrides: Vec<(String, String)> = Vec::new();
        let no_cache = false;
        let verbose = true;
        let sid = aifo_coder::toolchain_start_session(&kinds, &overrides, no_cache, verbose)
            .expect("start node sidecar");
        let (url, token, flag, handle) =
            aifo_coder::toolexec_start_proxy(&sid, true).expect("start proxy");
        let port = port_from_url(&url);
        // Use protocol v1 (buffered) to trigger recv_timeout. Run node for ~2 seconds.
        let (status, headers, _body) = http_post_tcp(
            port,
            &[
                ("Authorization", &format!("Bearer {}", token)),
                ("X-Aifo-Proto", "1"),
            ],
            &[
                ("tool", "node"),
                ("cwd", "."),
                ("arg", "-e"),
                ("arg", "setTimeout(()=>{},2000)"),
            ],
        );
        assert_eq!(
            status, 504,
            "expected 504, got {status}\nheaders:\n{headers}"
        );
        assert!(
            headers.to_ascii_lowercase().contains("x-exit-code: 124"),
            "expected X-Exit-Code: 124 in headers:\n{}",
            headers
        );
        flag.store(false, std::sync::atomic::Ordering::SeqCst);
        let _ = handle.join();
        aifo_coder::toolchain_cleanup_session(&sid, verbose);
        std::env::remove_var("AIFO_TOOLEEXEC_TIMEOUT_SECS");
    }
}
