use std::process::Command;
mod support;
use support::{have_git, init_repo_with_default_user};

#[test]
fn test_fork_clean_older_than_deletes_only_old_sessions() {
    if !have_git() {
        eprintln!("skipping: git not found in PATH");
        return;
    }
    let td = tempfile::tempdir().expect("tmpdir");
    let root = td.path().to_path_buf();
    let _ = init_repo_with_default_user(&root);

    // Old clean session
    let sid_old = "sid-old2";
    let base_old = root.join(".aifo-coder").join("forks").join(sid_old);
    let pane_old = base_old.join("pane-1");
    std::fs::create_dir_all(&pane_old).unwrap();
    let _ = init_repo_with_default_user(&pane_old);
    let head_old = String::from_utf8_lossy(
        &Command::new("git")
            .args(["rev-parse", "--verify", "HEAD"])
            .current_dir(&pane_old)
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
        - 20 * 86400;
    let meta_old = format!(
        "{{ \"created_at\": {}, \"base_label\": \"main\", \"base_ref_or_sha\": \"main\", \"base_commit_sha\": \"{}\", \"panes\": 1, \"pane_dirs\": [\"{}\"], \"branches\": [\"fork/main/{sid}-1\"], \"layout\": \"tiled\" }}",
        old_secs, head_old, pane_old.display(), sid = sid_old
    );
    std::fs::create_dir_all(&base_old).unwrap();
    std::fs::write(base_old.join(".meta.json"), meta_old).unwrap();

    // Recent clean session
    let sid_new = "sid-new2";
    let base_new = root.join(".aifo-coder").join("forks").join(sid_new);
    let pane_new = base_new.join("pane-1");
    std::fs::create_dir_all(&pane_new).unwrap();
    let _ = init_repo_with_default_user(&pane_new);
    let head_new = String::from_utf8_lossy(
        &Command::new("git")
            .args(["rev-parse", "--verify", "HEAD"])
            .current_dir(&pane_new)
            .output()
            .unwrap()
            .stdout,
    )
    .trim()
    .to_string();
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let meta_new = format!(
        "{{ \"created_at\": {}, \"base_label\": \"main\", \"base_ref_or_sha\": \"main\", \"base_commit_sha\": \"{}\", \"panes\": 1, \"pane_dirs\": [\"{}\"], \"branches\": [\"fork/main/{sid}-1\"], \"layout\": \"tiled\" }}",
        now_secs, head_new, pane_new.display(), sid = sid_new
    );
    std::fs::create_dir_all(&base_new).unwrap();
    std::fs::write(base_new.join(".meta.json"), meta_new).unwrap();

    // Clean with older-than=10 days should delete only sid-old2
    let opts = aifo_coder::ForkCleanOpts {
        session: None,
        older_than_days: Some(10),
        all: false,
        dry_run: false,
        yes: true,
        force: false,
        keep_dirty: false,
        json: false,
    };
    let code = aifo_coder::fork_clean(&root, &opts).expect("fork_clean older-than");
    assert_eq!(code, 0, "older-than clean should succeed");
    assert!(!base_old.exists(), "old session should be deleted");
    assert!(base_new.exists(), "recent session should remain");
}
