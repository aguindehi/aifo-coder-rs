mod support;
use std::fs;

#[test]
fn int_parse_rejects_non_absolute_exec() {
    let td = tempfile::tempdir().expect("tmpdir");
    let home = td.path().to_path_buf();
    let old_home = std::env::var("HOME").ok();
    std::env::set_var("HOME", &home);

    let cfg = r#"notifications-command: ["say","ok"]\n"#;
    let cfg_path = home.join(".aider.conf.yml");
    fs::write(&cfg_path, cfg).expect("write cfg");
    let old_cfg = std::env::var("AIFO_NOTIFICATIONS_CONFIG").ok();
    std::env::set_var("AIFO_NOTIFICATIONS_CONFIG", &cfg_path);

    let err = aifo_coder::notifications_handle_request(&[], false, 2)
        .err()
        .unwrap();
    assert!(
        err.contains("notifications-command executable must be an absolute path"),
        "unexpected error: {}",
        err
    );

    if let Some(v) = old_cfg {
        std::env::set_var("AIFO_NOTIFICATIONS_CONFIG", v);
    } else {
        std::env::remove_var("AIFO_NOTIFICATIONS_CONFIG");
    }
    if let Some(v) = old_home {
        std::env::set_var("HOME", v);
    } else {
        std::env::remove_var("HOME");
    }
}

#[test]
fn int_placeholder_must_be_trailing() {
    let td = tempfile::tempdir().expect("tmpdir");
    let home = td.path().to_path_buf();
    let old_home = std::env::var("HOME").ok();
    std::env::set_var("HOME", &home);

    // Absolute stub
    let bindir = home.join("bin");
    fs::create_dir_all(&bindir).expect("mkdir bin");
    let say = bindir.join("say");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::write(&say, "#!/bin/sh\nexit 0\n").expect("write say");
        fs::set_permissions(&say, fs::Permissions::from_mode(0o755)).expect("chmod say");
    }
    #[cfg(not(unix))]
    {
        fs::write(&say, "stub").expect("write say");
    }

    let cfg = format!(
        r#"notifications-command: ["{}","{{args}}","--"]\n"#,
        say.display()
    );
    let cfg_path = home.join(".aider.conf.yml");
    fs::write(&cfg_path, cfg).expect("write cfg");
    let old_cfg = std::env::var("AIFO_NOTIFICATIONS_CONFIG").ok();
    std::env::set_var("AIFO_NOTIFICATIONS_CONFIG", &cfg_path);

    let err = aifo_coder::notifications_handle_request(&[], false, 2)
        .err()
        .unwrap();
    assert!(
        err.contains("placeholder must be trailing"),
        "unexpected error: {}",
        err
    );

    if let Some(v) = old_cfg {
        std::env::set_var("AIFO_NOTIFICATIONS_CONFIG", v);
    } else {
        std::env::remove_var("AIFO_NOTIFICATIONS_CONFIG");
    }
    if let Some(v) = old_home {
        std::env::set_var("HOME", v);
    } else {
        std::env::remove_var("HOME");
    }
}

