use std::fs;
use std::io::{Read, Write};

#[test]
fn test_proxy_notifications_policy_auth_vs_noauth() {
    // Isolate HOME and PATH for config and say stub
    let old_home = std::env::var("HOME").ok();
    let old_path = std::env::var("PATH").ok();

    let td = tempfile::tempdir().expect("tmpdir");
    let home = td.path().join("home");
    let bindir = td.path().join("bin");
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

    // Provide a say stub; may or may not be used depending on policy outcome
    let say = bindir.join("say");
    fs::write(&say, "#!/bin/sh\necho say-$*\nexit 0\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&say, fs::Permissions::from_mode(0o755)).unwrap();
    }

    // Config with expected args
    fs::write(
        home.join(".aider.conf.yml"),
        "notifications-command: [\"say\", \"--title\", \"AIFO\"]\n",
    )
    .unwrap();

    // Start proxy
    let sid = format!("notifpol-{}", std::process::id());
    let (url, token, flag, handle) =
        aifo_coder::toolexec_start_proxy(&sid, false).expect("start proxy");

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

    // Default: missing auth -> 401 on /notify
    {
        use std::net::TcpStream;
        let mut s = TcpStream::connect(("127.0.0.1", port)).expect("connect unauth");
        let body = "arg=--title&arg=AIFO";
        let req = format!(
            "POST /notify HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n\r\n{}",
            body.len(), body
        );
        s.write_all(req.as_bytes()).expect("write");
        let mut resp = Vec::new();
        let _ = s.read_to_end(&mut resp);
        let txt = String::from_utf8_lossy(&resp);
        assert!(
            txt.contains("401 Unauthorized"),
            "expected 401 when missing auth: {}",
            txt
        );
    }

    // Auth present but missing proto -> 426
    {
        use std::net::TcpStream;
        let mut s = TcpStream::connect(("127.0.0.1", port)).expect("connect bad proto");
        let body = "arg=--title&arg=AIFO";
        let req = format!(
            "POST /notify HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {}\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n\r\n{}",
            token, body.len(), body
        );
        s.write_all(req.as_bytes()).expect("write");
        let mut resp = Vec::new();
        let _ = s.read_to_end(&mut resp);
        let txt = String::from_utf8_lossy(&resp);
        assert!(
            txt.contains("426 Upgrade Required"),
            "expected 426 when auth ok but missing proto: {}",
            txt
        );
    }

    // NOAUTH=1 allows unauthenticated /notify; with arg mismatch expect 403 + reason
    std::env::set_var("AIFO_NOTIFICATIONS_NOAUTH", "1");
    {
        use std::net::TcpStream;
        let mut s = TcpStream::connect(("127.0.0.1", port)).expect("connect noauth");
        let body = "arg=--oops";
        let req = format!(
            "POST /notify HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n\r\n{}",
            body.len(), body
        );
        s.write_all(req.as_bytes()).expect("write");
        let mut resp = Vec::new();
        let _ = s.read_to_end(&mut resp);
        let txt = String::from_utf8_lossy(&resp);
        assert!(
            txt.contains("403 Forbidden"),
            "expected 403 policy rejection under NOAUTH: {}",
            txt
        );
        assert!(
            txt.contains("arguments mismatch"),
            "expected mismatch reason under NOAUTH: {}",
            txt
        );
    }
    std::env::remove_var("AIFO_NOTIFICATIONS_NOAUTH");

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
