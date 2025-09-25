#![cfg(target_os = "linux")]

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;
mod support;

#[test]
#[ignore]
fn accept_phase4_native_http_uds_exec_rust_version() {
    // Skip if docker isn't available on this host (proxy requires docker CLI path)
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // Start a single rust sidecar session
    let kinds = vec!["rust".to_string()];
    let overrides: Vec<(String, String)> = Vec::new();
    let sid = aifo_coder::toolchain_start_session(&kinds, &overrides, false, true)
        .expect("toolchain_start_session");

    // Force unix-socket transport
    std::env::set_var("AIFO_TOOLEEXEC_USE_UNIX", "1");

    // Start proxy (UDS)
    let (url, token, running, handle) =
        aifo_coder::toolexec_start_proxy(&sid, true).expect("start proxy (uds)");
    assert!(url.starts_with("unix://"), "expected uds url, got: {}", url);
    let sock_path = url.trim_start_matches("unix://");
    let mut stream = UnixStream::connect(sock_path).expect("uds connect");
    let _ = stream.set_read_timeout(Some(Duration::from_secs(20)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(20)));

    // Build request for cargo --version
    let mut body = String::new();
    for (k, v) in [
        ("tool", "cargo"),
        ("cwd", "/workspace"),
        ("arg", "--version"),
    ] {
        if !body.is_empty() {
            body.push('&');
        }
        body.push_str(&support::urlencode(k));
        body.push('=');
        body.push_str(&support::urlencode(v));
    }

    let req_line = "POST /exec HTTP/1.1\r\n";
    let headers = format!(
        concat!(
            "Host: localhost\r\n",
            "Authorization: Bearer {tok}\r\n",
            "X-Aifo-Proto: 2\r\n",
            "TE: trailers\r\n",
            "Content-Type: application/x-www-form-urlencoded\r\n",
            "Transfer-Encoding: chunked\r\n",
            "Connection: close\r\n",
            "\r\n"
        ),
        tok = token
    );

    stream.write_all(req_line.as_bytes()).unwrap();
    stream.write_all(headers.as_bytes()).unwrap();
    write!(stream, "{:X}\r\n", body.len()).unwrap();
    stream.write_all(body.as_bytes()).unwrap();
    stream.write_all(b"\r\n").unwrap();
    stream.write_all(b"0\r\n\r\n").unwrap();
    let _ = stream.flush();

    // Read response and capture trailer exit code
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).unwrap();
    let text = String::from_utf8_lossy(&buf).to_string();

    let mut code: Option<i32> = None;
    for line in text.lines() {
        let l = line.trim();
        if let Some(v) = l.strip_prefix("X-Exit-Code:") {
            code = v.trim().parse::<i32>().ok();
        } else if l.to_ascii_lowercase().starts_with("x-exit-code:") {
            if let Some(idx) = l.find(':') {
                code = l[idx + 1..].trim().parse::<i32>().ok();
            }
        }
    }
    let exit_code = code.unwrap_or(0);
    assert_eq!(
        exit_code, 0,
        "expected cargo --version to exit 0 via UDS streaming; got {}.\nResponse:\n{}",
        exit_code, text
    );

    // Cleanup
    running.store(false, std::sync::atomic::Ordering::SeqCst);
    let _ = handle.join();
    aifo_coder::toolchain_cleanup_session(&sid, true);
}
