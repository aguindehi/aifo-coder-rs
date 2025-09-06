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

#[test]
fn test_cli_fork_clean_refusal_colorized() {
    if !have_git() {
        eprintln!("skipping: git not found in PATH");
        return;
    }
    let td = tempfile::tempdir().expect("tmpdir");
    let repo = td.path();

    // Initialize base repo (required by CLI to detect repo_root)
    assert!(Command::new("git").args(["init"]).current_dir(repo).status().unwrap().success());

    // Create a fork session with one pane initialized as a repo
    let sid = "sid-color-clean";
    let base = repo.join(".aifo-coder").join("forks").join(sid);
    let pane = base.join("pane-1");
    std::fs::create_dir_all(&pane).unwrap();
    // init pane repo with one commit
    assert!(Command::new("git").args(["init"]).current_dir(&pane).status().unwrap().success());
    let _ = Command::new("git").args(["config","user.name","UT"]).current_dir(&pane).status();
    let _ = Command::new("git").args(["config","user.email","ut@example.com"]).current_dir(&pane).status();
    std::fs::write(pane.join("a.txt"), "a\n").unwrap();
    assert!(Command::new("git").args(["add","-A"]).current_dir(&pane).status().unwrap().success());
    assert!(Command::new("git").args(["commit","-m","init"]).current_dir(&pane).status().unwrap().success());

    // Record base_commit_sha as current HEAD
    let head = Command::new("git").args(["rev-parse","--verify","HEAD"]).current_dir(&pane).output().unwrap();
    let head_sha = String::from_utf8_lossy(&head.stdout).trim().to_string();

    // Minimal session meta
    std::fs::create_dir_all(&base).unwrap();
    let meta = format!(
        "{{ \"created_at\": {}, \"base_label\": \"main\", \"base_ref_or_sha\": \"main\", \"base_commit_sha\": \"{}\", \"panes\": 1, \"pane_dirs\": [\"{}\"], \"branches\": [\"fork/main/{sid}-1\"], \"layout\": \"tiled\" }}",
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(),
        head_sha,
        pane.display()
    );
    std::fs::write(base.join(".meta.json"), meta).unwrap();

    // Advance pane to make it 'ahead'
    std::fs::write(pane.join("b.txt"), "b\n").unwrap();
    assert!(Command::new("git").args(["add","-A"]).current_dir(&pane).status().unwrap().success());
    assert!(Command::new("git").args(["commit","-m","advance pane"]).current_dir(&pane).status().unwrap().success());

    // Run fork clean (should refuse) with color=always
    let bin = env!("CARGO_BIN_EXE_aifo-coder");
    let out_cli = Command::new(bin)
        .args(["--color","always","fork","clean","--session",sid])
        .current_dir(repo)
        .output()
        .expect("run aifo-coder fork clean");
    assert!(!out_cli.status.success(), "expected refusal (exit non-zero)");
    let stderr = String::from_utf8_lossy(&out_cli.stderr);
    assert!(stderr.contains("\x1b["), "expected ANSI color in refusal stderr, got:\n{}", stderr);
    assert!(stderr.contains("refusing to delete"), "expected refusal message in stderr, got:\n{}", stderr);
}
