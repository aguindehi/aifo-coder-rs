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
fn test_fork_clean_planning_json_and_execution_summary() {
    // Prepare temp repo with one session having two panes: one clean and one protected (ahead)
    let td = tempfile::tempdir().expect("tmpdir");
    let root = td.path().to_path_buf();
    init_repo(&root);

    let sid = "sid-plan";
    let forks = root.join(".aifo-coder").join("forks");
    let sd = forks.join(sid);
    let pane1 = sd.join("pane-1"); // clean
    let pane2 = sd.join("pane-2"); // ahead
    std::fs::create_dir_all(&pane1).unwrap();
    std::fs::create_dir_all(&pane2).unwrap();
    init_repo(&pane1);
    init_repo(&pane2);

    // base_commit_sha as current HEAD for both
    let head1 = String::from_utf8_lossy(
        &std::process::Command::new("git")
            .args(["rev-parse", "--verify", "HEAD"])
            .current_dir(&pane1)
            .output()
            .unwrap()
            .stdout,
    )
    .trim()
    .to_string();

    // make pane2 "ahead"
    std::fs::write(pane2.join("new.txt"), "y\n").unwrap();
    assert!(std::process::Command::new("git")
        .args(["add", "-A"])
        .current_dir(&pane2)
        .status()
        .unwrap()
        .success());
    assert!(std::process::Command::new("git")
        .args(["commit", "-m", "advance pane2"])
        .current_dir(&pane2)
        .status()
        .unwrap()
        .success());

    std::fs::create_dir_all(&sd).unwrap();
    let meta = format!(
        "{{ \"created_at\": {}, \"base_label\": \"main\", \"base_ref_or_sha\": \"main\", \"base_commit_sha\": \"{}\", \"panes\": 2, \"pane_dirs\": [\"{}\",\"{}\"], \"branches\": [\"fork/main/{}-1\",\"fork/main/{}-2\"], \"layout\": \"tiled\" }}",
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(),
        head1,
        pane1.display(),
        pane2.display(),
        sid, sid
    );
    std::fs::write(sd.join(".meta.json"), meta).unwrap();

    let bin = env!("CARGO_BIN_EXE_aifo-coder");

    // Dry-run planning JSON
    let out = Command::new(bin)
        .args(["fork", "clean", "--session", sid, "--dry-run", "--json"])
        .current_dir(&root)
        .output()
        .expect("run aifo-coder fork clean planning json");
    assert!(
        out.status.success(),
        "planning json should succeed; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(
        s.contains("\"plan\":true"),
        "planning output must include plan=true: {}",
        s
    );
    assert!(
        s.contains("\"panes_total\":2"),
        "planning output should include panes_total=2: {}",
        s
    );
    assert!(
        s.contains("\"panes_clean\":1"),
        "planning output should include panes_clean=1: {}",
        s
    );
    assert!(
        s.contains("\"panes_protected\":1"),
        "planning output should include panes_protected=1: {}",
        s
    );

    // Dry-run planning JSON with --keep-dirty should indicate will_delete_session=false
    let out_kd = Command::new(bin)
        .args(["fork", "clean", "--session", sid, "--keep-dirty", "--dry-run", "--json"])
        .current_dir(&root)
        .output()
        .expect("run aifo-coder fork clean planning json keep-dirty");
    assert!(
        out_kd.status.success(),
        "planning json with --keep-dirty should succeed; stderr={}",
        String::from_utf8_lossy(&out_kd.stderr)
    );
    let s_kd = String::from_utf8_lossy(&out_kd.stdout);
    assert!(
        s_kd.contains("\"will_delete_session\":false"),
        "keep-dirty plan should NOT delete whole session: {}",
        s_kd
    );

    // Executed JSON summary with --force
    let out2 = Command::new(bin)
        .args(["fork", "clean", "--session", sid, "--force", "--yes", "--json"])
        .current_dir(&root)
        .output()
        .expect("run aifo-coder fork clean execution json");
    assert!(
        out2.status.success(),
        "execution json should succeed; stderr={}",
        String::from_utf8_lossy(&out2.stderr)
    );
    let s2 = String::from_utf8_lossy(&out2.stdout);
    assert!(
        s2.contains("\"executed\":true"),
        "execution output should include executed=true: {}",
        s2
    );
    assert!(
        s2.contains("\"deleted_sessions\":1"),
        "execution output should report deleted_sessions=1: {}",
        s2
    );
}
