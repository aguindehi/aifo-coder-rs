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
fn test_fork_base_info_branch_and_detached() {
    if !have_git() {
        eprintln!("skipping: git not found in PATH");
        return;
    }
    let td = tempfile::tempdir().expect("tmpdir");
    let repo = td.path();

    // init repo
    assert!(Command::new("git")
        .args(["init"])
        .current_dir(repo)
        .status()
        .expect("git init")
        .success());

    // configure identity
    let _ = Command::new("git")
        .args(["config", "user.name", "AIFO Test"])
        .current_dir(repo)
        .status();
    let _ = Command::new("git")
        .args(["config", "user.email", "aifo@example.com"])
        .current_dir(repo)
        .status();

    // make initial commit
    std::fs::write(repo.join("README.md"), "hello\n").expect("write");
    assert!(Command::new("git")
        .args(["add", "-A"])
        .current_dir(repo)
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(repo)
        .status()
        .unwrap()
        .success());

    // verify base info on branch
    let (label, base, head) = aifo_coder::fork_base_info(repo).expect("base info");
    assert!(!head.is_empty(), "HEAD sha must be non-empty");
    assert!(
        base == "master" || base == "main",
        "expected base to be current branch name, got {}",
        base
    );
    assert!(
        label == "master" || label == "main",
        "expected label to match sanitized branch name, got {}",
        label
    );

    // detached
    assert!(Command::new("git")
        .args(["checkout", "--detach", "HEAD"])
        .current_dir(repo)
        .status()
        .unwrap()
        .success());
    let (label2, base2, head2) = aifo_coder::fork_base_info(repo).expect("base info detached");
    assert_eq!(label2, "detached");
    assert_eq!(base2, head2);
}