#[test]
fn int_allowlist_env_extension_notify_send() {
    // Setup config with absolute stub notify-send and allowlist env extending defaults
    let td = tempfile::tempdir().expect("tmpdir");
    let dir = td.path();
    let bindir = dir.join("bin");
    fs::create_dir_all(&bindir).expect("mkdir bin");
    let notify = bindir.join("notify-send");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::write(&notify, "#!/bin/sh\necho ns\nexit 0\n").expect("write notify");
        fs::set_permissions(&notify, fs::Permissions::from_mode(0o755)).expect("chmod notify");
    }
    #[cfg(not(unix))]
    {
        fs::write(&notify, "ns").expect("write notify");
    }

    let cfg = format!(r#"notifications-command: ["{}"]\n"#, notify.display());
    let cfg_path = dir.join("aider.yml");
    fs::write(&cfg_path, cfg).expect("write cfg");

    let _env_guard = support::notifications_allow_test_exec_from(&bindir);
    let old_cfg = std::env::var("AIFO_NOTIFICATIONS_CONFIG").ok();
    let old_allow = std::env::var("AIFO_NOTIFICATIONS_ALLOWLIST").ok();
    std::env::set_var("AIFO_NOTIFICATIONS_CONFIG", &cfg_path);
    std::env::set_var("AIFO_NOTIFICATIONS_ALLOWLIST", "say,notify-send");

    let (code, out) = aifo_coder::notifications_handle_request(&[], false, 10).expect("notify ok");
    assert_eq!(code, 0, "expected exit 0");
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("ns"), "expected stub output, got: {}", s);

    if let Some(v) = old_cfg {
        std::env::set_var("AIFO_NOTIFICATIONS_CONFIG", v);
    } else {
        std::env::remove_var("AIFO_NOTIFICATIONS_CONFIG");
    }
    if let Some(v) = old_allow {
        std::env::set_var("AIFO_NOTIFICATIONS_ALLOWLIST", v);
    } else {
        std::env::remove_var("AIFO_NOTIFICATIONS_ALLOWLIST");
    }
}

#[test]
fn int_max_args_truncation_with_placeholder() {
    // Config with trailing {args} and MAX_ARGS=2; provide 3 args and expect truncation to 2.
    let td = tempfile::tempdir().expect("tmpdir");
    let dir = td.path();
    let bindir = dir.join("bin");
    fs::create_dir_all(&bindir).expect("mkdir bin");
    let echoargs = bindir.join("echoargs");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::write(&echoargs, "#!/bin/sh\necho args:$*\nexit 0\n").expect("write echoargs");
        fs::set_permissions(&echoargs, fs::Permissions::from_mode(0o755)).expect("chmod echoargs");
    }
    #[cfg(not(unix))]
    {
        fs::write(&echoargs, "args").expect("write echoargs");
    }

    let cfg = format!(
        r#"notifications-command: ["{}","--","{{args}}"]\n"#,
        echoargs.display()
    );
    let cfg_path = dir.join("aider.yml");
    fs::write(&cfg_path, cfg).expect("write cfg");

    let _env_guard = support::notifications_allow_test_exec_from(&bindir);
    let old_cfg = std::env::var("AIFO_NOTIFICATIONS_CONFIG").ok();
    let old_max = std::env::var("AIFO_NOTIFICATIONS_MAX_ARGS").ok();
    let old_allow = std::env::var("AIFO_NOTIFICATIONS_ALLOWLIST").ok();
    std::env::set_var("AIFO_NOTIFICATIONS_CONFIG", &cfg_path);
    std::env::set_var("AIFO_NOTIFICATIONS_MAX_ARGS", "2");
    // Extend allowlist for this test to include the stub 'echoargs' (default allowlist is ["say"])
    std::env::set_var("AIFO_NOTIFICATIONS_ALLOWLIST", "say,echoargs");

    let args = vec!["A".to_string(), "B".to_string(), "C".to_string()];
    let (_code, out) =
        aifo_coder::notifications_handle_request(&args, false, 10).expect("notify ok");
    let s = String::from_utf8_lossy(&out);
    assert!(
        s.contains("args:-- A B"),
        "expected only first 2 args, got: {}",
        s
    );
    assert!(
        !s.contains("A B C"),
        "should not include the 3rd arg, got: {}",
        s
    );

    if let Some(v) = old_cfg {
        std::env::set_var("AIFO_NOTIFICATIONS_CONFIG", v);
    } else {
        std::env::remove_var("AIFO_NOTIFICATIONS_CONFIG");
    }
    if let Some(v) = old_max {
        std::env::set_var("AIFO_NOTIFICATIONS_MAX_ARGS", v);
    } else {
        std::env::remove_var("AIFO_NOTIFICATIONS_MAX_ARGS");
    }
    if let Some(v) = old_allow {
        std::env::set_var("AIFO_NOTIFICATIONS_ALLOWLIST", v);
    } else {
        std::env::remove_var("AIFO_NOTIFICATIONS_ALLOWLIST");
    }
}
