#[cfg(windows)]
use std::process::Command;

#[cfg(windows)]
fn which(bin: &str) -> bool {
    Command::new("where")
        .arg(bin)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(windows)]
fn init_repo(dir: &std::path::Path) {
    let _ = std::process::Command::new("git")
        .arg("init")
        .current_dir(dir)
        .status();
    let _ = std::process::Command::new("git")
        .args(["config", "user.name", "UT"])
        .current_dir(dir)
        .status();
    let _ = std::process::Command::new("git")
        .args(["config", "user.email", "ut@example.com"])
        .current_dir(dir)
        .status();
    let _ = std::fs::write(dir.join("init.txt"), "x\n");
    let _ = std::process::Command::new("git")
        .args(["add", "-A"])
        .current_dir(dir)
        .status();
    let _ = std::process::Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(dir)
        .status();
}

#[cfg(windows)]
#[test]
fn test_e2e_fork_windows_terminal_smoke_opt_in() {
    // Only run when explicitly requested and prerequisites are present
    if std::env::var("AIFO_CODER_E2E").ok().as_deref() != Some("1") {
        eprintln!("skipping: AIFO_CODER_E2E!=1");
        return;
    }
    if !which("wt.exe") && !which("wt") {
        eprintln!("skipping: wt.exe not found");
        return;
    }
    if Command::new("git")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
        == false
    {
        eprintln!("skipping: git not found");
        return;
    }

    let td = tempfile::tempdir().expect("tmpdir");
    let root = td.path().to_path_buf();
    init_repo(&root);

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
        .current_dir(&root)
        .output()
        .expect("run aifo-coder fork smoke (Windows Terminal)");
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
}
