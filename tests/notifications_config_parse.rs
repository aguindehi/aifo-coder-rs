use once_cell::sync::Lazy;
use std::fs;
use std::sync::Mutex;

static NOTIF_CFG_ENV_GUARD: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

fn with_home<F: FnOnce(&std::path::Path)>(f: F) {
    let old_home = std::env::var("HOME").ok();
    let td = tempfile::tempdir().expect("tmpdir");
    let home = td.path().join("home");
    fs::create_dir_all(&home).unwrap();
    std::env::set_var("HOME", &home);
    f(&home);
    if let Some(v) = old_home {
        std::env::set_var("HOME", v);
    }
}

#[test]
fn test_notifications_config_parse_inline_array() {
    let _g = NOTIF_CFG_ENV_GUARD.lock().unwrap();
    with_home(|home| {
        // Create absolute stub 'say' and write absolute-path config
        let bindir = home.join("bin");
        fs::create_dir_all(&bindir).unwrap();
        let say = bindir.join("say");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::write(&say, "#!/bin/sh\nexit 0\n").unwrap();
            fs::set_permissions(&say, fs::Permissions::from_mode(0o755)).unwrap();
        }
        #[cfg(not(unix))]
        {
            fs::write(&say, "stub").unwrap();
        }

        let cfg = format!(
            "notifications-command: [\"{}\", \"--title\", \"AIFO\"]\n",
            say.display()
        );
        fs::write(home.join(".aider.conf.yml"), cfg).unwrap();

        let cmd = aifo_coder::parse_notifications_command_config().expect("parse ok");
        assert_eq!(cmd.len(), 3, "expected 3 tokens, got: {:?}", cmd);
        let bn = std::path::Path::new(&cmd[0])
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        assert_eq!(bn, "say", "expected basename 'say', got {}", cmd[0]);
        assert_eq!(&cmd[1..], ["--title", "AIFO"]);
    });
}

#[test]
fn test_notifications_config_parse_yaml_list() {
    let _g = NOTIF_CFG_ENV_GUARD.lock().unwrap();
    with_home(|home| {
        // Create absolute stub 'say' and write absolute-path config (YAML list)
        let bindir = home.join("bin");
        fs::create_dir_all(&bindir).unwrap();
        let say = bindir.join("say");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::write(&say, "#!/bin/sh\nexit 0\n").unwrap();
            fs::set_permissions(&say, fs::Permissions::from_mode(0o755)).unwrap();
        }
        #[cfg(not(unix))]
        {
            fs::write(&say, "stub").unwrap();
        }

        let cfg = format!(
            "notifications-command:\n  - \"{}\"\n  - --title\n  - AIFO\n",
            say.display()
        );
        fs::write(home.join(".aider.conf.yml"), cfg).unwrap();

        let cmd = aifo_coder::parse_notifications_command_config().expect("parse ok");
        assert_eq!(cmd.len(), 3, "expected 3 tokens, got: {:?}", cmd);
        let bn = std::path::Path::new(&cmd[0])
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        assert_eq!(bn, "say", "expected basename 'say', got {}", cmd[0]);
        assert_eq!(&cmd[1..], ["--title", "AIFO"]);
    });
}

#[test]
fn test_notifications_config_parse_block_scalar() {
    let _g = NOTIF_CFG_ENV_GUARD.lock().unwrap();
    with_home(|home| {
        // Create absolute stub 'say' and write absolute-path block scalar
        let bindir = home.join("bin");
        fs::create_dir_all(&bindir).unwrap();
        let say = bindir.join("say");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::write(&say, "#!/bin/sh\nexit 0\n").unwrap();
            fs::set_permissions(&say, fs::Permissions::from_mode(0o755)).unwrap();
        }
        #[cfg(not(unix))]
        {
            fs::write(&say, "stub").unwrap();
        }

        let cfg = format!(
            "notifications-command: |\n  {} --title AIFO\n",
            say.display()
        );
        fs::write(home.join(".aider.conf.yml"), cfg).unwrap();

        let cmd = aifo_coder::parse_notifications_command_config().expect("parse ok");
        assert_eq!(cmd.len(), 3, "expected 3 tokens, got: {:?}", cmd);
        let bn = std::path::Path::new(&cmd[0])
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        assert_eq!(bn, "say", "expected basename 'say', got {}", cmd[0]);
        assert_eq!(&cmd[1..], ["--title", "AIFO"]);
    });
}
