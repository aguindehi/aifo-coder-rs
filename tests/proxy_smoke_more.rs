use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::Command;
mod support;
use support::urlencode;

fn docker_present() -> bool {
    aifo_coder::container_runtime_path().is_ok()
}

fn image_present(img: &str) -> bool {
    if let Ok(rt) = aifo_coder::container_runtime_path() {
        return Command::new(rt)
            .args(["image", "inspect", img])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
    }
    false
}

fn post_exec(url: &str, token: &str, tool: &str, args: &[&str]) -> (i32, String) {
    // Very small HTTP client for http://host:port/path
    let u = url
        .strip_prefix("http://")
        .expect("only http:// supported in tests");
    let mut parts = u.splitn(2, '/');
    let hostport = parts.next().unwrap_or_default();
    let path_rest = parts.next().unwrap_or_default();
    let mut hp = hostport.rsplitn(2, ':');
    let port_str = hp.next().unwrap_or("80");
    let host = hp.next().unwrap_or(hostport);
    let port: u16 = port_str.parse().expect("port parse");
    let path = format!("/{}", path_rest);

    // Some hosts (e.g., macOS test runners) may not resolve host.docker.internal; connect to loopback in that case.
    let connect_host = if host == "host.docker.internal" {
        "127.0.0.1"
    } else {
        host
    };
    let mut stream = TcpStream::connect((connect_host, port)).expect("connect failed");

    let mut body = format!("tool={}&cwd={}", urlencode(tool), urlencode("."));
    for a in args {
        body.push('&');
        body.push_str(&format!("arg={}", urlencode(a)));
    }

    let req = format!(
        "POST {} HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {}\r\nX-Aifo-Proto: 1\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        path, host, token, body.len(), body
    );
    stream.write_all(req.as_bytes()).expect("write");
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).expect("read");

    // Parse headers
    let mut exit_code: i32 = 1;
    let mut content_len: usize = 0;
    if let Some(hend) = buf.windows(4).position(|w| w == b"\r\n\r\n").map(|p| p + 4) {
        let header = String::from_utf8_lossy(&buf[..hend]).to_string();
        for line in header.lines() {
            if let Some(v) = line.strip_prefix("X-Exit-Code: ") {
                exit_code = v.trim().parse::<i32>().unwrap_or(1);
            } else if let Some(v) = line.strip_prefix("Content-Length: ") {
                content_len = v.trim().parse::<usize>().unwrap_or(0);
            }
        }
        let body_bytes = &buf[hend..];
        let mut s = String::new();
        s.push_str(&String::from_utf8_lossy(
            &body_bytes[..std::cmp::min(content_len, body_bytes.len())],
        ));
        (exit_code, s)
    } else {
        (exit_code, String::new())
    }
}

#[test]
fn test_proxy_smoke_python() {
    if !docker_present() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }
    let img = "python:3.12-slim";
    if !image_present(img) {
        eprintln!("skipping: image {} not present locally", img);
        return;
    }

    // Linux: ensure host connectivity for sidecars
    #[cfg(target_os = "linux")]
    std::env::set_var("AIFO_TOOLEEXEC_ADD_HOST", "1");

    let kinds = vec!["python".to_string()];
    let overrides: Vec<(String, String)> = Vec::new();

    let sid = aifo_coder::toolchain_start_session(&kinds, &overrides, false, true)
        .expect("start session");
    let (url, token, flag, handle) =
        aifo_coder::toolexec_start_proxy(&sid, true).expect("start proxy");

    let (code, out) = post_exec(&url, &token, "python", &["--version"]);
    assert_eq!(code, 0, "python --version failed: {}", out);
    assert!(
        out.to_lowercase().contains("python"),
        "unexpected python version output: {}",
        out
    );

    // Cleanup
    flag.store(false, std::sync::atomic::Ordering::SeqCst);
    let _ = handle.join();
    aifo_coder::toolchain_cleanup_session(&sid, true);
}

#[test]
fn test_proxy_smoke_c_cpp() {
    if !docker_present() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }
    let img = "aifo-cpp-toolchain:latest";
    if !image_present(img) {
        eprintln!("skipping: image {} not present locally", img);
        return;
    }

    #[cfg(target_os = "linux")]
    std::env::set_var("AIFO_TOOLEEXEC_ADD_HOST", "1");

    let kinds = vec!["c-cpp".to_string()];
    let overrides: Vec<(String, String)> = Vec::new();

    let sid = aifo_coder::toolchain_start_session(&kinds, &overrides, false, true)
        .expect("start session");
    let (url, token, flag, handle) =
        aifo_coder::toolexec_start_proxy(&sid, true).expect("start proxy");

    let (code, out) = post_exec(&url, &token, "cmake", &["--version"]);
    assert_eq!(code, 0, "cmake --version failed: {}", out);
    assert!(
        out.to_lowercase().contains("cmake version"),
        "unexpected cmake version output: {}",
        out
    );

    flag.store(false, std::sync::atomic::Ordering::SeqCst);
    let _ = handle.join();
    aifo_coder::toolchain_cleanup_session(&sid, true);
}

#[test]
fn test_proxy_smoke_go() {
    if !docker_present() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }
    let img = "golang:1.22-bookworm";
    if !image_present(img) {
        eprintln!("skipping: image {} not present locally", img);
        return;
    }

    #[cfg(target_os = "linux")]
    std::env::set_var("AIFO_TOOLEEXEC_ADD_HOST", "1");

    let kinds = vec!["go".to_string()];
    let overrides: Vec<(String, String)> = Vec::new();

    let sid = aifo_coder::toolchain_start_session(&kinds, &overrides, false, true)
        .expect("start session");
    let (url, token, flag, handle) =
        aifo_coder::toolexec_start_proxy(&sid, true).expect("start proxy");

    let (code, out) = post_exec(&url, &token, "go", &["version"]);
    assert_eq!(code, 0, "go version failed: {}", out);
    assert!(
        out.to_lowercase().contains("go version"),
        "unexpected go version output: {}",
        out
    );

    flag.store(false, std::sync::atomic::Ordering::SeqCst);
    let _ = handle.join();
    aifo_coder::toolchain_cleanup_session(&sid, true);
}
