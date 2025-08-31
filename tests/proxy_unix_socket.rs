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

    let (url, token, flag, handle) = aifo_coder::toolexec_start_proxy(&sid, verbose)
        .expect("failed to start proxy");

    // Ensure we received a unix URL for the agent
    assert!(url.starts_with("unix://"), "expected unix URL, got: {url}");

    // Host-side unix socket directory should be exported via env by the proxy starter
    let sock_dir = std::env::var("AIFO_TOOLEEXEC_UNIX_DIR").expect("AIFO_TOOLEEXEC_UNIX_DIR not set");
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

    // rust: cargo --version
    let (code_rust, _out_rust) = post_exec_unix(&sock_path, &token, "cargo", &["--version"]);
    assert_eq!(code_rust, 0, "cargo --version failed via unix proxy");

    // node: npx --version
    let (code_node, _out_node) = post_exec_unix(&sock_path, &token, "npx", &["--version"]);
    assert_eq!(code_node, 0, "npx --version failed via unix proxy");

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
