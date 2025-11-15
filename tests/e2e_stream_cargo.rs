use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

fn docker_image_present(runtime: &std::path::Path, image: &str) -> bool {
    std::process::Command::new(runtime)
        .args(["image", "inspect", image])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn parse_host_port_from_http_url(url: &str) -> Option<(String, u16, String)> {
    // url is http://HOST:PORT/path
    let rest = url.strip_prefix("http://")?.to_string();
    let mut parts = rest.splitn(2, '/');
    let host_port = parts.next().unwrap_or("");
    let path = format!("/{}", parts.next().unwrap_or("exec"));
    let mut hp = host_port.splitn(2, ':');
    let host = hp.next().unwrap_or("").to_string();
    let port = hp
        .next()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(80u16);
    Some((host, port, path))
}

fn read_all_with_timeout(stream: &mut TcpStream, max_ms: u64) -> Vec<u8> {
    let _ = stream.set_read_timeout(Some(Duration::from_millis(max_ms)));
    let mut buf = Vec::<u8>::new();
    let mut tmp = [0u8; 4096];
    loop {
        match stream.read(&mut tmp) {
            Ok(0) => break,
            Ok(n) => buf.extend_from_slice(&tmp[..n]),
            Err(ref e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                break;
            }
            Err(_) => break,
        }
    }
    buf
}

// Minimal chunked decoder that also captures X-Exit-Code trailer when present.
// Returns (body, exit_code_opt).
fn decode_chunked(data: Vec<u8>) -> (Vec<u8>, Option<i32>) {
    // Find header end (CRLFCRLF or LFLF)
    let hdr_end = if let Some(i) = data.windows(4).position(|w| w == b"\r\n\r\n") {
        i + 4
    } else if let Some(i) = data.windows(2).position(|w| w == b"\n\n") {
        i + 2
    } else {
        data.len()
    };
    let mut body = Vec::<u8>::new();
    let mut exit_code: Option<i32> = None;

    let mut cursor = hdr_end;
    loop {
        // read size line
        let mut line = Vec::new();
        while cursor < data.len() {
            let b = data[cursor];
            cursor += 1;
            if b == b'\r' {
                if cursor < data.len() && data[cursor] == b'\n' {
                    cursor += 1;
                }
                break;
            } else if b == b'\n' {
                break;
            } else {
                line.push(b);
            }
        }
        if line.is_empty() {
            if cursor >= data.len() {
                break;
            }
            continue;
        }
        let size_str = String::from_utf8_lossy(&line);
        let size_hex = size_str.split(';').next().unwrap_or(&size_str);
        let size = usize::from_str_radix(size_hex.trim(), 16).unwrap_or(0);
        if size == 0 {
            // Read trailers until blank
            loop {
                let mut tr = Vec::new();
                while cursor < data.len() {
                    let b = data[cursor];
                    cursor += 1;
                    if b == b'\r' {
                        if cursor < data.len() && data[cursor] == b'\n' {
                            cursor += 1;
                        }
                        break;
                    } else if b == b'\n' {
                        break;
                    } else {
                        tr.push(b);
                    }
                }
                if tr.is_empty() {
                    break;
                }
                let tline = String::from_utf8_lossy(&tr).to_string();
                let tlc = tline.to_ascii_lowercase();
                if tlc.starts_with("x-exit-code:") {
                    if let Some(idx) = tline.find(':') {
                        let v = &tline[idx + 1..];
                        if let Ok(n) = v.trim().parse::<i32>() {
                            exit_code = Some(n);
                        }
                    }
                }
            }
            break;
        }
        // copy payload
        let end = cursor.saturating_add(size).min(data.len());
        if cursor < end {
            body.extend_from_slice(&data[cursor..end]);
        }
        cursor = end;
        // consume chunk CRLF or LF
        if cursor < data.len() && data[cursor] == b'\r' {
            cursor += 1;
            if cursor < data.len() && data[cursor] == b'\n' {
                cursor += 1;
            }
        } else if cursor < data.len() && data[cursor] == b'\n' {
            cursor += 1;
        }
    }
    (body, exit_code)
}
#[ignore]
#[test]
fn e2e_stream_cargo_help_v2() {
    // Skip if docker isn't available
    let runtime = match aifo_coder::container_runtime_path() {
        Ok(p) => p,
        Err(_) => {
            eprintln!("skipping: docker not found in PATH");
            return;
        }
    };

    // Prefer MR/default-branch toolchain image when provided by CI; otherwise fall back to official.
    let have_mr_toolchain = std::env::var("AIFO_CODER_TEST_RUST_IMAGE")
        .ok()
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);
    if !have_mr_toolchain {
        // Force official rust image for the sidecar to avoid depending on unpublished toolchain images.
        std::env::set_var("AIFO_RUST_TOOLCHAIN_USE_OFFICIAL", "1");
        // Default version used by code is 1.80; ensure it's present locally to avoid pulling in tests.
        let official = "rust:1.80-bookworm";
        if !docker_image_present(&runtime, official) {
            eprintln!(
                "skipping: {} not present locally (avoid pulling in tests)",
                official
            );
            // Cleanup env
            std::env::remove_var("AIFO_RUST_TOOLCHAIN_USE_OFFICIAL");
            return;
        }
    }

    // Ensure we do not allocate a TTY for v2 streaming
    std::env::set_var("AIFO_TOOLEEXEC_TTY", "0");

    // Start sidecar(s) and proxy
    let kinds = vec!["rust".to_string()];

    // Prefer corporate CA inside the sidecar to allow rustup/curl TLS in restricted environments.
    // Source: AIFO_TEST_CORP_CA or $HOME/.certificates/MigrosRootCA2.crt
    let corp_ca_src = std::env::var("AIFO_TEST_CORP_CA")
        .ok()
        .filter(|p| std::path::Path::new(p).exists())
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|h| format!("{}/.certificates/MigrosRootCA2.crt", h))
                .filter(|p| std::path::Path::new(p).exists())
        });

    let mut overrides: Vec<(String, String)> = Vec::new();
    let mut ca_copied = false;
    if let Some(src) = corp_ca_src {
        // Copy CA into workspace so the sidecar can use it at /workspace/corp-ca.crt
        let _ = std::fs::copy(&src, "corp-ca.crt");
        if std::path::Path::new("corp-ca.crt").exists() {
            ca_copied = true;
            overrides.push((
                "SSL_CERT_FILE".to_string(),
                "/workspace/corp-ca.crt".to_string(),
            ));
            overrides.push((
                "CURL_CA_BUNDLE".to_string(),
                "/workspace/corp-ca.crt".to_string(),
            ));
            overrides.push((
                "REQUESTS_CA_BUNDLE".to_string(),
                "/workspace/corp-ca.crt".to_string(),
            ));
            overrides.push((
                "CARGO_HTTP_CAINFO".to_string(),
                "/workspace/corp-ca.crt".to_string(),
            ));
            overrides.push(("RUSTUP_USE_CURL".to_string(), "1".to_string()));
            // Align with official rust image defaults to avoid triggering fresh installs
            overrides.push(("CARGO_HOME".to_string(), "/usr/local/cargo".to_string()));
            overrides.push(("RUSTUP_HOME".to_string(), "/usr/local/rustup".to_string()));
        }
    } else if !have_mr_toolchain {
        eprintln!("skipping: corporate CA not found and no MR toolchain image provided; rustup may require TLS");
        std::env::remove_var("AIFO_RUST_TOOLCHAIN_USE_OFFICIAL");
        return;
    }

    let sid = aifo_coder::toolchain_start_session(&kinds, &overrides, false, true)
        .expect("start session");
    let (url, token, flag, handle) =
        aifo_coder::toolexec_start_proxy(&sid, true).expect("start proxy");

    // Build a v2 streaming /exec request for cargo
    let (host, port, _path) = parse_host_port_from_http_url(&url).expect("parse url");
    let addr = format!("{}:{}", host, port);
    let mut stream = match TcpStream::connect(&addr) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("skipping: cannot connect to proxy: {}", e);
            // Cleanup
            flag.store(false, std::sync::atomic::Ordering::SeqCst);
            let _ = handle.join();
            aifo_coder::toolchain_cleanup_session(&sid, true);
            return;
        }
    };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));

    // Request body: tool=cargo (+ no args). cwd doesn't matter for help text.
    let body = "tool=cargo&cwd=.";
    let req = format!(
        concat!(
            "POST /exec HTTP/1.1\r\n",
            "Host: {}\r\n",
            "Authorization: Bearer {}\r\n",
            "X-Aifo-Proto: 2\r\n",
            "TE: trailers\r\n",
            "Content-Type: application/x-www-form-urlencoded\r\n",
            "Content-Length: {}\r\n",
            "Connection: close\r\n",
            "\r\n",
            "{}"
        ),
        host,
        token,
        body.len(),
        body
    );
    let _ = stream.write_all(req.as_bytes());
    let _ = stream.flush();

    // Read response and decode chunked
    let buf = read_all_with_timeout(&mut stream, 8000);
    let (body_bytes, exit_code_opt) = decode_chunked(buf);
    let text = String::from_utf8_lossy(&body_bytes).to_string();

    // Debug trace if desired:
    eprintln!(
        "e2e cargo output (first 200 chars): {}",
        &text[..text.len().min(200)]
    );

    // Assert we see cargo help output
    assert!(
        text.contains("Usage: cargo"),
        "expected 'Usage: cargo' in output; got: {}",
        text
    );
    // Trailer exit code must be 0
    assert_eq!(
        exit_code_opt,
        Some(0),
        "expected exit code 0 in trailer; got {:?}",
        exit_code_opt
    );

    // Cleanup proxy and session
    flag.store(false, std::sync::atomic::Ordering::SeqCst);
    let _ = handle.join();
    aifo_coder::toolchain_cleanup_session(&sid, true);
    if ca_copied {
        let _ = std::fs::remove_file("corp-ca.crt");
    }

    // Restore env
    std::env::remove_var("AIFO_RUST_TOOLCHAIN_USE_OFFICIAL");
    std::env::remove_var("AIFO_TOOLEEXEC_TTY");
}
