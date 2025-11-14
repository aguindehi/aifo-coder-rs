#[test]
fn test_proxy_notify_say_noauth_tcp() {
    // Skip if docker isn't available on this host (proxy requires docker CLI path for runtime)
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    use std::io::{Read, Write};

    // Prepare temp config and stub 'say' on PATH
    let td = tempfile::tempdir().expect("tmpdir");
    let dir = td.path();

    // Stub say script that prints its args
    let bindir = dir.join("bin");
    std::fs::create_dir_all(&bindir).expect("mkdir bin");
    let say = bindir.join("say");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::write(
            &say,
            "#!/bin/sh\nprintf \"stub-say:%s %s\\n\" \"$1\" \"$2\"\n",
        )
        .expect("write say");
        std::fs::set_permissions(&say, std::fs::Permissions::from_mode(0o755)).expect("chmod say");
    }
    #[cfg(not(unix))]
    {
        // On non-unix hosts, skip (stub would need .bat/.cmd)
        eprintln!("skipping: non-unix host for stub say");
        return;
    }

    // Config file (absolute path to stub)
    let cfg = dir.join("aider.yml");
    let cfg_content = format!(
        "notifications-command: [\"{}\",\"--title\",\"AIFO\"]\n",
        say.display()
    );
    std::fs::write(&cfg, cfg_content).expect("write cfg");

    // Save env and set for proxy thread
    let old_cfg = std::env::var("AIFO_NOTIFICATIONS_CONFIG").ok();
    let old_path = std::env::var("PATH").ok();
    let old_noauth = std::env::var("AIFO_NOTIFICATIONS_NOAUTH").ok();

    std::env::set_var("AIFO_NOTIFICATIONS_CONFIG", &cfg);
    let mut path_val = bindir.to_string_lossy().to_string();
    if let Some(p) = &old_path {
        if !p.is_empty() {
            path_val.push(':');
            path_val.push_str(p);
        }
    }
    std::env::set_var("PATH", path_val);
    std::env::set_var("AIFO_NOTIFICATIONS_NOAUTH", "1");

    // Start proxy (TCP)
    let (url, _token, running, handle) =
        aifo_coder::toolexec_start_proxy("unit-test-session", false).expect("start proxy");

    // Derive host:port from url like http://127.0.0.1:12345/exec
    let addr = url
        .strip_prefix("http://")
        .unwrap_or(&url)
        .split('/')
        .next()
        .unwrap()
        .to_string();

    // Build request body and HTTP message
    let body = "cmd=say&arg=--title&arg=AIFO";
    let req = format!(
        "POST /notify HTTP/1.1\r\nHost: localhost\r\nX-Aifo-Proto: 2\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );

    // Connect and send
    let mut stream = std::net::TcpStream::connect(&addr).expect("connect");
    stream.write_all(req.as_bytes()).expect("write request");
    stream.flush().ok();

    // Read all response
    let mut resp = Vec::new();
    stream.read_to_end(&mut resp).ok();
    let s = String::from_utf8_lossy(&resp);

    assert!(
        s.contains("200 OK"),
        "expected 200 OK, got:\n---\n{}\n---",
        s
    );
    // Check exit code
    let mut exit_code_ok = false;
    for line in s.lines() {
        if line.to_ascii_lowercase().starts_with("x-exit-code:") {
            let v = line.split(':').nth(1).unwrap_or("").trim();
            if v == "0" {
                exit_code_ok = true;
            }
        }
    }
    assert!(exit_code_ok, "expected X-Exit-Code: 0, got:\n{}", s);
    assert!(
        s.contains("stub-say:--title AIFO"),
        "expected stub say output, got:\n{}",
        s
    );

    // Stop proxy
    use std::sync::atomic::Ordering;
    running.store(false, Ordering::SeqCst);
    let _ = handle.join();

    // Restore env
    if let Some(v) = old_cfg {
        std::env::set_var("AIFO_NOTIFICATIONS_CONFIG", v);
    } else {
        std::env::remove_var("AIFO_NOTIFICATIONS_CONFIG");
    }
    if let Some(v) = old_path {
        std::env::set_var("PATH", v);
    } else {
        std::env::remove_var("PATH");
    }
    if let Some(v) = old_noauth {
        std::env::set_var("AIFO_NOTIFICATIONS_NOAUTH", v);
    } else {
        std::env::remove_var("AIFO_NOTIFICATIONS_NOAUTH");
    }
}
