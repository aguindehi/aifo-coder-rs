use once_cell::sync::Lazy;
use std::sync::Mutex;

static NOTIF_ENV_GUARD: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

fn write_cfg(tmp_home: &std::path::Path, content: &str) -> std::path::PathBuf {
    let cfg_path = tmp_home.join(".aider.conf.yml");
    std::fs::write(&cfg_path, content).expect("write config");
    cfg_path
}

#[test]
fn test_parse_notifications_inline_array() {
    let _g = NOTIF_ENV_GUARD.lock().unwrap();
    let td = tempfile::tempdir().expect("tmpdir");
    let home = td.path().to_path_buf();
    let old_home = std::env::var("HOME").ok();
    std::env::set_var("HOME", &home);

    // Create absolute stub 'say' and write absolute-path config
    let bindir = home.join("bin");
    std::fs::create_dir_all(&bindir).expect("mkdir bin");
    let say = bindir.join("say");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::write(&say, "#!/bin/sh\nexit 0\n").expect("write say");
        std::fs::set_permissions(&say, std::fs::Permissions::from_mode(0o755)).expect("chmod say");
    }
    let cfg = format!(
        r#"notifications-command: ["{}", "--title", "AIFO"]\n"#,
        say.display()
    );
    let cfg_path = write_cfg(&home, &cfg);
    let old_cfg = std::env::var("AIFO_NOTIFICATIONS_CONFIG").ok();
    std::env::set_var("AIFO_NOTIFICATIONS_CONFIG", &cfg_path);
    let argv = aifo_coder::parse_notifications_command_config().expect("parse array");
    assert_eq!(argv.len(), 3, "expected 3 tokens, got: {:?}", argv);
    let bn = std::path::Path::new(&argv[0])
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    assert_eq!(bn, "say", "expected basename 'say', got {}", argv[0]);
    assert_eq!(&argv[1..], ["--title", "AIFO"]);

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
fn test_parse_notifications_nested_array_lines() {
    let _g = NOTIF_ENV_GUARD.lock().unwrap();
    let td = tempfile::tempdir().expect("tmpdir");
    let home = td.path().to_path_buf();
    let old_home = std::env::var("HOME").ok();
    std::env::set_var("HOME", &home);

    // Create absolute stub 'say' and write absolute-path config (nested YAML list)
    let bindir = home.join("bin");
    std::fs::create_dir_all(&bindir).expect("mkdir bin");
    let say = bindir.join("say");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::write(&say, "#!/bin/sh\nexit 0\n").expect("write say");
        std::fs::set_permissions(&say, std::fs::Permissions::from_mode(0o755)).expect("chmod say");
    }
    #[cfg(not(unix))]
    {
        std::fs::write(&say, "stub").expect("write say");
    }

    let cfg = format!(
        "notifications-command:\n  - \"{}\"\n  - --title\n  - AIFO\n",
        say.display()
    );
    let cfg_path = write_cfg(&home, &cfg);
    let old_cfg = std::env::var("AIFO_NOTIFICATIONS_CONFIG").ok();
    std::env::set_var("AIFO_NOTIFICATIONS_CONFIG", &cfg_path);
    let argv = aifo_coder::parse_notifications_command_config().expect("parse nested array");
    assert_eq!(argv.len(), 3, "expected 3 tokens");
    let bn = std::path::Path::new(&argv[0])
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    assert_eq!(bn, "say", "expected basename 'say', got {}", argv[0]);
    assert_eq!(&argv[1..], ["--title", "AIFO"]);

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
fn test_parse_notifications_block_scalar() {
    let _g = NOTIF_ENV_GUARD.lock().unwrap();
    let td = tempfile::tempdir().expect("tmpdir");
    let home = td.path().to_path_buf();
    let old_home = std::env::var("HOME").ok();
    std::env::set_var("HOME", &home);

    // Create absolute stub 'say' and write absolute-path command in block scalar
    let bindir = home.join("bin");
    std::fs::create_dir_all(&bindir).expect("mkdir bin");
    let say = bindir.join("say");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::write(&say, "#!/bin/sh\nexit 0\n").expect("write say");
        std::fs::set_permissions(&say, std::fs::Permissions::from_mode(0o755)).expect("chmod say");
    }
    #[cfg(not(unix))]
    {
        std::fs::write(&say, "stub").expect("write say");
    }

    let cfg = format!(
        "notifications-command: |\n  {} --title \"AIFO\"\n",
        say.display()
    );
    let cfg_path = write_cfg(&home, &cfg);
    let old_cfg = std::env::var("AIFO_NOTIFICATIONS_CONFIG").ok();
    std::env::set_var("AIFO_NOTIFICATIONS_CONFIG", &cfg_path);
    let argv = aifo_coder::parse_notifications_command_config().expect("parse block");
    assert_eq!(argv.len(), 3);
    let bn = std::path::Path::new(&argv[0])
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    assert_eq!(bn, "say");
    assert_eq!(&argv[1..], ["--title", "AIFO"]);

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
fn test_parse_notifications_single_line_string() {
    let _g = NOTIF_ENV_GUARD.lock().unwrap();
    let td = tempfile::tempdir().expect("tmpdir");
    let home = td.path().to_path_buf();
    let old_home = std::env::var("HOME").ok();
    std::env::set_var("HOME", &home);

    // Create absolute stub 'say' and write absolute-path single string
    let bindir = home.join("bin");
    std::fs::create_dir_all(&bindir).expect("mkdir bin");
    let say = bindir.join("say");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::write(&say, "#!/bin/sh\nexit 0\n").expect("write say");
        std::fs::set_permissions(&say, std::fs::Permissions::from_mode(0o755)).expect("chmod say");
    }
    #[cfg(not(unix))]
    {
        std::fs::write(&say, "stub").expect("write say");
    }

    let cfg = format!(
        r#"notifications-command: "{} --title AIFO"\n"#,
        say.display()
    );
    let cfg_path = write_cfg(&home, &cfg);
    let old_cfg = std::env::var("AIFO_NOTIFICATIONS_CONFIG").ok();
    std::env::set_var("AIFO_NOTIFICATIONS_CONFIG", &cfg_path);
    let argv = aifo_coder::parse_notifications_command_config().expect("parse string");
    assert_eq!(argv.len(), 3);
    let bn = std::path::Path::new(&argv[0])
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    assert_eq!(bn, "say");
    assert_eq!(&argv[1..], ["--title", "AIFO"]);

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
fn test_notifications_args_mismatch_error() {
    let _g = NOTIF_ENV_GUARD.lock().unwrap();
    let td = tempfile::tempdir().expect("tmpdir");
    let home = td.path().to_path_buf();
    let old_home = std::env::var("HOME").ok();
    std::env::set_var("HOME", &home);

    // Create absolute stub 'say' and write absolute-path config
    let bindir = home.join("bin");
    std::fs::create_dir_all(&bindir).expect("mkdir bin");
    let say = bindir.join("say");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::write(&say, "#!/bin/sh\nexit 0\n").expect("write say");
        std::fs::set_permissions(&say, std::fs::Permissions::from_mode(0o755)).expect("chmod say");
    }
    let cfg = format!(
        r#"notifications-command: ["{}", "--title", "AIFO"]\n"#,
        say.display()
    );
    let cfg_path = write_cfg(&home, &cfg);
    let old_cfg = std::env::var("AIFO_NOTIFICATIONS_CONFIG").ok();
    std::env::set_var("AIFO_NOTIFICATIONS_CONFIG", &cfg_path);

    let res =
        aifo_coder::notifications_handle_request(&["--title".into(), "Other".into()], false, 1);
    assert!(res.is_err(), "expected mismatch error, got: {:?}", res);
    let msg = res.err().unwrap();
    assert!(
        msg.contains("arguments mismatch"),
        "unexpected error message: {}",
        msg
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
fn test_notifications_config_rejects_non_say() {
    let _g = NOTIF_ENV_GUARD.lock().unwrap();
    let td = tempfile::tempdir().expect("tmpdir");
    let home = td.path().to_path_buf();
    let old_home = std::env::var("HOME").ok();
    std::env::set_var("HOME", &home);

    // Create absolute stub 'notify' and write absolute-path config
    let bindir = home.join("bin");
    std::fs::create_dir_all(&bindir).expect("mkdir bin");
    let notify = bindir.join("notify");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::write(&notify, "#!/bin/sh\nexit 0\n").expect("write notify");
        std::fs::set_permissions(&notify, std::fs::Permissions::from_mode(0o755))
            .expect("chmod notify");
    }
    let cfg = format!(
        r#"notifications-command: ["{}", "--title", "AIFO"]\n"#,
        notify.display()
    );
    let cfg_path = write_cfg(&home, &cfg);
    let old_cfg = std::env::var("AIFO_NOTIFICATIONS_CONFIG").ok();
    std::env::set_var("AIFO_NOTIFICATIONS_CONFIG", &cfg_path);
    let res =
        aifo_coder::notifications_handle_request(&["--title".into(), "AIFO".into()], false, 1);
    assert!(res.is_err(), "expected error when executable is not 'say'");
    let msg = res.err().unwrap();
    assert!(
        msg.contains("command 'notify' not allowed for notifications"),
        "unexpected error: {}",
        msg
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
