use std::process::Command;
mod support;
use support::{have_git, init_repo_with_default_user};

fn init_repo(dir: &std::path::Path) {
    let _ = init_repo_with_default_user(dir);
}

#[test]
fn test_fork_clean_protects_ahead_and_force_deletes() {
    if !have_git() {
        eprintln!("skipping: git not found in PATH");
        return;
    }
    let td = tempfile::tempdir().expect("tmpdir");
    let root = td.path().to_path_buf();

    // Session/pane setup
    let sid = "sid-ahead";
    let base = root.join(".aifo-coder").join("forks").join(sid);
    let pane = base.join("pane-1");
    std::fs::create_dir_all(&pane).unwrap();
    init_repo(&pane);

    // Record base_commit_sha as current HEAD
    let head = Command::new("git")
        .args(["rev-parse", "--verify", "HEAD"])
        .current_dir(&pane)
        .output()
        .unwrap();
    let head_sha = String::from_utf8_lossy(&head.stdout).trim().to_string();

    // Write minimal meta.json
    std::fs::create_dir_all(&base).unwrap();
    let meta = format!(
        "{{ \"created_at\": {}, \"base_label\": \"main\", \"base_ref_or_sha\": \"main\", \"base_commit_sha\": \"{}\", \"panes\": 1, \"pane_dirs\": [\"{}\"], \"branches\": [\"fork/main/{sid}-1\"], \"layout\": \"tiled\" }}",
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(),
        head_sha,
        pane.display()
    );
    std::fs::write(base.join(".meta.json"), meta).unwrap();

    // Create an extra commit in the pane to make it "ahead" of base_commit_sha
    std::fs::write(pane.join("new.txt"), "y\n").unwrap();
    assert!(Command::new("git")
        .args(["add", "-A"])
        .current_dir(&pane)
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .args(["commit", "-m", "advance pane"])
        .current_dir(&pane)
        .status()
        .unwrap()
        .success());

    // Default clean should REFUSE because pane is ahead
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
    let code = aifo_coder::fork_clean(&root, &opts_refuse).expect("fork_clean refuse");
    assert_eq!(code, 1, "expected refusal when pane is ahead");
    assert!(base.exists(), "session dir must remain after refusal");

    // keep-dirty should succeed, keep the ahead pane and update meta
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
    let code2 = aifo_coder::fork_clean(&root, &opts_keep).expect("fork_clean keep-dirty");
    assert_eq!(
        code2, 0,
        "keep-dirty should succeed (no deletions if all panes protected)"
    );
    assert!(pane.exists(), "ahead pane should remain");
    let meta2 = std::fs::read_to_string(base.join(".meta.json")).expect("read meta2");
    assert!(
        meta2.contains("\"panes_remaining\""),
        "meta should be updated to include panes_remaining"
    );

    // force should delete the session
    let opts_force = aifo_coder::ForkCleanOpts {
        session: Some(sid.to_string()),
        older_than_days: None,
        all: false,
        dry_run: false,
        yes: true,
        force: true,
        keep_dirty: false,
        json: false,
    };
    let code3 = aifo_coder::fork_clean(&root, &opts_force).expect("fork_clean force");
    assert_eq!(code3, 0, "force should succeed");
    assert!(!base.exists(), "session dir should be removed by force");
}
