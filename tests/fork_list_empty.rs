use std::process::Command;

fn init_repo(dir: &std::path::Path) {
    let _ = std::process::Command::new("git")
        .arg("init")
        .current_dir(dir)
        .status();
    let _ = std::process::Command::new("git")
        .args(["config", "user.name", "UT"])
        .current_dir(dir)
        .status();
    let _ = std::process::Command::new("git")
        .args(["config", "user.email", "ut@example.com"])
        .current_dir(dir)
        .status();
    let _ = std::fs::write(dir.join("init.txt"), "x\n");
    let _ = std::process::Command::new("git")
        .args(["add", "-A"])
        .current_dir(dir)
        .status();
    let _ = std::process::Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(dir)
        .status();
}

#[test]
fn test_fork_list_json_empty_returns_empty_array() {
    let td = tempfile::tempdir().expect("tmpdir");
    let root = td.path().to_path_buf();
    init_repo(&root);

    let bin = env!("CARGO_BIN_EXE_aifo-coder");
    let out = Command::new(bin)
        .args(["fork", "list", "--json"])
        .current_dir(&root)
        .output()
        .expect("run aifo-coder fork list --json");
    assert!(
        out.status.success(),
        "fork list --json should succeed; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(stdout.trim(), "[]", "expected empty JSON array, got:\n{}", stdout);
}

#[test]
fn test_fork_list_text_empty_reports_none() {
    let td = tempfile::tempdir().expect("tmpdir");
    let root = td.path().to_path_buf();
    init_repo(&root);

    let bin = env!("CARGO_BIN_EXE_aifo-coder");
    let out = Command::new(bin)
        .args(["fork", "list"])
        .current_dir(&root)
        .output()
        .expect("run aifo-coder fork list");
    assert!(
        out.status.success(),
        "fork list should succeed; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("no fork sessions found"),
        "expected 'no fork sessions found' text, got:\n{}",
        stdout
    );
}
