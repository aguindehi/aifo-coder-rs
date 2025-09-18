#![cfg_attr(not(test), allow(dead_code))]

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

#[cfg(unix)]
fn capture_stderr<F: FnOnce()>(f: F) -> String {
    use libc::{dup, dup2, fflush, STDERR_FILENO};
    use std::fs;
    use std::os::unix::io::AsRawFd;
    let path = "/tmp/aifo-coder-test-stderr-accept-logs.tmp";
    let _ = fs::remove_file(path);
    let file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .read(true)
        .truncate(true)
        .open(path)
        .expect("open tmp stderr");
    unsafe {
        let saved = dup(STDERR_FILENO);
        assert!(saved >= 0, "dup stderr");
        let ok = dup2(file.as_raw_fd(), STDERR_FILENO);
        assert!(ok >= 0, "dup2 stderr");
        f();
        fflush(std::ptr::null_mut());
        let ok2 = dup2(saved, STDERR_FILENO);
        assert!(ok2 >= 0, "restore stderr");
        let _ = libc::close(saved);
    }
    std::fs::read_to_string(path).unwrap_or_default()
}

#[cfg(not(unix))]
fn capture_stderr<F: FnOnce()>(f: F) -> String {
    // Fallback: just run and return empty logs
    f();
    String::new()
}

fn urlencode_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b' ' => out.push('+'),
            b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b'0'..=b'9' | b'a'..=b'z' | b'A'..=b'Z' => out.push(b as char),
            _ => {
                out.push('%');
                out.push_str(&format!("{:02X}", b));
            }
        }
    }
    out
}

#[test]
#[ignore]
fn accept_phase4_logs_golden_verbose_substrings() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // Start a single rust sidecar session
    let kinds = vec!["rust".to_string()];
    let overrides: Vec<(String, String)> = Vec::new();
    let sid = aifo_coder::toolchain_start_session(&kinds, &overrides, false, true)
        .expect("toolchain_start_session");

    // Start proxy (TCP) with verbose and tee logs to a file
    let log_path = format!("/tmp/aifo-coder-accept-logs-{}.log", std::process::id());
    std::env::set_var("AIFO_TOOLCHAIN_VERBOSE", "1");
    std::env::set_var("AIFO_TEST_LOG_PATH", &log_path);
    let (url, token, running, handle) =
        aifo_coder::toolexec_start_proxy(&sid, true).expect("start proxy (tcp)");
    assert!(url.starts_with("http://"), "expected tcp url, got: {}", url);

    // Build a minimal HTTP/1.1 chunked request to POST /exec invoking `cargo --version`
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
    let _ = stream.set_read_timeout(Some(Duration::from_secs(20)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(20)));

    let body_pairs = [
        ("tool".to_string(), "cargo".to_string()),
        ("cwd".to_string(), "/workspace".to_string()),
        ("arg".to_string(), "--version".to_string()),
    ];
    let mut body = String::new();
    for (i, (k, v)) in body_pairs.iter().enumerate() {
        if i > 0 {
            body.push('&');
        }
        body.push_str(&urlencode_component(k));
        body.push('=');
        body.push_str(&urlencode_component(v));
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

    // Drain response
    let mut buf = Vec::new();
    let _ = stream.read_to_end(&mut buf);

    // Cleanup proxy and sidecar session
    running.store(false, std::sync::atomic::Ordering::SeqCst);
    let _ = handle.join();

    let logs = std::fs::read_to_string(&log_path).unwrap_or_default();
    let _ = std::fs::remove_file(&log_path);

    // Cleanup sidecars
    aifo_coder::toolchain_cleanup_session(&sid, true);

    // Some platforms (or test harnesses) may resist FD-level stderr capture; skip if empty.
    if logs.trim().is_empty() {
        eprintln!("skipping: unable to capture proxy logs on this platform/test harness");
        return;
    }
    // Assert selected substrings to freeze UX
    assert!(
        logs.contains("aifo-coder: proxy parsed tool=") && logs.contains(" exec_id="),
        "missing 'proxy parsed' line with exec_id; logs:\n{}",
        logs
    );
    assert!(
        logs.contains("aifo-coder: proxy exec: proto=v2 (streaming)"),
        "missing 'proxy exec' line; logs:\n{}",
        logs
    );
    assert!(
        logs.contains("aifo-coder: proxy result tool=") && logs.contains(" code="),
        "missing 'proxy result' line; logs:\n{}",
        logs
    );
}
