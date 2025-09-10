#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
#[ignore]
#[test]
fn test_tsc_local_resolution_tcp_v2() {
    use std::fs;
    use std::io::Write;

    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // Create temp workspace with a fake local ./node_modules/.bin/tsc that prints a marker
    let td = tempfile::tempdir().expect("tmpdir");
    let pwd = td.path().to_path_buf();
    let nm_bin = pwd.join("node_modules").join(".bin");
    fs::create_dir_all(&nm_bin).expect("mkdir -p node_modules/.bin");
    let local_tsc = nm_bin.join("tsc");

    #[cfg(unix)]
    {
        let mut f = fs::File::create(&local_tsc).expect("create local tsc");
        f.write_all(b"#!/bin/sh\necho local-tsc\nexit 0\n")
            .expect("write tsc");
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&local_tsc, fs::Permissions::from_mode(0o755)).expect("chmod +x");
    }
    #[cfg(not(unix))]
    {
        // On non-Unix, create a .cmd wrapper if needed; here we just skip as CI focuses on Unix runners
        eprintln!("skipping on non-Unix host for simplicity");
        return;
    }

    // chdir into workspace so sidecars mount this directory at /workspace
    std::env::set_current_dir(&pwd).expect("chdir");

    // Force TCP (disable unix mode)
    std::env::remove_var("AIFO_TOOLEEXEC_USE_UNIX");

    // Start only node sidecar (tsc resolves locally via ./node_modules/.bin/tsc)
    let kinds = vec!["node".to_string()];
    let overrides: Vec<(String, String)> = Vec::new();
    let no_cache = true;
    let verbose = true;

    let sid = aifo_coder::toolchain_start_session(&kinds, &overrides, no_cache, verbose)
        .expect("failed to start node sidecar session");
    let (url, token, flag, handle) =
        aifo_coder::toolexec_start_proxy(&sid, verbose).expect("failed to start proxy");

    // Extract port from URL; connect to localhost:<port> from the host
    let port: u16 = {
        assert!(
            url.starts_with("http://"),
            "expected http:// URL, got: {url}"
        );
        let without_proto = url.trim_start_matches("http://");
        let host_port = without_proto.split('/').next().unwrap_or(without_proto);
        host_port
            .rsplitn(2, ':')
            .next()
            .and_then(|s| s.parse::<u16>().ok())
            .expect("failed to parse port from URL")
    };

    // Minimal HTTP v2 client to run tool=tsc and read chunked body and trailers
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpStream;

    let mut stream =
        TcpStream::connect(("127.0.0.1", port)).expect("connect 127.0.0.1:<port> failed");

    let mut body = format!(
        "tool={}&cwd={}",
        urlencoding::Encoded::new("tsc"),
        urlencoding::Encoded::new(".")
    );

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

    // Read chunked body; assemble into a string, then parse trailers for exit code
    let mut body_out = Vec::new();
    loop {
        let mut size_line = String::new();
        reader
            .read_line(&mut size_line)
            .expect("read chunk size failed");
        if size_line.is_empty() {
            break;
        }
        let size_str = size_line.trim();
        let size_only = size_str.split(';').next().unwrap_or(size_str);
        let Ok(sz) = usize::from_str_radix(size_only, 16) else {
            break;
        };
        if sz == 0 {
            break;
        }
        let mut chunk = vec![0u8; sz];
        reader
            .read_exact(&mut chunk)
            .expect("read chunk data failed");
        body_out.extend_from_slice(&chunk);
        let mut crlf = [0u8; 2];
        reader.read_exact(&mut crlf).expect("read CRLF failed");
    }

    // Read trailers; extract X-Exit-Code
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
    assert!(
        text.contains("local-tsc"),
        "expected local-tsc marker from ./node_modules/.bin/tsc, got:\n{}",
        text
    );
    assert_eq!(code, 0, "expected exit code 0");

    // Cleanup
    flag.store(false, std::sync::atomic::Ordering::SeqCst);
    let _ = handle.join();
    aifo_coder::toolchain_cleanup_session(&sid, verbose);
}
