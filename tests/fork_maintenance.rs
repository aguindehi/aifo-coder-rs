use std::fs;
use std::path::PathBuf;

fn have_git() -> bool {
    std::process::Command::new("git")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

// Helper: initialize a minimal git repo with one commit
fn init_repo(dir: &PathBuf) {
    let _ = std::process::Command::new("git").arg("init").current_dir(dir).status();
    let _ = std::process::Command::new("git").args(["config","user.name","UT"]).current_dir(dir).status();
    let _ = std::process::Command::new("git").args(["config","user.email","ut@example.com"]).current_dir(dir).status();
    fs::write(dir.join("init.txt"), "x\n").unwrap();
    let _ = std::process::Command::new("git").args(["add","-A"]).current_dir(dir).status();
    let _ = std::process::Command::new("git").args(["commit","-m","init"]).current_dir(dir).status();
}

#[test]
fn test_fork_clean_refuses_when_protected_without_overrides() {
    if !have_git() {
        eprintln!("skipping: git not found in PATH");
        return;
    }
    let td = tempfile::tempdir().expect("tmpdir");
    // Set up a base git repo to act as current repo
    let root = td.path().to_path_buf();
    init_repo(&root);

    // Create a fork session with one pane that is dirty
    let sid = "sid1";
    let base = root.join(".aifo-coder").join("forks").join(sid);
    let pane1 = base.join("pane-1");
    fs::create_dir_all(&pane1).unwrap();
    init_repo(&pane1);

    // Write .meta.json with base_commit_sha matching current HEAD
    let head = std::process::Command::new("git")
        .args(["rev-parse","--verify","HEAD"])
        .current_dir(&pane1)
        .output().unwrap();
    let head_sha = String::from_utf8_lossy(&head.stdout).trim().to_string();
    let meta = format!(
        "{{ \"created_at\": {}, \"base_label\": \"main\", \"base_ref_or_sha\": \"main\", \"base_commit_sha\": \"{}\", \"panes\": 1, \"pane_dirs\": [\"{}\"], \"branches\": [\"fork/main/{sid}-1\"], \"layout\": \"tiled\" }}",
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(),
        head_sha,
        pane1.display()
    );
    fs::create_dir_all(&base).unwrap();
    fs::write(base.join(".meta.json"), meta).unwrap();

    // Make the pane dirty
    fs::write(pane1.join("dirty.txt"), "d\n").unwrap();

    // Run fork_clean without overrides: should refuse (return code 1)
    let opts = aifo_coder::ForkCleanOpts {
        session: Some(sid.to_string()),
        older_than_days: None,
        all: false,
        dry_run: false,
        yes: false,
        force: false,
        keep_dirty: false,
        json: false,
    };
    let code = aifo_coder::fork_clean(&root, &opts).expect("fork_clean");
    assert_eq!(code, 1, "expected refusal when pane is protected");
    assert!(base.exists(), "session dir must remain");
}

#[test]
fn test_fork_clean_keep_dirty_removes_only_clean_panes_and_updates_meta() {
    if !have_git() {
        eprintln!("skipping: git not found in PATH");
        return;
    }
    let td = tempfile::tempdir().expect("tmpdir");
    let root = td.path().to_path_buf();
    init_repo(&root);

    let sid = "sid2";
    let base = root.join(".aifo-coder").join("forks").join(sid);
    let pane_clean = base.join("pane-1");
    let pane_dirty = base.join("pane-2");
    fs::create_dir_all(&pane_clean).unwrap();
    fs::create_dir_all(&pane_dirty).unwrap();
    init_repo(&pane_clean);
    init_repo(&pane_dirty);

    // HEAD sha
    let head_clean = String::from_utf8_lossy(&std::process::Command::new("git")
        .args(["rev-parse","--verify","HEAD"]).current_dir(&pane_clean).output().unwrap().stdout).trim().to_string();

    // Make pane-2 dirty
    fs::write(pane_dirty.join("dirty.txt"), "d\n").unwrap();

    // Meta with both panes; base_commit set to head_clean (both at same HEAD initially)
    let meta = format!(
        "{{ \"created_at\": {}, \"base_label\": \"main\", \"base_ref_or_sha\": \"main\", \"base_commit_sha\": \"{}\", \"panes\": 2, \"pane_dirs\": [\"{}\",\"{}\"], \"branches\": [\"fork/main/{sid}-1\",\"fork/main/{sid}-2\"], \"layout\": \"tiled\" }}",
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(),
        head_clean,
        pane_clean.display(),
        pane_dirty.display()
    );
    fs::create_dir_all(&base).unwrap();
    fs::write(base.join(".meta.json"), meta).unwrap();

    let opts = aifo_coder::ForkCleanOpts {
        session: Some(sid.to_string()),
        older_than_days: None,
        all: false,
        dry_run: false,
        yes: true,   // skip prompt in CI
        force: false,
        keep_dirty: true,
        json: false,
    };
    let code = aifo_coder::fork_clean(&root, &opts).expect("fork_clean keep-dirty");
    assert_eq!(code, 0, "keep-dirty should succeed");
    assert!(!pane_clean.exists(), "clean pane should be deleted");
    assert!(pane_dirty.exists(), "dirty pane should remain");
    // Meta should still exist and list remaining pane-2 directory
    let meta2 = fs::read_to_string(base.join(".meta.json")).expect("read meta");
    assert!(meta2.contains("panes_remaining"), "meta should contain panes_remaining");
    assert!(meta2.contains(&format!("\"{}\"", pane_dirty.display())), "meta should include remaining pane dir");
}

#[test]
fn test_fork_autoclean_deletes_old_clean_session() {
    if !have_git() {
        eprintln!("skipping: git not found in PATH");
        return;
    }
    let td = tempfile::tempdir().expect("tmpdir");
    let root = td.path().to_path_buf();
    init_repo(&root);

    // Create one clean session older than threshold
    let sid = "sid-old";
    let base = root.join(".aifo-coder").join("forks").join(sid);
    let pane = base.join("pane-1");
    fs::create_dir_all(&pane).unwrap();
    init_repo(&pane);
    let head = String::from_utf8_lossy(&std::process::Command::new("git")
        .args(["rev-parse","--verify","HEAD"]).current_dir(&pane).output().unwrap().stdout).trim().to_string();
    let old_secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() - 40 * 86400;
    let meta = format!(
        "{{ \"created_at\": {}, \"base_label\": \"main\", \"base_ref_or_sha\": \"main\", \"base_commit_sha\": \"{}\", \"panes\": 1, \"pane_dirs\": [\"{}\"], \"branches\": [\"fork/main/{sid}-1\"], \"layout\": \"tiled\" }}",
        old_secs,
        head,
        pane.display()
    );
    fs::create_dir_all(&base).unwrap();
    fs::write(base.join(".meta.json"), meta).unwrap();

    // Ensure env enables autoclean with threshold 30 days
    std::env::set_var("AIFO_CODER_FORK_AUTOCLEAN", "1");
    std::env::set_var("AIFO_CODER_FORK_STALE_DAYS", "30");

    // Change CWD into repo so repo_root() can find it
    let old_cwd = std::env::current_dir().unwrap();
    std::env::set_current_dir(&root).unwrap();
    aifo_coder::fork_autoclean_if_enabled();
    std::env::set_current_dir(old_cwd).unwrap();

    assert!(!base.exists(), "old clean session should have been auto-removed");
}
