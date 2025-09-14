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
fn test_git_stdout_str_invalid_subcommand_returns_none() {
    // This should fail with a non-zero exit, yielding None
    let out = aifo_coder::fork_impl_git::git_stdout_str(None, &["this-subcommand-does-not-exist"]);
    assert!(out.is_none(), "expected None for invalid subcommand");
}

#[test]
fn test_git_status_porcelain_clean_repo_is_empty() {
    if !have_git() {
        eprintln!("skipping: git not found in PATH");
        return;
    }
    let td = tempfile::tempdir().expect("tmpdir");
    let root = td.path();

    assert!(Command::new("git").args(["init"]).current_dir(root).status().unwrap().success());
    let _ = Command::new("git").args(["config", "user.name", "AIFO Test"]).current_dir(root).status();
    let _ = Command::new("git").args(["config", "user.email", "aifo@example.com"]).current_dir(root).status();
    std::fs::write(root.join("a.txt"), "a\n").unwrap();
    assert!(Command::new("git").args(["add", "-A"]).current_dir(root).status().unwrap().success());
    assert!(Command::new("git").args(["commit", "-m", "c1"]).current_dir(root).status().unwrap().success());

    let s = aifo_coder::fork_impl_git::git_status_porcelain(root).unwrap_or_default();
    assert!(
        s.trim().is_empty(),
        "expected empty porcelain status for clean repo, got: {}",
        s
    );
}

#[test]
fn test_push_file_allow_args_appends_correct_flags() {
    let mut v = vec!["git".to_string()];
    aifo_coder::fork_impl_git::push_file_allow_args(&mut v);
    assert_eq!(v.get(1).map(String::as_str), Some("-c"));
    assert_eq!(v.get(2).map(String::as_str), Some("protocol.file.allow=always"));
}
