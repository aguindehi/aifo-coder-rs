use std::fs;
use std::path::PathBuf;
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

fn init_repo(dir: &PathBuf) {
    let _ = Command::new("git").arg("init").current_dir(dir).status();
    let _ = Command::new("git")
        .args(["config", "user.name", "UT"])
        .current_dir(dir)
        .status();
    let _ = Command::new("git")
        .args(["config", "user.email", "ut@example.com"])
        .current_dir(dir)
        .status();
    let _ = fs::write(dir.join("init.txt"), "x\n");
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
fn test_stale_notice_suppressed_during_maintenance() {
    if !have_git() {
        eprintln!("skipping: git not found in PATH");
        return;
    }
    let td = tempfile::tempdir().expect("tmpdir");
    let root = td.path().to_path_buf();
    init_repo(&root);

    // Create one old session so stale notice would trigger if not suppressed
    let sid = "sid-old-notice";
    let base = root.join(".aifo-coder").join("forks").join(sid);
    let pane = base.join("pane-1");
    fs::create_dir_all(&pane).unwrap();
    init_repo(&pane);
    let head = String::from_utf8_lossy(
        &Command::new("git")
            .args(["rev-parse", "--verify", "HEAD"])
            .current_dir(&pane)
            .output()
            .unwrap()
            .stdout,
    )
    .trim()
    .to_string();
    let old_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        - 5 * 86400;
    let meta = format!(
        "{{ \"created_at\": {}, \"base_label\": \"main\", \"base_ref_or_sha\": \"main\", \"base_commit_sha\": \"{}\", \"panes\": 1, \"pane_dirs\": [\"{}\"], \"branches\": [\"fork/main/{sid}-1\"], \"layout\": \"tiled\" }}",
        old_secs, head, pane.display()
    );
    fs::create_dir_all(&base).unwrap();
    fs::write(base.join(".meta.json"), meta).unwrap();

    let bin = env!("CARGO_BIN_EXE_aifo-coder");
    let out = Command::new(bin)
        .args(["fork", "list", "--json"])
        .env("AIFO_CODER_FORK_STALE_DAYS", "1")
        .current_dir(&root)
        .output()
        .expect("run aifo-coder fork list --json");
    assert!(out.status.success(), "fork list should succeed");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        !err.contains("Found ") && !err.contains("old fork sessions"),
        "stale notice should be suppressed for maintenance commands; got stderr: {}",
        err
    );
}

#[test]
fn test_stale_notice_printed_for_doctor() {
    if !have_git() {
        eprintln!("skipping: git not found in PATH");
        return;
    }
    let td = tempfile::tempdir().expect("tmpdir");
    let root = td.path().to_path_buf();
    init_repo(&root);

    // Create one old session so stale notice should trigger
    let sid = "sid-old-notice2";
    let base = root.join(".aifo-coder").join("forks").join(sid);
    let pane = base.join("pane-1");
    fs::create_dir_all(&pane).unwrap();
    init_repo(&pane);
    let head = String::from_utf8_lossy(
        &Command::new("git")
            .args(["rev-parse", "--verify", "HEAD"])
            .current_dir(&pane)
            .output()
            .unwrap()
            .stdout,
    )
    .trim()
    .to_string();
    let old_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        - 5 * 86400;
    let meta = format!(
        "{{ \"created_at\": {}, \"base_label\": \"main\", \"base_ref_or_sha\": \"main\", \"base_commit_sha\": \"{}\", \"panes\": 1, \"pane_dirs\": [\"{}\"], \"branches\": [\"fork/main/{sid}-1\"], \"layout\": \"tiled\" }}",
        old_secs, head, pane.display()
    );
    fs::create_dir_all(&base).unwrap();
    fs::write(base.join(".meta.json"), meta).unwrap();

    let bin = env!("CARGO_BIN_EXE_aifo-coder");
    // Run doctor with PATH emptied to avoid docker dependency
    let out = Command::new(bin)
        .arg("doctor")
        .env("AIFO_CODER_FORK_STALE_DAYS", "1")
        .env("PATH", "")
        .current_dir(&root)
        .output()
        .expect("run aifo-coder doctor");
    assert!(out.status.success(), "doctor should succeed");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("Found ") && err.contains("old fork sessions"),
        "stale notice should be printed during non-maintenance runs; stderr: {}",
        err
    );
}
