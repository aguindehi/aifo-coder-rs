use std::process::Command;
mod support;
use support::have_git;


#[test]
fn test_fork_merge_octopus_conflict_sets_meta_and_leaves_branches() {
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
    std::fs::write(repo.join("seed.txt"), "seed\n").unwrap();
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

    let sid = "sid-merge-oct-conflict";
    let clones =
        aifo_coder::fork_clone_and_checkout_panes(repo, sid, 2, &cur_branch, &base_label, false)
            .expect("clone panes");
    assert_eq!(clones.len(), 2, "expected two panes");

    // Pane 1 writes conflict.txt
    {
        let (pane_dir, _) = &clones[0];
        std::fs::write(pane_dir.join("conflict.txt"), "A\n").unwrap();
        let _ = Command::new("git")
            .args(["config", "user.name", "AIFO Test"])
            .current_dir(pane_dir)
            .status();
        let _ = Command::new("git")
            .args(["config", "user.email", "aifo@example.com"])
            .current_dir(pane_dir)
            .status();
        assert!(Command::new("git")
            .args(["add", "-A"])
            .current_dir(pane_dir)
            .status()
            .unwrap()
            .success());
        assert!(Command::new("git")
            .args(["commit", "-m", "pane1"])
            .current_dir(pane_dir)
            .status()
            .unwrap()
            .success());
    }
    // Pane 2 writes conflicting content
    {
        let (pane_dir, _) = &clones[1];
        std::fs::write(pane_dir.join("conflict.txt"), "B\n").unwrap();
        let _ = Command::new("git")
            .args(["config", "user.name", "AIFO Test"])
            .current_dir(pane_dir)
            .status();
        let _ = Command::new("git")
            .args(["config", "user.email", "aifo@example.com"])
            .current_dir(pane_dir)
            .status();
        assert!(Command::new("git")
            .args(["add", "-A"])
            .current_dir(pane_dir)
            .status()
            .unwrap()
            .success());
        assert!(Command::new("git")
            .args(["commit", "-m", "pane2"])
            .current_dir(pane_dir)
            .status()
            .unwrap()
            .success());
    }

    let res = aifo_coder::fork_merge_branches_by_session(
        repo,
        sid,
        aifo_coder::MergingStrategy::Octopus,
        true,
        false,
    );
    assert!(res.is_err(), "octopus merge should fail due to conflicts");

    let meta_path = repo
        .join(".aifo-coder")
        .join("forks")
        .join(sid)
        .join(".meta.json");
    let meta = std::fs::read_to_string(&meta_path).expect("read meta");
    assert!(
        meta.contains("\"merge_failed\":true"),
        "meta should include merge_failed:true, got: {}",
        meta
    );

    for (_pane_dir, branch) in &clones {
        let ok = Command::new("git")
            .args(["show-ref", "--verify", &format!("refs/heads/{}", branch)])
            .current_dir(repo)
            .status()
            .unwrap()
            .success();
        assert!(
            ok,
            "pane branch '{}' should exist after failed merge",
            branch
        );
    }

    let out2 = Command::new("git")
        .args(["ls-files", "-u"])
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(
        !out2.stdout.is_empty(),
        "expected unmerged paths after failed octopus merge"
    );
}
