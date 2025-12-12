mod support;

#[cfg(unix)]
#[test]
fn int_notifications_handle_request_with_stub_say() {
    use std::fs::{self, File};
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;

    // Temp workspace for config and stub binary
    let td = tempfile::tempdir().expect("tmpdir");
    let dir = td.path();

    // Create a stub 'say' that prints its first two args and exits 0
    let bindir = dir.join("bin");
    fs::create_dir_all(&bindir).expect("mkdir bin");
    let say = bindir.join("say");
    let mut s = File::create(&say).expect("create say");
    writeln!(s, "#!/bin/sh\nprintf \"stub-say:%s %s\\n\" \"$1\" \"$2\"").expect("write say");
    fs::set_permissions(&say, fs::Permissions::from_mode(0o755)).expect("chmod say");
    // Ensure the stub file is closed before attempting to execute it to avoid ETXTBUSY
    drop(s);

    // Write minimal config pointing to absolute stub say with fixed args
    let cfg = dir.join("aider.yml");
    let cfg_line = format!(
        "notifications-command: [\"{}\",\"--title\",\"AIFO\"]",
        say.display()
    );
    let mut f = File::create(&cfg).expect("create cfg");
    writeln!(f, "{cfg_line}").expect("write cfg");

    // Save and set environment (use EnvGuard to ensure restoration even on panic)
    let _env_guard = support::notifications_allow_test_exec_from(&bindir)
        .set("AIFO_NOTIFICATIONS_CONFIG", cfg.to_string_lossy().to_string())
        .set(
            "PATH",
            format!(
                "{}:{}",
                bindir.display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        );

    // Invoke notifications handler
    let args = vec!["--title".to_string(), "AIFO".to_string()];
    let (code, out) = aifo_coder::notifications_handle_request(&args, false, 3)
        .expect("notifications_handle_request ok");

    assert_eq!(code, 0, "expected exit 0, got {}", code);
    let s = String::from_utf8_lossy(&out).trim().to_string();
    assert_eq!(s, "stub-say:--title AIFO");

    // Env restored by EnvGuard
}
