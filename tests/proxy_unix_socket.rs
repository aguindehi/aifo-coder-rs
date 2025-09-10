#[cfg(target_os = "linux")]
#[ignore]
#[test]
fn test_proxy_unix_socket_rust_and_node() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // Enable unix socket transport for the proxy
    std::env::set_var("AIFO_TOOLEEXEC_USE_UNIX", "1");

    // Start sidecars for rust and node and the proxy
    let kinds = vec!["rust".to_string(), "node".to_string()];
    let overrides: Vec<(String, String)> = Vec::new();
    let no_cache = false;
    let verbose = true;

    let sid = aifo_coder::toolchain_start_session(&kinds, &overrides, no_cache, verbose)
        .expect("failed to start toolchain session");

    let (url, token, flag, handle) =
        aifo_coder::toolexec_start_proxy(&sid, verbose).expect("failed to start proxy");

    // Ensure we received a unix URL for the agent
    assert!(url.starts_with("unix://"), "expected unix URL, got: {url}");

    // Host-side unix socket directory should be exported via env by the proxy starter
    let sock_dir =
        std::env::var("AIFO_TOOLEEXEC_UNIX_DIR").expect("AIFO_TOOLEEXEC_UNIX_DIR not set");
    let sock_path = format!("{}/toolexec.sock", sock_dir);

    fn post_exec_unix(sock: &str, token: &str, tool: &str, args: &[&str]) -> (i32, String) {
        use std::io::{Read, Write};
        use std::os::unix::net::UnixStream;

        let mut stream = UnixStream::connect(sock).expect("connect unix socket failed");

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
            "POST /exec HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {}\r\nX-Aifo-Proto: 1\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
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

    fn post_exec_unix_v2(sock: &str, token: &str, tool: &str, args: &[&str]) -> (i32, String) {
        use std::io::{BufRead, BufReader, Read, Write};
        use std::os::unix::net::UnixStream;

        let mut stream = UnixStream::connect(sock).expect("connect unix socket failed");

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
            "POST /exec HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {}\r\nX-Aifo-Proto: 2\r\nTE: trailers\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
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
            // Trim CRLF
            let size_str = size_line.trim();
            // Some proxies may emit optional chunk extensions after ';'
            let size_only = size_str
                .split(';')
                .next()
                .unwrap_or(size_str)
                .trim_start_matches("0x");
            let Ok(mut sz) = usize::from_str_radix(size_only, 16) else {
                // Malformed size; bail out
                break;
            };
            if sz == 0 {
                // consume trailing CRLF after the 0-size chunk size line has already been read
                // Next lines will contain trailers terminated by an empty line
                break;
            }
            // Read exactly sz bytes of data
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
    let (code_rust, _out_rust) = post_exec_unix(&sock_path, &token, "cargo", &["--version"]);
    assert_eq!(code_rust, 0, "cargo --version failed via unix proxy");

    // node: npx --version
    let (code_node, _out_node) = post_exec_unix(&sock_path, &token, "npx", &["--version"]);
    assert_eq!(code_node, 0, "npx --version failed via unix proxy");

    // v2 streaming: rust cargo --version
    let (code_rust_v2, _out_rust_v2) =
        post_exec_unix_v2(&sock_path, &token, "cargo", &["--version"]);
    assert_eq!(
        code_rust_v2, 0,
        "cargo --version failed via unix proxy (v2)"
    );

    // v2 streaming: node npx --version
    let (code_node_v2, _out_node_v2) = post_exec_unix_v2(&sock_path, &token, "npx", &["--version"]);
    assert_eq!(code_node_v2, 0, "npx --version failed via unix proxy (v2)");

    // Cleanup
    flag.store(false, std::sync::atomic::Ordering::SeqCst);
    let _ = handle.join();
    aifo_coder::toolchain_cleanup_session(&sid, verbose);

    // After cleanup, the unix socket directory should be removed by toolchain_cleanup_session
    let dir_path = std::path::Path::new(&sock_dir);
    assert!(
        !dir_path.exists(),
        "expected unix socket dir to be removed after cleanup: {}",
        sock_dir
    );
}
