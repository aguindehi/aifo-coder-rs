use std::fs;

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
    with_home(|home| {
        fs::write(
            home.join(".aider.conf.yml"),
            "notifications-command: [\"say\", \"--title\", \"AIFO\"]\n",
        )
        .unwrap();
        let cmd = aifo_coder::parse_notifications_command_config().expect("parse ok");
        assert_eq!(cmd, vec!["say", "--title", "AIFO"]);
    });
}

#[test]
fn test_notifications_config_parse_yaml_list() {
    with_home(|home| {
        fs::write(
            home.join(".aider.conf.yml"),
            "notifications-command:\n  - say\n  - --title\n  - AIFO\n",
        )
        .unwrap();
        let cmd = aifo_coder::parse_notifications_command_config().expect("parse ok");
        assert_eq!(cmd, vec!["say", "--title", "AIFO"]);
    });
}

#[test]
fn test_notifications_config_parse_block_scalar() {
    with_home(|home| {
        fs::write(
            home.join(".aider.conf.yml"),
            "notifications-command: |\n  say --title AIFO\n",
        )
        .unwrap();
        let cmd = aifo_coder::parse_notifications_command_config().expect("parse ok");
        // Block scalar may split by whitespace
        assert_eq!(cmd, vec!["say", "--title", "AIFO"]);
    });
}
