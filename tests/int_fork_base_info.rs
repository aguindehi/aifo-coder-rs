use std::process::Command;
mod support;
use support::{have_git, init_repo_with_default_user};

#[test]
fn int_test_fork_base_info_branch_and_detached() {
    if !have_git() {
        eprintln!("skipping: git not found in PATH");
        return;
    }
    let td = tempfile::tempdir().expect("tmpdir");
    let repo = td.path();

    // init repo
    let _ = init_repo_with_default_user(repo);

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
