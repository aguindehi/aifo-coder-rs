use std::process::Command;
mod support;
use support::have_git;

#[test]
fn int_test_fork_create_snapshot_commit_exists() {
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
    std::fs::write(repo.join("a.txt"), "a\n").unwrap();
    assert!(Command::new("git")
        .args(["add", "-A"])
        .current_dir(repo)
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .args(["commit", "-m", "c1"])
        .current_dir(repo)
        .status()
        .unwrap()
        .success());

    // dirty change
    std::fs::write(repo.join("b.txt"), "b\n").unwrap();

    let sid = "ut";
    let snap = aifo_coder::fork_create_snapshot(repo, sid).expect("snapshot");
    assert_eq!(snap.len(), 40, "snapshot should be a 40-hex sha: {}", snap);

    let out = Command::new("git")
        .arg("cat-file")
        .arg("-t")
        .arg(&snap)
        .current_dir(repo)
        .output()
        .expect("git cat-file");
    let t = String::from_utf8_lossy(&out.stdout).trim().to_string();
    assert_eq!(
        t, "commit",
        "snapshot object type must be commit, got {}",
        t
    );
}

#[test]
fn int_test_fork_create_snapshot_on_empty_repo() {
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
    // Set local author identity for CI where global config may be missing
    let _ = Command::new("git")
        .args(["config", "user.name", "AIFO Test"])
        .current_dir(repo)
        .status();
    let _ = Command::new("git")
        .args(["config", "user.email", "aifo@example.com"])
        .current_dir(repo)
        .status();
    std::fs::write(repo.join("a.txt"), "a\n").unwrap();
    let sid = "empty";
    let snap = aifo_coder::fork_create_snapshot(repo, sid).expect("snapshot on empty repo");
    assert_eq!(snap.len(), 40, "snapshot sha length");
    let out = Command::new("git")
        .args(["cat-file", "-t", &snap])
        .current_dir(repo)
        .output()
        .unwrap();
    let t = String::from_utf8_lossy(&out.stdout).trim().to_string();
    assert_eq!(t, "commit", "snapshot object must be a commit");
}
