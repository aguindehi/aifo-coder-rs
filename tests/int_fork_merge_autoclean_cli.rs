use std::process::Command;
mod support;
use support::have_git;

#[test]
fn int_test_cli_fork_merge_octopus_autoclean_disposes_session() {
    if !have_git() {
        eprintln!("skipping: git not found in PATH");
        return;
    }
    let td = tempfile::tempdir().expect("tmpdir");
    let repo = td.path();

    // init repo with one commit on default branch
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

    // Determine current branch name and base label
    let out = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(repo)
        .output()
        .unwrap();
    let cur_branch = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let base_label = aifo_coder::fork_sanitize_base_label(&cur_branch);

    // Create a fork session with two panes and non-conflicting commits
    let sid = "cli-merge-octopus-autoclean";
    let clones =
        aifo_coder::fork_clone_and_checkout_panes(repo, sid, 2, &cur_branch, &base_label, false)
            .expect("clone panes");
    assert_eq!(clones.len(), 2, "expected two panes");

    for (idx, (pane_dir, _)) in clones.iter().enumerate() {
        let fname = format!("pane-autoclean-{}.txt", idx + 1);
        std::fs::write(pane_dir.join(&fname), format!("ok {}\n", idx + 1)).unwrap();
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
            .args(["commit", "-m", &format!("pane ok {}", idx + 1)])
            .current_dir(pane_dir)
            .status()
            .unwrap()
            .success());
    }

    // Run CLI subcommand: fork merge --strategy octopus --autoclean
    let bin = env!("CARGO_BIN_EXE_aifo-coder");
    let out_cli = Command::new(bin)
        .args([
            "fork",
            "merge",
            "--session",
            sid,
            "--strategy",
            "octopus",
            "--autoclean",
        ])
        .current_dir(repo)
        .output()
        .expect("run aifo-coder fork merge octopus --autoclean");
    assert!(
        out_cli.status.success(),
        "fork merge octopus --autoclean failed: stdout={} stderr={}",
        String::from_utf8_lossy(&out_cli.stdout),
        String::from_utf8_lossy(&out_cli.stderr)
    );

    // Verify we returned to original branch
    let out2 = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(repo)
        .output()
        .unwrap();
    let head_branch = String::from_utf8_lossy(&out2.stdout).trim().to_string();
    assert_eq!(
        head_branch, cur_branch,
        "expected HEAD to return to base branch"
    );

    // Verify session directory is removed
    let session_dir = repo.join(".aifo-coder").join("forks").join(sid);
    assert!(
        !session_dir.exists(),
        "session dir should be removed by autoclean: {}",
        session_dir.display()
    );
}
