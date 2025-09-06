use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::Command;

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
    let u = url.strip_prefix("http://").expect("only http:// supported");
    let mut parts = u.splitn(2, '/');
    let hostport = parts.next().unwrap_or_default();
    let path_rest = parts.next().unwrap_or_default();
    let mut hp = hostport.rsplitn(2, ':');
    let port_str = hp.next().unwrap_or("80");
    let host = hp.next().unwrap_or(hostport);
    let port: u16 = port_str.parse().expect("port parse");
    let path = format!("/{}", path_rest);
    let connect_host = if host == "host.docker.internal" {
        "127.0.0.1"
    } else {
        host
    };
    let mut stream = TcpStream::connect((connect_host, port)).expect("connect failed");

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
        "POST {} HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {}\r\nX-Aifo-Proto: 1\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        path, host, token, body.len(), body
    );
    stream.write_all(req.as_bytes()).expect("write");
    let mut buf = Vec::new();
    // removed unused buffer
    stream.read_to_end(&mut buf).expect("read");
    // parse headers
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
        let out =
            String::from_utf8_lossy(&body_bytes[..std::cmp::min(content_len, body_bytes.len())])
                .to_string();
        (exit_code, out)
    } else {
        (exit_code, String::new())
    }
}

#[test]
#[ignore]
fn test_go_env_vars_inside_sidecar() {
    if !docker_present() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }
    let img = "golang:1.22-bookworm";
    if !image_present(img) {
        eprintln!("skipping: image {} not present locally", img);
        return;
    }

    let kinds = vec!["go".to_string()];
    let overrides: Vec<(String, String)> = Vec::new();

    let sid = aifo_coder::toolchain_start_session(&kinds, &overrides, false, true)
        .expect("start session");
    let (url, token, flag, handle) =
        aifo_coder::toolexec_start_proxy(&sid, true).expect("start proxy");

    let (code, out) = post_exec(
        &url,
        &token,
        "go",
        &["env", "GOPATH", "GOMODCACHE", "GOCACHE"],
    );
    assert_eq!(code, 0, "go env failed: {}", out);
    assert!(
        out.contains("/go"),
        "expected GOPATH=/go in output: {}",
        out
    );
    assert!(
        out.contains("/go/pkg/mod"),
        "expected GOMODCACHE=/go/pkg/mod in output: {}",
        out
    );
    assert!(
        out.contains("/go/build-cache"),
        "expected GOCACHE=/go/build-cache in output: {}",
        out
    );

    flag.store(false, std::sync::atomic::Ordering::SeqCst);
    let _ = handle.join();
    aifo_coder::toolchain_cleanup_session(&sid, true);
}
