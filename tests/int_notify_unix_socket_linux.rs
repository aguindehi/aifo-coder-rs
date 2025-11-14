#[test]
fn test_notify_unix_socket_say_ok_linux_only() {
    // Linux only (UDS transport)
    if !cfg!(target_os = "linux") {
        eprintln!("skipping: unix socket transport not supported on this OS");
        return;
    }
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    use std::fs;
    use std::process::Command;

    // Isolate HOME and PATH; provide a fake 'say'
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
    let say = bindir.join("say");
    fs::write(&say, "#!/bin/sh\necho say-$*\nexit 0\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&say, fs::Permissions::from_mode(0o755)).unwrap();
    }

    // Write notifications-command config
    fs::write(
        home.join(".aider.conf.yml"),
        "notifications-command: [\"say\", \"--title\", \"AIFO\"]\n",
    )
    .unwrap();

    // Start proxy with unix socket transport
    std::env::set_var("AIFO_TOOLEEXEC_USE_UNIX", "1");
    let sid = format!("notify-uds-{}", std::process::id());
    // Ensure socket directory exists; skip if we cannot create it on this host.
    let dir = format!("/run/aifo/aifo-{}", sid);
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("skipping: cannot create {}: {}", dir, e);
        std::env::remove_var("AIFO_TOOLEEXEC_USE_UNIX");
        return;
    }
    let (url, token, flag, handle) =
        aifo_coder::toolexec_start_proxy(&sid, false).expect("start proxy");

    // Write shims and invoke say over UDS
    let tmp = tempfile::tempdir().expect("tmpdir2");
    aifo_coder::toolchain_write_shims(tmp.path()).expect("write shims");
    let shim = tmp.path().join("say");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&shim, fs::Permissions::from_mode(0o755));
    }

    let status = Command::new(&shim)
        .arg("--title")
        .arg("AIFO")
        .env("AIFO_TOOLEEXEC_URL", &url)
        .env("AIFO_TOOLEEXEC_TOKEN", &token)
        .status()
        .expect("exec say shim");
    let code = status.code().unwrap_or(1);
    assert_eq!(code, 0, "expected exit 0 via UDS, got {}", code);

    // Cleanup proxy/session
    flag.store(false, std::sync::atomic::Ordering::SeqCst);
    let _ = handle.join();
    aifo_coder::toolchain_cleanup_session(&sid, false);
    std::env::remove_var("AIFO_TOOLEEXEC_USE_UNIX");

    // Restore env
    if let Some(v) = old_home {
        std::env::set_var("HOME", v);
    }
    if let Some(v) = old_path {
        std::env::set_var("PATH", v);
    }
}
