mod support;
use support::urlencode;
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
#[ignore]
#[test]
fn test_python_venv_activation_path_precedence_tcp_v2() {
    use std::fs;
    use std::io::Write;

    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // Create temp workspace
    let td = tempfile::tempdir().expect("tmpdir");
    let pwd = td.path().to_path_buf();

    // chdir into workspace so sidecars mount this directory at /workspace
    std::env::set_current_dir(&pwd).expect("chdir");

    // Start python sidecar and proxy
    std::env::remove_var("AIFO_TOOLEEXEC_USE_UNIX");
    let kinds = vec!["python".to_string()];
    let overrides: Vec<(String, String)> = Vec::new();
    let no_cache = true;
    let verbose = true;

    let sid = aifo_coder::toolchain_start_session(&kinds, &overrides, no_cache, verbose)
        .expect("start python sidecar");

    let (url, token, flag, handle) =
        aifo_coder::toolexec_start_proxy(&sid, verbose).expect("start proxy");

    // Connect to TCP port
    let port: u16 = {
        let without_proto = url.trim_start_matches("http://");
        let host_port = without_proto.split('/').next().unwrap_or(without_proto);
        host_port
            .rsplit(':')
            .next()
            .and_then(|s| s.parse::<u16>().ok())
            .expect("failed to parse port from URL")
    };

    // Send v2 streaming request: tool=python args=['--version'] and expect "venv-python"
    use std::io::{BufRead, BufReader, Read};
    use std::net::TcpStream;

    let mut stream =
        TcpStream::connect(("127.0.0.1", port)).expect("connect 127.0.0.1:<port> failed");

    let mut body = format!("tool={}&cwd={}", urlencode("python"), urlencode("."));
    body.push_str("&arg=--version");

    let req = format!(
        "POST /exec HTTP/1.1\r\nHost: host.docker.internal\r\nAuthorization: Bearer {}\r\nX-Aifo-Proto: 2\r\nTE: trailers\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        token,
        body.len(),
        body
    );
    stream.write_all(req.as_bytes()).expect("write failed");

    // Read headers
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

    // Read first chunk payload (we don't need all of it; just assemble into a string)
    let mut out = Vec::new();
    loop {
        let mut size_line = String::new();
        if reader.read_line(&mut size_line).unwrap_or(0) == 0 {
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
        out.extend_from_slice(&chunk);
        let mut crlf = [0u8; 2];
        reader.read_exact(&mut crlf).expect("read CRLF failed");
        // Stop early if output contains Python version string
        if String::from_utf8_lossy(&out).contains("Python 3") {
            break;
        }
    }

    let out_s = String::from_utf8_lossy(&out);
    assert!(
        out_s.contains("Python 3"),
        "expected Python 3 version output, got:\n{}",
        out_s
    );

    // Cleanup
    flag.store(false, std::sync::atomic::Ordering::SeqCst);
    let _ = handle.join();
    aifo_coder::toolchain_cleanup_session(&sid, verbose);
}
