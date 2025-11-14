#![cfg_attr(not(test), allow(dead_code))]

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;
mod support;
#[ignore]
#[test]
fn accept_phase4_stream_large_output_node() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // Start node sidecar session
    let kinds = vec!["node".to_string()];
    let overrides: Vec<(String, String)> = Vec::new();
    let sid = aifo_coder::toolchain_start_session(&kinds, &overrides, false, true)
        .expect("toolchain_start_session");

    // Start proxy (TCP)
    let (url, token, running, handle) =
        aifo_coder::toolexec_start_proxy(&sid, true).expect("start proxy (tcp)");

    // Build HTTP/1.1 chunked request to run node -e 'process.stdout.write("x".repeat(N))'
    let rest = url.trim_start_matches("http://").to_string();
    let path_idx = rest.find('/').unwrap_or(rest.len());
    let (host_port, path) = rest.split_at(path_idx);
    let (host, port) = if let Some((h, p)) = host_port.split_once(':') {
        let pn = p.parse::<u16>().unwrap_or(80);
        (h.to_string(), pn)
    } else {
        (host_port.to_string(), 80u16)
    };
    let req_path = if path.is_empty() { "/exec" } else { path };

    let mut stream = TcpStream::connect((host.as_str(), port)).expect("connect tcp");
    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(30)));

    let n: usize = 262_144; // 256 KiB
    let code = format!(
        "process.stdout.write(String.fromCharCode(120).repeat({}));",
        n
    );
    let body_pairs = [
        ("tool".to_string(), "node".to_string()),
        ("cwd".to_string(), "/workspace".to_string()),
        ("arg".to_string(), "-e".to_string()),
        ("arg".to_string(), code),
    ];
    let mut body = String::new();
    for (i, (k, v)) in body_pairs.iter().enumerate() {
        if i > 0 {
            body.push('&');
        }
        body.push_str(&support::urlencode(k));
        body.push('=');
        body.push_str(&support::urlencode(v));
    }

    let req_line = format!("POST {} HTTP/1.1\r\n", req_path);
    let headers = format!(
        concat!(
            "Host: {host}\r\n",
            "Authorization: Bearer {tok}\r\n",
            "X-Aifo-Proto: 2\r\n",
            "TE: trailers\r\n",
            "Content-Type: application/x-www-form-urlencoded\r\n",
            "Transfer-Encoding: chunked\r\n",
            "Connection: close\r\n",
            "\r\n"
        ),
        host = host,
        tok = token
    );

    stream.write_all(req_line.as_bytes()).unwrap();
    stream.write_all(headers.as_bytes()).unwrap();
    // Chunked body
    write!(stream, "{:X}\r\n", body.len()).unwrap();
    stream.write_all(body.as_bytes()).unwrap();
    stream.write_all(b"\r\n").unwrap();
    stream.write_all(b"0\r\n\r\n").unwrap();
    let _ = stream.flush();

    // Read response and count 'x' bytes in body
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).unwrap();
    let text = String::from_utf8_lossy(&buf).to_string();

    // Trailer/header exit code
    let mut code_hdr: Option<i32> = None;
    for line in text.lines() {
        let l = line.trim();
        if let Some(v) = l.strip_prefix("X-Exit-Code:") {
            code_hdr = v.trim().parse::<i32>().ok();
        } else if l.to_ascii_lowercase().starts_with("x-exit-code:") {
            if let Some(idx) = l.find(':') {
                code_hdr = l[idx + 1..].trim().parse::<i32>().ok();
            }
        }
    }
    let exit_code = code_hdr.unwrap_or(0);
    assert_eq!(
        exit_code, 0,
        "expected node -e to exit 0 via proxy streaming; got {}.\nResponse:\n{}",
        exit_code, text
    );
    // Count 'x' characters in the whole response and expect at least N
    let count_x = text.as_bytes().iter().filter(|b| **b == b'x').count();
    assert!(
        count_x >= n,
        "expected at least {} 'x' bytes in streamed output, got {}",
        n,
        count_x
    );

    // Cleanup
    running.store(false, std::sync::atomic::Ordering::SeqCst);
    let _ = handle.join();
    aifo_coder::toolchain_cleanup_session(&sid, true);
}
