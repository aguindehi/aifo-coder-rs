use std::process::Command;
mod support;

#[test]
fn test_fork_clone_and_checkout_panes_lfs_marker_does_not_fail_without_lfs() {
    if !support::have_git() {
        eprintln!("skipping: git not found in PATH");
        return;
    }
    let td = tempfile::tempdir().expect("tmpdir");
    let repo = td.path();

    assert!(Command::new("git")
        .args(["init"])
        .current_dir(repo)
        .status()
        .unwrap()
        .success());
    let _ = Command::new("git")
        .args(["config", "user.name", "AIFO Test"])
        .current_dir(repo)
        .status();
    let _ = Command::new("git")
        .args(["config", "user.email", "aifo@example.com"])
        .current_dir(repo)
        .status();

    std::fs::write(
        repo.join(".gitattributes"),
        "*.bin filter=lfs diff=lfs merge=lfs -text\n",
    )
    .unwrap();
    std::fs::write(repo.join("a.bin"), b"\x00\x01\x02").unwrap();
    assert!(Command::new("git")
        .args(["add", "-A"])
        .current_dir(repo)
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .args(["commit", "-m", "add lfs marker"])
        .current_dir(repo)
        .status()
        .unwrap()
        .success());

    let out = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(repo)
        .output()
        .unwrap();
    let cur_branch = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let base_label = aifo_coder::fork_sanitize_base_label(&cur_branch);

    let res = aifo_coder::fork_clone_and_checkout_panes(
        repo,
        "sid-lfs",
        1,
        &cur_branch,
        &base_label,
        false,
    )
    .expect("clone panes with lfs marker");
    assert_eq!(res.len(), 1);
}
