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
fn test_fork_clean_cli_safety_and_flags() {
    // Prepare temp repo and a protected session (ahead)
    let td = tempfile::tempdir().expect("tmpdir");
    let root = td.path().to_path_buf();
    init_repo(&root);

    let sid = "sid-cli-prot";
    let forks = root.join(".aifo-coder").join("forks");
    let sd = forks.join(sid);
    let pane = sd.join("pane-1");
    std::fs::create_dir_all(&pane).unwrap();
    init_repo(&pane);
    // Record base_commit_sha as current HEAD
    let head = String::from_utf8_lossy(
        &std::process::Command::new("git")
            .args(["rev-parse", "--verify", "HEAD"])
            .current_dir(&pane)
            .output()
            .unwrap()
            .stdout,
    )
    .trim()
    .to_string();
    std::fs::create_dir_all(&sd).unwrap();
    let meta = format!(
        "{{ \"created_at\": {}, \"base_label\": \"main\", \"base_ref_or_sha\": \"main\", \"base_commit_sha\": \"{}\", \"panes\": 1, \"pane_dirs\": [\"{}\"], \"branches\": [\"fork/main/{}-1\"], \"layout\": \"tiled\" }}",
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(),
        head, pane.display(), sid
    );
    std::fs::write(sd.join(".meta.json"), meta).unwrap();
    // Make pane ahead (commit one more)
    std::fs::write(pane.join("new.txt"), "y\n").unwrap();
    assert!(std::process::Command::new("git")
        .args(["add", "-A"])
        .current_dir(&pane)
        .status()
        .unwrap()
        .success());
    assert!(std::process::Command::new("git")
        .args(["commit", "-m", "advance pane"])
        .current_dir(&pane)
        .status()
        .unwrap()
        .success());

    let bin = env!("CARGO_BIN_EXE_aifo-coder");

    // 1) Default refusal
    let out = Command::new(bin)
        .args(["fork", "clean", "--session", sid])
        .current_dir(&root)
        .output()
        .expect("run fork clean");
    assert_eq!(
        out.status.code(),
        Some(1),
        "expected refusal exit=1; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(sd.exists(), "session dir should remain after refusal");

    // 2) keep-dirty should succeed and keep pane; meta should contain panes_remaining
    let out2 = Command::new(bin)
        .args(["fork", "clean", "--session", sid, "--keep-dirty", "--yes"])
        .current_dir(&root)
        .output()
        .expect("run fork clean --keep-dirty");
    assert!(
        out2.status.success(),
        "keep-dirty should succeed; stderr={}",
        String::from_utf8_lossy(&out2.stderr)
    );
    assert!(pane.exists(), "ahead pane should remain after keep-dirty");
    let meta2 = std::fs::read_to_string(sd.join(".meta.json")).expect("read meta2");
    assert!(
        meta2.contains("\"panes_remaining\""),
        "meta.json should be updated with panes_remaining"
    );

    // 3) force should delete session
    let out3 = Command::new(bin)
        .args(["fork", "clean", "--session", sid, "--force", "--yes"])
        .current_dir(&root)
        .output()
        .expect("run fork clean --force");
    assert!(
        out3.status.success(),
        "force should succeed; stderr={}",
        String::from_utf8_lossy(&out3.stderr)
    );
    assert!(!sd.exists(), "session dir should be removed by force");
}
