use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
mod support;

fn write_fake_session(repo_root: &std::path::Path, sid: &str, age_days: u64) {
    let forks = repo_root.join(".aifo-coder").join("forks");
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
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let created_at = now.saturating_sub(age_days * 86400);
    let meta = format!(
        "{{ \"created_at\": {}, \"base_label\": \"main\", \"base_ref_or_sha\": \"main\", \"base_commit_sha\": \"{}\", \"panes\": 1, \"pane_dirs\": [\"{}\"], \"branches\": [\"fork/main/{}-1\"], \"layout\": \"tiled\" }}",
        created_at, head, pane.display(), sid
    );
    std::fs::write(sd.join(".meta.json"), meta).unwrap();
}

#[test]
fn test_fork_list_all_repos_json_includes_both() {
    let ws = tempfile::tempdir().expect("tmpdir");
    let wsdir = ws.path();

    // repo1
    let repo1 = wsdir.join("repo1");
    std::fs::create_dir_all(&repo1).unwrap();
    let _ = support::init_repo_with_default_user(&repo1);
    write_fake_session(&repo1, "sid-r1", 20);

    // repo2
    let repo2 = wsdir.join("repo2");
    std::fs::create_dir_all(&repo2).unwrap();
    let _ = support::init_repo_with_default_user(&repo2);
    write_fake_session(&repo2, "sid-r2", 1);

    let bin = env!("CARGO_BIN_EXE_aifo-coder");
    let out = Command::new(bin)
        .args(["fork", "list", "--json", "--all-repos"])
        .env("AIFO_CODER_WORKSPACE_ROOT", wsdir)
        .current_dir(wsdir)
        .output()
        .expect("run aifo-coder fork list --all-repos --json");
    assert!(
        out.status.success(),
        "fork list --all-repos should succeed, stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(
        s.contains(&format!(
            "\"repo_root\":{}",
            aifo_coder::json_escape(&repo1.display().to_string())
        )),
        "json should include repo1 root: {}",
        s
    );
    assert!(
        s.contains(&format!(
            "\"repo_root\":{}",
            aifo_coder::json_escape(&repo2.display().to_string())
        )),
        "json should include repo2 root: {}",
        s
    );
}

#[test]
fn test_fork_list_all_repos_requires_env() {
    let ws = tempfile::tempdir().expect("tmpdir");
    let bin = env!("CARGO_BIN_EXE_aifo-coder");
    let out = Command::new(bin)
        .args(["fork", "list", "--all-repos"])
        .current_dir(ws.path())
        .output()
        .expect("run aifo-coder fork list --all-repos without env");
    assert_eq!(
        out.status.code(),
        Some(1),
        "expected exit 1; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("requires AIFO_CODER_WORKSPACE_ROOT"),
        "stderr should mention AIFO_CODER_WORKSPACE_ROOT requirement, got:\n{}",
        err
    );
}
