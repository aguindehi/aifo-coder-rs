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
fn test_fork_clean_protects_submodule_dirty() {
    if !have_git() {
        eprintln!("skipping: git not found in PATH");
        return;
    }
    let td = tempfile::tempdir().expect("tmpdir");
    let root = td.path().to_path_buf();

    // Prepare submodule upstream repo
    let upstream = td.path().join("upstream-sm");
    std::fs::create_dir_all(&upstream).unwrap();
    assert!(Command::new("git")
        .args(["init"])
        .current_dir(&upstream)
        .status()
        .unwrap()
        .success());
    let _ = Command::new("git")
        .args(["config", "user.name", "UT"])
        .current_dir(&upstream)
        .status();
    let _ = Command::new("git")
        .args(["config", "user.email", "ut@example.com"])
        .current_dir(&upstream)
        .status();
    std::fs::write(upstream.join("a.txt"), "a\n").unwrap();
    assert!(Command::new("git")
        .args(["add", "-A"])
        .current_dir(&upstream)
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .args(["commit", "-m", "sm init"])
        .current_dir(&upstream)
        .status()
        .unwrap()
        .success());

    // Create pane repo and add submodule
    let sid = "sid-subdirty";
    let base = root.join(".aifo-coder").join("forks").join(sid);
    let pane = base.join("pane-1");
    std::fs::create_dir_all(&pane).unwrap();
    assert!(Command::new("git")
        .args(["init"])
        .current_dir(&pane)
        .status()
        .unwrap()
        .success());
    let _ = Command::new("git")
        .args(["config", "user.name", "UT"])
        .current_dir(&pane)
        .status();
    let _ = Command::new("git")
        .args(["config", "user.email", "ut@example.com"])
        .current_dir(&pane)
        .status();
    std::fs::write(pane.join("root.txt"), "r\n").unwrap();
    assert!(Command::new("git")
        .args(["add", "-A"])
        .current_dir(&pane)
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .args(["commit", "-m", "root"])
        .current_dir(&pane)
        .status()
        .unwrap()
        .success());
    let up_path = upstream.display().to_string();
    assert!(Command::new("git")
        .args([
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            &up_path,
            "sub"
        ])
        .current_dir(&pane)
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .args(["commit", "-m", "add submodule"])
        .current_dir(&pane)
        .status()
        .unwrap()
        .success());

    // Record base_commit_sha as current HEAD in pane
    let head = Command::new("git")
        .args(["rev-parse", "--verify", "HEAD"])
        .current_dir(&pane)
        .output()
        .unwrap();
    let head_sha = String::from_utf8_lossy(&head.stdout).trim().to_string();
    std::fs::create_dir_all(&base).unwrap();
    let meta = format!(
        "{{ \"created_at\": {}, \"base_label\": \"main\", \"base_ref_or_sha\": \"main\", \"base_commit_sha\": \"{}\", \"panes\": 1, \"pane_dirs\": [\"{}\"], \"branches\": [\"fork/main/{sid}-1\"], \"layout\": \"tiled\" }}",
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(),
        head_sha,
        pane.display()
    );
    std::fs::write(base.join(".meta.json"), meta).unwrap();

    // Make submodule dirty relative to recorded commit
    let subdir = pane.join("sub");
    let _ = Command::new("git")
        .args(["config", "user.name", "UT"])
        .current_dir(&subdir)
        .status();
    let _ = Command::new("git")
        .args(["config", "user.email", "ut@example.com"])
        .current_dir(&subdir)
        .status();
    std::fs::write(subdir.join("b.txt"), "b\n").unwrap();
    assert!(Command::new("git")
        .args(["add", "-A"])
        .current_dir(&subdir)
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .args(["commit", "-m", "advance sub"])
        .current_dir(&subdir)
        .status()
        .unwrap()
        .success());

    // Default clean should refuse
    let opts_refuse = aifo_coder::ForkCleanOpts {
        session: Some(sid.to_string()),
        older_than_days: None,
        all: false,
        dry_run: false,
        yes: false,
        force: false,
        keep_dirty: false,
        json: false,
    };
    let code =
        aifo_coder::fork_clean(&root, &opts_refuse).expect("fork_clean refuse submodule-dirty");
    assert_eq!(code, 1, "expected refusal when submodule is dirty");
    assert!(base.exists(), "session dir must remain");

    // keep-dirty should keep the pane and succeed
    let opts_keep = aifo_coder::ForkCleanOpts {
        session: Some(sid.to_string()),
        older_than_days: None,
        all: false,
        dry_run: false,
        yes: true,
        force: false,
        keep_dirty: true,
        json: false,
    };
    let code2 = aifo_coder::fork_clean(&root, &opts_keep).expect("fork_clean keep-dirty submodule");
    assert_eq!(code2, 0, "keep-dirty should succeed");
    assert!(pane.exists(), "pane with dirty submodule should remain");
}
