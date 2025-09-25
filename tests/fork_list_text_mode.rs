use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
mod support;

#[test]
fn test_fork_list_text_mode_header_and_stale_mark() {
    let td = tempfile::tempdir().expect("tmpdir");
    let root = td.path().to_path_buf();
    let _ = support::init_repo_with_default_user(&root);

    // Old session for stale marking
    let forks = root.join(".aifo-coder").join("forks");
    std::fs::create_dir_all(&forks).unwrap();
    let sid = "sid-text-stale";
    let sd = forks.join(sid);
    let pane = sd.join("pane-1");
    std::fs::create_dir_all(&pane).unwrap();
    let _ = support::init_repo_with_default_user(&pane);
    let head = String::from_utf8_lossy(
        &std::process::Command::new("git")
            .args(["rev-parse", "--verify", "HEAD"])
            .current_dir(&pane)
            .output()
            .unwrap()
            .stdout,
    )
    .trim()
    .to_string();
    let created_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        - 20 * 86400;
    let meta = format!(
        "{{ \"created_at\": {}, \"base_label\": \"main\", \"base_ref_or_sha\": \"main\", \"base_commit_sha\": \"{}\", \"panes\": 1, \"pane_dirs\": [\"{}\"], \"branches\": [\"fork/main/{}-1\"], \"layout\": \"tiled\" }}",
        created_at, head, pane.display(), sid
    );
    std::fs::write(sd.join(".meta.json"), meta).unwrap();

    let bin = env!("CARGO_BIN_EXE_aifo-coder");
    let out = Command::new(bin)
        .args(["fork", "list"])
        .current_dir(&root)
        .output()
        .expect("run aifo-coder fork list");
    assert!(
        out.status.success(),
        "fork list should succeed, stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(
        s.contains("aifo-coder: fork sessions under"),
        "text output should include header path, got:\n{}",
        s
    );
    assert!(
        s.contains("(stale)"),
        "text output should include '(stale)' marking for old sessions, got:\n{}",
        s
    );
}
