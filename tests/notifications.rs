use std::fs;
use std::io::{Read, Write};

#[test]
fn test_notifications_cmd_e2e_ok_and_mismatch() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // Save current HOME/PATH to restore later
    let old_home = std::env::var("HOME").ok();
    let old_path = std::env::var("PATH").ok();

    // Isolate HOME and PATH
    let tmpd = tempfile::tempdir().expect("tmpdir");
    let home = tmpd.path().join("home");
    let bindir = tmpd.path().join("bin");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&bindir).unwrap();
    std::env::set_var("HOME", &home);
    std::env::set_var(
        "PATH",
        format!(
            "{}:{}",
            bindir.display(),
            old_path.clone().unwrap_or_default()
        ),
    );

    // Synthetic 'say' printing args and exiting 0
    let say = bindir.join("say");
    fs::write(&say, "#!/bin/sh\necho say-$*\nexit 0\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&say, fs::Permissions::from_mode(0o755)).unwrap();
    }

    // ~/.aider.conf.yml with notifications-command
    fs::write(
        home.join(".aider.conf.yml"),
        "notifications-command: [\"say\", \"--title\", \"AIFO\"]\n",
    )
    .unwrap();

    // Start proxy without launching sidecars (notifications-cmd does not require sidecars)
    let sid = format!("notif-{}", std::process::id());
    let (url, token, flag, handle) =
        aifo_coder::toolexec_start_proxy(&sid, false).expect("failed to start proxy");

    fn extract_port(u: &str) -> u16 {
        let after_scheme = u.split("://").nth(1).unwrap_or(u);
        let host_port = after_scheme.split('/').next().unwrap_or(after_scheme);
        host_port
            .rsplit(':')
            .next()
            .unwrap_or("0")
            .parse::<u16>()
            .unwrap_or(0)
    }
    let port = extract_port(&url);

    // OK case: args match config
    {
        use std::net::TcpStream;
        let mut s = TcpStream::connect(("127.0.0.1", port)).expect("connect");
        let body = "arg=--title&arg=AIFO";
        let req = format!(
            "POST /notify HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {}\r\nX-Aifo-Proto: 1\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n\r\n{}",
            token, body.len(), body
        );
        s.write_all(req.as_bytes()).expect("write");
        let mut resp = Vec::new();
        s.read_to_end(&mut resp).ok();
        let txt = String::from_utf8_lossy(&resp);
        assert!(txt.contains("200 OK"), "expected 200: {}", txt);
        assert!(
            txt.contains("X-Exit-Code: 0"),
            "exit code mismatch: {}",
            txt
        );
        assert!(
            txt.contains("say---title AIFO"),
            "output mismatch (say args missing): {}",
            txt
        );
    }

    // Mismatch case: wrong args → expect 403 and exit 86
    {
        use std::net::TcpStream;
        let mut s = TcpStream::connect(("127.0.0.1", port)).expect("connect2");
        let body = "arg=--oops";
        let req = format!(
            "POST /notify HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {}\r\nX-Aifo-Proto: 1\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n\r\n{}",
            token, body.len(), body
        );
        s.write_all(req.as_bytes()).expect("write2");
        let mut resp = Vec::new();
        s.read_to_end(&mut resp).ok();
        let txt = String::from_utf8_lossy(&resp);
        assert!(txt.contains("403 Forbidden"), "expected 403: {}", txt);
        assert!(txt.contains("X-Exit-Code: 86"), "expected exit 86: {}", txt);
        assert!(
            txt.contains("arguments mismatch"),
            "expected mismatch reason"
        );
    }

    // Cleanup proxy/session
    flag.store(false, std::sync::atomic::Ordering::SeqCst);
    let _ = handle.join();
    aifo_coder::toolchain_cleanup_session(&sid, false);

    // Restore environment
    if let Some(v) = old_home {
        std::env::set_var("HOME", v);
    }
    if let Some(v) = old_path {
        std::env::set_var("PATH", v);
    }
}
