use std::process::Command;
mod support;

#[test]
fn int_test_fork_list_json_empty_returns_empty_array() {
    let td = tempfile::tempdir().expect("tmpdir");
    let root = td.path().to_path_buf();
    let _ = support::init_repo_with_default_user(&root);

    let bin = env!("CARGO_BIN_EXE_aifo-coder");
    let out = Command::new(bin)
        .args(["fork", "list", "--json"])
        .current_dir(&root)
        .output()
        .expect("run aifo-coder fork list --json");
    assert!(
        out.status.success(),
        "fork list --json should succeed; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        stdout.trim(),
        "[]",
        "expected empty JSON array, got:\n{}",
        stdout
    );
}

#[test]
fn int_test_fork_list_text_empty_reports_none() {
    let td = tempfile::tempdir().expect("tmpdir");
    let root = td.path().to_path_buf();
    let _ = support::init_repo_with_default_user(&root);

    let bin = env!("CARGO_BIN_EXE_aifo-coder");
    let out = Command::new(bin)
        .args(["fork", "list"])
        .current_dir(&root)
        .output()
        .expect("run aifo-coder fork list");
    assert!(
        out.status.success(),
        "fork list should succeed; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("no fork sessions found"),
        "expected 'no fork sessions found' text, got:\n{}",
        stdout
    );
}
