use std::process::Command;
mod support;
use support::{have_git, init_repo_with_default_user};

fn init_repo(dir: &std::path::Path) {
    let _ = init_repo_with_default_user(dir);
}

#[test]
fn test_fork_autoclean_removes_only_clean_sessions() {
    if !have_git() {
        eprintln!("skipping: git not found in PATH");
        return;
    }
    let td = tempfile::tempdir().expect("tmpdir");
    let root = td.path().to_path_buf();
    init_repo(&root);

    // Old clean session
    let sid_clean = "sid-clean-old";
    let base_clean = root.join(".aifo-coder").join("forks").join(sid_clean);
    let pane_clean = base_clean.join("pane-1");
    std::fs::create_dir_all(&pane_clean).unwrap();
    init_repo(&pane_clean);
    let head_clean = String::from_utf8_lossy(
        &Command::new("git")
            .args(["rev-parse", "--verify", "HEAD"])
            .current_dir(&pane_clean)
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
        - 40 * 86400;
    std::fs::create_dir_all(&base_clean).unwrap();
    let meta_clean = format!(
        "{{ \"created_at\": {}, \"base_label\": \"main\", \"base_ref_or_sha\": \"main\", \"base_commit_sha\": \"{}\", \"panes\": 1, \"pane_dirs\": [\"{}\"], \"branches\": [\"fork/main/{sid}-1\"], \"layout\": \"tiled\" }}",
        old_secs, head_clean, pane_clean.display(), sid = sid_clean
    );
    std::fs::write(base_clean.join(".meta.json"), meta_clean).unwrap();

    // Old protected (ahead) session
    let sid_prot = "sid-protected-old";
    let base_prot = root.join(".aifo-coder").join("forks").join(sid_prot);
    let pane_prot = base_prot.join("pane-1");
    std::fs::create_dir_all(&pane_prot).unwrap();
    init_repo(&pane_prot);
    let head_prot = String::from_utf8_lossy(
        &Command::new("git")
            .args(["rev-parse", "--verify", "HEAD"])
            .current_dir(&pane_prot)
            .output()
            .unwrap()
            .stdout,
    )
    .trim()
    .to_string();
    std::fs::create_dir_all(&base_prot).unwrap();
    let meta_prot = format!(
        "{{ \"created_at\": {}, \"base_label\": \"main\", \"base_ref_or_sha\": \"main\", \"base_commit_sha\": \"{}\", \"panes\": 1, \"pane_dirs\": [\"{}\"], \"branches\": [\"fork/main/{sid}-1\"], \"layout\": \"tiled\" }}",
        old_secs, head_prot, pane_prot.display(), sid = sid_prot
    );
    std::fs::write(base_prot.join(".meta.json"), meta_prot).unwrap();
    // Make pane ahead of base_commit_sha
    std::fs::write(pane_prot.join("new.txt"), "y\n").unwrap();
    assert!(Command::new("git")
        .args(["add", "-A"])
        .current_dir(&pane_prot)
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .args(["commit", "-m", "advance pane"])
        .current_dir(&pane_prot)
        .status()
        .unwrap()
        .success());

    // Run autoclean with threshold 1 day
    let old_cwd = std::env::current_dir().unwrap();
    std::env::set_current_dir(&root).unwrap();
    let old_env1 = std::env::var("AIFO_CODER_FORK_AUTOCLEAN").ok();
    let old_env2 = std::env::var("AIFO_CODER_FORK_STALE_DAYS").ok();
    std::env::set_var("AIFO_CODER_FORK_AUTOCLEAN", "1");
    std::env::set_var("AIFO_CODER_FORK_STALE_DAYS", "1");
    aifo_coder::fork_autoclean_if_enabled();
    // Restore cwd and env
    std::env::set_current_dir(old_cwd).ok();
    if let Some(v) = old_env1 {
        std::env::set_var("AIFO_CODER_FORK_AUTOCLEAN", v);
    } else {
        std::env::remove_var("AIFO_CODER_FORK_AUTOCLEAN");
    }
    if let Some(v) = old_env2 {
        std::env::set_var("AIFO_CODER_FORK_STALE_DAYS", v);
    } else {
        std::env::remove_var("AIFO_CODER_FORK_STALE_DAYS");
    }

    assert!(
        !base_clean.exists(),
        "clean old session should have been deleted by autoclean"
    );
    assert!(
        base_prot.exists(),
        "protected old session should have been kept by autoclean"
    );
}
