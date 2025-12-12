mod support;
use std::fs;

#[test]
fn int_test_proxy_notifications_policy_auth_vs_noauth() {
    // Respect CI override disabling docker and ensure daemon is reachable
    if std::env::var("AIFO_CODER_TEST_DISABLE_DOCKER")
        .ok()
        .as_deref()
        == Some("1")
    {
        eprintln!("skipping: AIFO_CODER_TEST_DISABLE_DOCKER=1");
        return;
    }
    let runtime = match aifo_coder::container_runtime_path() {
        Ok(p) => p,
        Err(_) => {
            eprintln!("skipping: docker not found in PATH");
            return;
        }
    };
    let ok = std::process::Command::new(&runtime)
        .arg("ps")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !ok {
        eprintln!("skipping: Docker daemon not reachable");
        return;
    }

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

    // Config with expected args (absolute stub path)
    let cfg_content = format!(
        "notifications-command: [\"{}\", \"--title\", \"AIFO\"]\n",
        say.display()
    );
    fs::write(home.join(".aider.conf.yml"), cfg_content).unwrap();
    let _env_guard = support::notifications_allow_test_exec_from(&bindir);

    // Start proxy
    let sid = format!("notifpol-{}", std::process::id());
    let (url, token, flag, handle) =
        aifo_coder::toolexec_start_proxy(&sid, false).expect("start proxy");

    fn extract_port(u: &str) -> u16 {
        support::port_from_http_url(u)
    }
    let port = extract_port(&url);

    // Default: missing auth -> 401 on /notify
    {
        let body = "arg=--title&arg=AIFO";
        let req = format!(
            "POST /notify HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n\r\n{}",
            body.len(), body
        );
        let txt = support::http_send_raw(port, &req);
        assert!(
            txt.contains("401 Unauthorized"),
            "expected 401 when missing auth: {}",
            txt
        );
    }

    // Auth present but missing proto -> 426
    {
        let body = "arg=--title&arg=AIFO";
        let req = format!(
            "POST /notify HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {}\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n\r\n{}",
            token, body.len(), body
        );
        let txt = support::http_send_raw(port, &req);
        assert!(
            txt.contains("426 Upgrade Required"),
            "expected 426 when auth ok but missing proto: {}",
            txt
        );
    }

    // NOAUTH=1 allows unauthenticated /notify; with arg mismatch expect 403 + reason
    std::env::set_var("AIFO_NOTIFICATIONS_NOAUTH", "1");
    {
        let body = "cmd=say&arg=--oops";
        let req = format!(
            "POST /notify HTTP/1.1\r\nHost: localhost\r\nX-Aifo-Proto: 2\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n\r\n{}",
            body.len(), body
        );
        let txt = support::http_send_raw(port, &req);
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
