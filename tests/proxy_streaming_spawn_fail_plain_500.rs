#![cfg(unix)]

use std::fs;
use std::io::{Read, Write};

mod support;

/// This test verifies that the streaming path (proto v2) returns a plain 500 (no chunked prelude)
/// when the spawn of the docker runtime fails, and that X-Exit-Code is 86 as per spec.
///
/// Preconditions (manual/CI environment specific):
/// - A suitable sidecar (e.g., python) must be running for the session so that container_exists
///   would normally pass. We start one here.
/// - The proxy must capture a broken docker runtime path at startup to force spawn failure.
///   We achieve this by prepending a non-executable "docker" stub directory to PATH before
///   starting the proxy. Note: this likely causes container_exists() to fail early in some
///   environments if it uses the same runtime path; in that case the test may yield 409 and
///   should be considered skipped/unmet preconditions.
///
/// The test is marked #[ignore] by default because it depends on host docker and PATH behavior.
/// Run it explicitly with:
///   cargo test --test proxy_streaming_spawn_fail_plain_500 -- --ignored --nocapture
#[ignore]
#[test]
fn test_streaming_spawn_fail_plain_500() {
    // Skip if docker isn't available on this host (for sidecar start)
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // Start python sidecar so container_exists would normally succeed
    let kinds = vec!["python".to_string()];
    let overrides: Vec<(String, String)> = Vec::new();
    let sid = aifo_coder::toolchain_start_session(&kinds, &overrides, false, true)
        .expect("failed to start sidecar session");

    // Create a directory with a non-executable 'docker' to force spawn failure
    let td = tempfile::tempdir().expect("tmpdir");
    let badbin = td.path().join("badbin");
    fs::create_dir_all(&badbin).unwrap();
    let fake_docker = badbin.join("docker");
    fs::write(&fake_docker, b"not-executable").unwrap();
    // Do not set executable bit to ensure spawn() fails with EACCES
    // Prepend badbin to PATH before starting the proxy so it captures the broken runtime
    let old_path = std::env::var("PATH").ok();
    let new_path = format!(
        "{}:{}",
        badbin.display(),
        old_path.clone().unwrap_or_default()
    );
    std::env::set_var("PATH", &new_path);

    // Start proxy (captures broken docker path)
    let (url, token, flag, handle) =
        aifo_coder::toolexec_start_proxy(&sid, true).expect("failed to start proxy");

    // Restore PATH for the rest of the test (container_exists inside proxy may already be impacted)
    if let Some(v) = old_path {
        std::env::set_var("PATH", v);
    }

    fn extract_port(u: &str) -> u16 {
        support::port_from_http_url(u)
    }
    let port = extract_port(&url);

    // Issue a streaming exec request; if spawn fails properly, we should get plain 500 with X-Exit-Code: 86
    use std::net::TcpStream;
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect failed");
    let body = "tool=python&cwd=.&arg=-c&arg=print(123)";
    let req = format!(
        "POST /exec HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {}\r\nX-Aifo-Proto: 2\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n\r\n{}",
        token, body.len(), body
    );
    stream.write_all(req.as_bytes()).expect("write failed");

    let mut resp = Vec::new();
    stream.read_to_end(&mut resp).ok();
    let text = String::from_utf8_lossy(&resp).to_string();

    // If preconditions didn't hold, some environments may return 409 (container not found)
    if text.contains("409 Conflict") {
        eprintln!(
            "preconditions unmet: container not found; response:\n{}",
            text
        );
        // Cleanup and early return
        flag.store(false, std::sync::atomic::Ordering::SeqCst);
        let _ = handle.join();
        aifo_coder::toolchain_cleanup_session(&sid, true);
        return;
    }

    // If the proxy executed successfully (200 OK), skip as preconditions didn't force spawn failure.
    if text.contains("200 OK") && text.contains("X-Exit-Code: 0") {
        eprintln!(
            "spawn did not fail as intended; environment did not reproduce spawn-failure; response:\n{}",
            text
        );
        // Cleanup and early return
        flag.store(false, std::sync::atomic::Ordering::SeqCst);
        let _ = handle.join();
        aifo_coder::toolchain_cleanup_session(&sid, true);
        return;
    }

    // Assert plain 500 (no Transfer-Encoding: chunked)
    assert!(
        text.contains("500 Internal Server Error"),
        "expected plain 500; got:\n{}",
        text
    );
    assert!(
        !text.contains("Transfer-Encoding: chunked"),
        "must not send chunked prelude on spawn error; got:\n{}",
        text
    );
    assert!(
        text.contains("X-Exit-Code: 86"),
        "expected X-Exit-Code: 86 on proxy spawn error; got:\n{}",
        text
    );

    // Cleanup
    flag.store(false, std::sync::atomic::Ordering::SeqCst);
    let _ = handle.join();
    aifo_coder::toolchain_cleanup_session(&sid, true);
}
