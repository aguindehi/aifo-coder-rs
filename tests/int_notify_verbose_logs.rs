mod support;
use std::fs;

#[test]
fn int_test_notify_verbose_logs_include_parsed_and_result() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // Isolate HOME and PATH; provide a fake 'say' (EnvGuard restores automatically)
    let td = tempfile::tempdir().expect("tmpdir");
    let home = td.path().join("home");
    let bindir = td.path().join("bin");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&bindir).unwrap();
    let _env_guard = support::EnvGuard::new()
        .set("HOME", home.to_string_lossy().to_string())
        .set(
            "PATH",
            format!(
                "{}:{}",
                bindir.display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        );
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

    // Prepare log path
    let logf = td.path().join("proxy.log");
    std::env::set_var("AIFO_TEST_LOG_PATH", &logf);

    // Start proxy in verbose mode
    let sid = format!("notify-logs-{}", std::process::id());
    let (url, _token, flag, handle) =
        aifo_coder::toolexec_start_proxy(&sid, true).expect("start proxy");

    // Allow NOAUTH to simplify raw HTTP request
    std::env::set_var("AIFO_NOTIFICATIONS_NOAUTH", "1");

    // Extract port from URL and send a raw notify request with cmd + args
    let port = support::port_from_http_url(&url);
    let body = "cmd=say&arg=--title&arg=AIFO";
    let req = format!(
        "POST /notify HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/x-www-form-urlencoded\r\nX-Aifo-Proto: 2\r\nContent-Length: {}\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = support::http_send_raw(port, &req);

    // Read log file and assert lines present
    let log = fs::read_to_string(&logf).expect("read log");
    assert!(
        log.contains("proxy notify parsed cmd=say argv=--title AIFO"),
        "missing parsed log line in: {}",
        log
    );
    assert!(
        log.contains("proxy result tool=say kind=notify code=0 dur_ms="),
        "missing result log line in: {}",
        log
    );

    // Cleanup
    std::env::remove_var("AIFO_NOTIFICATIONS_NOAUTH");
    std::env::remove_var("AIFO_TEST_LOG_PATH");
    flag.store(false, std::sync::atomic::Ordering::SeqCst);
    let _ = handle.join();
    aifo_coder::toolchain_cleanup_session(&sid, false);

    // Env restored by EnvGuard
}
