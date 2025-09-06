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
fn test_cli_fork_merge_fetch_e2e() {
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

    // Create a fork session with two panes and make commits
    let sid = "cli-merge-fetch";
    let clones =
        aifo_coder::fork_clone_and_checkout_panes(repo, sid, 2, &cur_branch, &base_label, false)
            .expect("clone panes");
    assert_eq!(clones.len(), 2, "expected two panes");

    for (idx, (pane_dir, _br)) in clones.iter().enumerate() {
        let fname = format!("pane-fetch-{}.txt", idx + 1);
        std::fs::write(pane_dir.join(&fname), format!("pane {}\n", idx + 1)).unwrap();
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
            .args(["commit", "-m", &format!("pane {}", idx + 1)])
            .current_dir(pane_dir)
            .status()
            .unwrap()
            .success());
    }

    // Run CLI subcommand: fork merge --session ... --strategy fetch
    let bin = env!("CARGO_BIN_EXE_aifo-coder");
    let out_cli = Command::new(bin)
        .args(["fork", "merge", "--session", sid, "--strategy", "fetch"])
        .current_dir(repo)
        .output()
        .expect("run aifo-coder fork merge fetch");
    assert!(
        out_cli.status.success(),
        "fork merge fetch failed: stdout={} stderr={}",
        String::from_utf8_lossy(&out_cli.stdout),
        String::from_utf8_lossy(&out_cli.stderr)
    );

    // Verify branches exist in the original repo
    for (_pane_dir, branch) in &clones {
        let ok = Command::new("git")
            .args(["rev-parse", "--verify", branch])
            .current_dir(repo)
            .status()
            .unwrap()
            .success();
        assert!(ok, "expected branch '{}' to exist in original repo", branch);
    }

    // Verify metadata contains merge_strategy=fetch
    let meta_path = repo
        .join(".aifo-coder")
        .join("forks")
        .join(sid)
        .join(".meta.json");
    let meta = std::fs::read_to_string(&meta_path).expect("read meta");
    assert!(
        meta.contains("\"merge_strategy\"") && meta.contains("fetch"),
        "meta should include merge_strategy=fetch, got: {}",
        meta
    );
}

#[test]
fn test_cli_fork_merge_octopus_e2e() {
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
    let sid = "cli-merge-octopus";
    let clones =
        aifo_coder::fork_clone_and_checkout_panes(repo, sid, 2, &cur_branch, &base_label, false)
            .expect("clone panes");
    assert_eq!(clones.len(), 2, "expected two panes");

    for (idx, (pane_dir, _)) in clones.iter().enumerate() {
        let fname = format!("pane-oct-{}.txt", idx + 1);
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

    // Run CLI subcommand: fork merge --session ... --strategy octopus
    let bin = env!("CARGO_BIN_EXE_aifo-coder");
    let out_cli = Command::new(bin)
        .args(["fork", "merge", "--session", sid, "--strategy", "octopus"])
        .current_dir(repo)
        .output()
        .expect("run aifo-coder fork merge octopus");
    assert!(
        out_cli.status.success(),
        "fork merge octopus failed: stdout={} stderr={}",
        String::from_utf8_lossy(&out_cli.stdout),
        String::from_utf8_lossy(&out_cli.stderr)
    );

    // Verify we are on merge/<sid>
    let out2 = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(repo)
        .output()
        .unwrap();
    let head_branch = String::from_utf8_lossy(&out2.stdout).trim().to_string();
    assert_eq!(
        head_branch,
        format!("merge/{}", sid),
        "expected HEAD to be merge/<sid>"
    );

    // Verify pane branches are deleted from original repo
    for (_pane_dir, branch) in &clones {
        let ok = Command::new("git")
            .args(["show-ref", "--verify", &format!("refs/heads/{}", branch)])
            .current_dir(repo)
            .status()
            .unwrap()
            .success();
        assert!(
            !ok,
            "pane branch '{}' should be deleted after octopus merge",
            branch
        );
    }

    // Verify metadata contains merge_target and merge_commit_sha
    let meta_path = repo
        .join(".aifo-coder")
        .join("forks")
        .join(sid)
        .join(".meta.json");
    let meta = std::fs::read_to_string(&meta_path).expect("read meta");
    assert!(
        meta.contains("\"merge_target\"") && meta.contains(&format!("merge/{}", sid)),
        "meta should include merge_target=merge/<sid>: {}",
        meta
    );
    assert!(
        meta.contains("\"merge_commit_sha\""),
        "meta should include merge_commit_sha: {}",
        meta
    );
}
