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

#[test]
fn test_cli_fork_list_color_always_has_ansi() {
    if !have_git() {
        eprintln!("skipping: git not found in PATH");
        return;
    }
    let td = tempfile::tempdir().expect("tmpdir");
    let repo = td.path();
    init_repo(repo);

    // Create a minimal fork session with one pane
    let sid = "sid-color";
    let session = repo.join(".aifo-coder").join("forks").join(sid);
    std::fs::create_dir_all(session.join("pane-1")).unwrap();
    // Minimal metadata (optional)
    let meta = format!(
        "{{\"created_at\":{},\"base_label\":\"main\"}}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    );
    let _ = std::fs::write(session.join(".meta.json"), meta);

    let bin = env!("CARGO_BIN_EXE_aifo-coder");
    let out = Command::new(bin)
        .args(["fork", "list", "--color", "always"])
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
        "expected ANSI escapes in color=always output, got:\n{}",
        stdout
    );
}

#[test]
fn test_cli_fork_list_color_never_no_ansi() {
    if !have_git() {
        eprintln!("skipping: git not found in PATH");
        return;
    }
    let td = tempfile::tempdir().expect("tmpdir");
    let repo = td.path();
    init_repo(repo);

    // Create session
    let sid = "sid-color-never";
    let session = repo.join(".aifo-coder").join("forks").join(sid);
    std::fs::create_dir_all(session.join("pane-1")).unwrap();

    let bin = env!("CARGO_BIN_EXE_aifo-coder");
    let out = Command::new(bin)
        .args(["fork", "list", "--color", "never"])
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
        "expected no ANSI escapes in color=never output, got:\n{}",
        stdout
    );
}

#[test]
fn test_cli_fork_list_json_never_colorizes() {
    if !have_git() {
        eprintln!("skipping: git not found in PATH");
        return;
    }
    let td = tempfile::tempdir().expect("tmpdir");
    let repo = td.path();
    init_repo(repo);

    // Create session
    let sid = "sid-color-json";
    let session = repo.join(".aifo-coder").join("forks").join(sid);
    std::fs::create_dir_all(session.join("pane-1")).unwrap();

    let bin = env!("CARGO_BIN_EXE_aifo-coder");
    let out = Command::new(bin)
        .args(["fork", "list", "--json", "--color", "always"])
        .current_dir(repo)
        .output()
        .expect("run aifo-coder fork list --json");
    assert!(
        out.status.success(),
        "fork list --json failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.contains("\x1b["),
        "expected no color in JSON output even with color=always, got:\n{}",
        stdout
    );
}
