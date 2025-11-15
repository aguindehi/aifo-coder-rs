use std::process::Command;
mod support;
use support::have_git;

#[test]
fn int_test_fork_clone_and_checkout_panes_creates_branches() {
    if !have_git() {
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
    std::fs::write(repo.join("file.txt"), "x\n").unwrap();
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

    let out = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(repo)
        .output()
        .unwrap();
    let cur_branch = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let base_label = aifo_coder::fork_sanitize_base_label(&cur_branch);

    let sid = "forksid";
    let res =
        aifo_coder::fork_clone_and_checkout_panes(repo, sid, 2, &cur_branch, &base_label, false)
            .expect("clone panes");
    assert_eq!(res.len(), 2, "expected two panes");

    // Verify branches are checked out in panes
    for (idx, (pane_dir, branch)) in res.iter().enumerate() {
        assert!(
            pane_dir.exists(),
            "pane dir must exist: {}",
            pane_dir.display()
        );
        let out = Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(pane_dir)
            .output()
            .unwrap();
        let head_branch = String::from_utf8_lossy(&out.stdout).trim().to_string();
        assert_eq!(
            &head_branch,
            branch,
            "pane {} HEAD should be {}",
            idx + 1,
            branch
        );
    }
}

#[test]
fn int_test_fork_clone_with_dissociate() {
    if !have_git() {
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
    std::fs::write(repo.join("f.txt"), "x\n").unwrap();
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
    let out = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(repo)
        .output()
        .unwrap();
    let cur_branch = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let base_label = aifo_coder::fork_sanitize_base_label(&cur_branch);
    let res = aifo_coder::fork_clone_and_checkout_panes(
        repo,
        "sid-dissoc",
        1,
        &cur_branch,
        &base_label,
        true,
    )
    .expect("clone with --dissociate");
    assert_eq!(res.len(), 1);
    assert!(res[0].0.exists());
}
