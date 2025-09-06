use std::process::Command;

fn have_git() -> bool {
    Command::new("git")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn init_repo(dir: &std::path::Path) {
    let _ = Command::new("git").arg("init").current_dir(dir).status();
    let _ = Command::new("git")
        .args(["config", "user.name", "UT"])
        .current_dir(dir)
        .status();
    let _ = Command::new("git")
        .args(["config", "user.email", "ut@example.com"])
        .current_dir(dir)
        .status();
    let _ = std::fs::write(dir.join("init.txt"), "x\n");
    let _ = Command::new("git")
        .args(["add", "-A"])
        .current_dir(dir)
        .status();
    let _ = Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(dir)
        .status();
}

fn ensure_minimal_session(repo: &std::path::Path, sid: &str) {
    let session = repo.join(".aifo-coder").join("forks").join(sid);
    std::fs::create_dir_all(session.join("pane-1")).unwrap();
    let meta = format!(
        "{{\"created_at\":{},\"base_label\":\"main\"}}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    );
    let _ = std::fs::write(session.join(".meta.json"), meta);
}

#[test]
fn test_color_env_always_applies_when_no_cli_flag() {
    if !have_git() {
        eprintln!("skipping: git not found in PATH");
        return;
    }
    let td = tempfile::tempdir().expect("tmpdir");
    let repo = td.path();
    init_repo(repo);
    ensure_minimal_session(repo, "sid-color-env");

    let bin = env!("CARGO_BIN_EXE_aifo-coder");
    let out = Command::new(bin)
        .args(["fork", "list"])
        .env("AIFO_CODER_COLOR", "always")
        .current_dir(repo)
        .output()
        .expect("run aifo-coder fork list");
    assert!(
        out.status.success(),
        "fork list failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("\x1b["),
        "expected ANSI escapes when AIFO_CODER_COLOR=always and no CLI flag, got:\n{}",
        stdout
    );
}

#[test]
fn test_no_color_env_disables_even_with_cli_always() {
    if !have_git() {
        eprintln!("skipping: git not found in PATH");
        return;
    }
    let td = tempfile::tempdir().expect("tmpdir");
    let repo = td.path();
    init_repo(repo);
    ensure_minimal_session(repo, "sid-no-color");

    let bin = env!("CARGO_BIN_EXE_aifo-coder");
    let out = Command::new(bin)
        .args(["fork", "list", "--color", "always"])
        .env("NO_COLOR", "1")
        .current_dir(repo)
        .output()
        .expect("run aifo-coder fork list");
    assert!(
        out.status.success(),
        "fork list failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.contains("\x1b["),
        "expected no ANSI escapes when NO_COLOR=1 even with --color always, got:\n{}",
        stdout
    );
}

#[test]
fn test_cli_overrides_env_when_no_no_color() {
    if !have_git() {
        eprintln!("skipping: git not found in PATH");
        return;
    }
    let td = tempfile::tempdir().expect("tmpdir");
    let repo = td.path();
    init_repo(repo);
    ensure_minimal_session(repo, "sid-cli-over-env");

    let bin = env!("CARGO_BIN_EXE_aifo-coder");
    let out = Command::new(bin)
        .args(["fork", "list", "--color", "always"])
        .env("AIFO_CODER_COLOR", "never")
        .current_dir(repo)
        .output()
        .expect("run aifo-coder fork list");
    assert!(
        out.status.success(),
        "fork list failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("\x1b["),
        "expected ANSI escapes when --color always overrides AIFO_CODER_COLOR=never, got:\n{}",
        stdout
    );
}
