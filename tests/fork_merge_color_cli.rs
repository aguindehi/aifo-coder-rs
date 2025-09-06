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
fn test_cli_fork_merge_octopus_autoclean_color_success() {
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

    // base branch/label
    let out = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(repo)
        .output()
        .unwrap();
    let cur_branch = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let base_label = aifo_coder::fork_sanitize_base_label(&cur_branch);

    // fork two panes and commit non-conflicting changes
    let sid = "merge-color-ok";
    let clones =
        aifo_coder::fork_clone_and_checkout_panes(repo, sid, 2, &cur_branch, &base_label, false)
            .expect("clone");
    for (idx, (pane_dir, _)) in clones.iter().enumerate() {
        let fname = format!("pane-ok-{}.txt", idx + 1);
        std::fs::write(pane_dir.join(&fname), "ok\n").unwrap();
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
            .args(["commit", "-m", "ok"])
            .current_dir(pane_dir)
            .status()
            .unwrap()
            .success());
    }

    // run merge with autoclean and forced color
    let bin = env!("CARGO_BIN_EXE_aifo-coder");
    let out_cli = Command::new(bin)
        .args([
            "--color",
            "always",
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
        .expect("run aifo-coder fork merge");
    assert!(
        out_cli.status.success(),
        "merge failed: stderr={}",
        String::from_utf8_lossy(&out_cli.stderr)
    );

    let stderr = String::from_utf8_lossy(&out_cli.stderr);
    assert!(
        stderr.contains("\x1b["),
        "expected ANSI color in stderr, got:\n{}",
        stderr
    );
    assert!(
        stderr.contains("octopus merge succeeded"),
        "expected success message in stderr, got:\n{}",
        stderr
    );
}

#[test]
fn test_cli_fork_merge_octopus_color_failure() {
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

    // base branch/label
    let out = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(repo)
        .output()
        .unwrap();
    let cur_branch = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let base_label = aifo_coder::fork_sanitize_base_label(&cur_branch);

    // fork two panes and create a conflict
    let sid = "merge-color-fail";
    let clones =
        aifo_coder::fork_clone_and_checkout_panes(repo, sid, 2, &cur_branch, &base_label, false)
            .expect("clone");
    // pane 1
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
            .args(["commit", "-m", "p1"])
            .current_dir(pane_dir)
            .status()
            .unwrap()
            .success());
    }
    // pane 2
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
            .args(["commit", "-m", "p2"])
            .current_dir(pane_dir)
            .status()
            .unwrap()
            .success());
    }

    // run merge with forced color; expect failure
    let bin = env!("CARGO_BIN_EXE_aifo-coder");
    let out_cli = Command::new(bin)
        .args([
            "--color",
            "always",
            "fork",
            "merge",
            "--session",
            sid,
            "--strategy",
            "octopus",
        ])
        .current_dir(repo)
        .output()
        .expect("run aifo-coder fork merge");
    assert!(!out_cli.status.success(), "expected merge failure");

    let stderr = String::from_utf8_lossy(&out_cli.stderr);
    assert!(
        stderr.contains("\x1b["),
        "expected ANSI color in stderr, got:\n{}",
        stderr
    );
    assert!(
        stderr.to_ascii_lowercase().contains("failed"),
        "expected failure message in stderr, got:\n{}",
        stderr
    );
}
