use std::process::Command;
use std::thread;
use std::time::Duration;

mod support;
use support::{have_git, init_repo_with_default_user};

fn init_repo(path: &std::path::Path) {
    assert!(Command::new("git")
        .args(["init"])
        .current_dir(path)
        .status()
        .unwrap()
        .success());
    let _ = Command::new("git")
        .args(["config", "user.name", "AIFO Test"])
        .current_dir(path)
        .status();
    let _ = Command::new("git")
        .args(["config", "user.email", "aifo@example.com"])
        .current_dir(path)
        .status();
    std::fs::write(path.join("seed.txt"), "seed\n").unwrap();
    assert!(Command::new("git")
        .args(["add", "-A"])
        .current_dir(path)
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(path)
        .status()
        .unwrap()
        .success());
}

#[test]
fn int_test_fork_merge_lock_serializes_concurrent_merges() {
    if !have_git() {
        eprintln!("skipping: git not found in PATH");
        return;
    }
    let td = tempfile::tempdir().expect("tmpdir");
    let repo = td.path();
    init_repo(repo);

    let head = String::from_utf8_lossy(
        &Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(repo)
            .output()
            .unwrap()
            .stdout,
    )
    .trim()
    .to_string();
    let base_label = aifo_coder::fork_sanitize_base_label(&head);

    // Create two independent fork sessions (simulate concurrent forks).
    let sid1 = "sid-merge-lock-1";
    let sid2 = "sid-merge-lock-2";

    let clones1 =
        aifo_coder::fork_clone_and_checkout_panes(repo, sid1, 1, &head, &base_label, false)
            .expect("clone pane 1");
    let clones2 =
        aifo_coder::fork_clone_and_checkout_panes(repo, sid2, 1, &head, &base_label, false)
            .expect("clone pane 2");

    // Commit a change in each pane.
    for (pane_dir, _branch) in clones1.iter().chain(clones2.iter()) {
        let _ = init_repo_with_default_user(pane_dir);
        std::fs::write(
            pane_dir.join("change.txt"),
            format!("pane at {:?}", pane_dir),
        )
        .unwrap();
        assert!(Command::new("git")
            .args(["add", "-A"])
            .current_dir(pane_dir)
            .status()
            .unwrap()
            .success());
        assert!(Command::new("git")
            .args(["commit", "-m", "pane change"])
            .current_dir(pane_dir)
            .status()
            .unwrap()
            .success());
    }

    // Kick off two merges in parallel; they should serialize via the merge lock.
    let repo1 = repo.to_path_buf();
    let handle1 = thread::spawn(move || {
        aifo_coder::fork_merge_branches_by_session(
            &repo1,
            sid1,
            aifo_coder::MergingStrategy::Fetch,
            true,
            false,
        )
    });

    // Small delay to increase overlap likelihood.
    thread::sleep(Duration::from_millis(50));

    let repo2 = repo.to_path_buf();
    let handle2 = thread::spawn(move || {
        aifo_coder::fork_merge_branches_by_session(
            &repo2,
            sid2,
            aifo_coder::MergingStrategy::Fetch,
            true,
            false,
        )
    });

    let res1 = handle1.join().expect("thread 1 join");
    let res2 = handle2.join().expect("thread 2 join");

    assert!(res1.is_ok(), "first merge should succeed: {:?}", res1.err());
    assert!(
        res2.is_ok(),
        "second merge should succeed: {:?}",
        res2.err()
    );

    // Verify both branches exist in original repo after fetch merges.
    for (_, branch) in clones1.iter().chain(clones2.iter()) {
        let ok = Command::new("git")
            .args(["rev-parse", "--verify", branch])
            .current_dir(repo)
            .status()
            .unwrap()
            .success();
        assert!(ok, "expected branch '{}' to exist in original repo", branch);
    }
}
