use std::process::Command;
mod support;

fn which(bin: &str) -> bool {
    support::which(bin).is_some()
}

fn init_repo(dir: &std::path::Path) {
    let _ = support::init_repo_with_default_user(dir);
}

#[cfg(unix)]
#[test]
fn test_e2e_fork_tmux_smoke_opt_in() {
    // Only run when explicitly requested and prerequisites are present
    if std::env::var("AIFO_CODER_E2E").ok().as_deref() != Some("1") {
        eprintln!("skipping: AIFO_CODER_E2E!=1");
        return;
    }
    if !which("tmux") {
        eprintln!("skipping: tmux not found");
        return;
    }
    if !Command::new("git")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
    {
        eprintln!("skipping: git not found");
        return;
    }

    let td = tempfile::tempdir().expect("tmpdir");
    let root = td.path().to_path_buf();
    init_repo(&root);

    // Run fork with 2 panes; set TMUX to force switch-client (non-attaching)
    let bin = env!("CARGO_BIN_EXE_aifo-coder");
    let out = Command::new(bin)
        .args([
            "--fork",
            "2",
            "--fork-session-name",
            "ut-smoke",
            "aider",
            "--",
            "--help",
        ])
        .env("TMUX", "1")
        .current_dir(&root)
        .output()
        .expect("run aifo-coder fork smoke");
    // Accept either success or non-zero; just verify clones exist
    let forks = root.join(".aifo-coder").join("forks");
    let entries = std::fs::read_dir(&forks)
        .ok()
        .into_iter()
        .flat_map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.path()))
        .collect::<Vec<_>>();
    assert!(
        !entries.is_empty(),
        "expected a session dir to be created under {}. stderr={}",
        forks.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    // Inspect the first session dir
    let session = &entries[0];
    assert!(session.join("pane-1").exists(), "pane-1 must exist");
    assert!(session.join("pane-2").exists(), "pane-2 must exist");

    // Check branches in panes start with fork/
    for i in 1..=2 {
        let pane = session.join(format!("pane-{}", i));
        let head = Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(&pane)
            .output()
            .expect("git rev-parse in pane");
        let b = String::from_utf8_lossy(&head.stdout).trim().to_string();
        assert!(
            b.starts_with("fork/"),
            "expected pane branch to start with fork/, got {}",
            b
        );
    }
}
