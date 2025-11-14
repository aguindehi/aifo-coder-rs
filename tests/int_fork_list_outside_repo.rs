use std::process::Command;

#[test]
fn int_test_fork_list_outside_git_repo_errors() {
    // Create a temp dir without initializing a git repo
    let td = tempfile::tempdir().expect("tmpdir");
    let dir = td.path();

    let bin = env!("CARGO_BIN_EXE_aifo-coder");
    let out = Command::new(bin)
        .args(["fork", "list"])
        .current_dir(dir)
        .output()
        .expect("run aifo-coder fork list outside repo");
    assert!(
        !out.status.success(),
        "expected non-zero exit outside git repo"
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("must be run inside a Git repository"),
        "stderr should mention running inside a Git repository, got:\n{}",
        err
    );
}
