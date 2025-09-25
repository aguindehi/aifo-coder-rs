use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
mod support;

#[test]
fn test_fork_list_cli_json_stale_highlight() {
    // Prepare temp repo
    let td = tempfile::tempdir().expect("tmpdir");
    let root = td.path().to_path_buf();
    let _ = support::init_repo_with_default_user(&root);

    let forks = root.join(".aifo-coder").join("forks");
    std::fs::create_dir_all(&forks).unwrap();

    // Old session (older than default 14d)
    let sid_old = "sid-old-cli";
    let sd_old = forks.join(sid_old);
    let pane_old = sd_old.join("pane-1");
    std::fs::create_dir_all(&pane_old).unwrap();
    init_repo(&pane_old);
    let head_old = String::from_utf8_lossy(
        &std::process::Command::new("git")
            .args(["rev-parse", "--verify", "HEAD"])
            .current_dir(&pane_old)
            .output()
            .unwrap()
            .stdout,
    )
    .trim()
    .to_string();
    let old_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        - 20 * 86400;
    let meta_old = format!(
        "{{ \"created_at\": {}, \"base_label\": \"main\", \"base_ref_or_sha\": \"main\", \"base_commit_sha\": \"{}\", \"panes\": 1, \"pane_dirs\": [\"{}\"], \"branches\": [\"fork/main/{}-1\"], \"layout\": \"tiled\" }}",
        old_secs, head_old, pane_old.display(), sid_old
    );
    std::fs::write(sd_old.join(".meta.json"), meta_old).unwrap();

    // Recent session
    let sid_new = "sid-new-cli";
    let sd_new = forks.join(sid_new);
    let pane_new = sd_new.join("pane-1");
    std::fs::create_dir_all(&pane_new).unwrap();
    init_repo(&pane_new);
    let head_new = String::from_utf8_lossy(
        &std::process::Command::new("git")
            .args(["rev-parse", "--verify", "HEAD"])
            .current_dir(&pane_new)
            .output()
            .unwrap()
            .stdout,
    )
    .trim()
    .to_string();
    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let meta_new = format!(
        "{{ \"created_at\": {}, \"base_label\": \"main\", \"base_ref_or_sha\": \"main\", \"base_commit_sha\": \"{}\", \"panes\": 1, \"pane_dirs\": [\"{}\"], \"branches\": [\"fork/main/{}-1\"], \"layout\": \"tiled\" }}",
        now_secs, head_new, pane_new.display(), sid_new
    );
    std::fs::write(sd_new.join(".meta.json"), meta_new).unwrap();

    // Run CLI
    let bin = env!("CARGO_BIN_EXE_aifo-coder");
    let out = Command::new(bin)
        .arg("fork")
        .arg("list")
        .arg("--json")
        .current_dir(&root)
        .output()
        .expect("run fork list --json");
    assert!(
        out.status.success(),
        "fork list should succeed, stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let s = String::from_utf8_lossy(&out.stdout);
    // Expect both sids present
    assert!(
        s.contains(&format!("\"sid\":\"{}\"", sid_old)),
        "json should include old sid: {}",
        s
    );
    assert!(
        s.contains(&format!("\"sid\":\"{}\"", sid_new)),
        "json should include new sid: {}",
        s
    );
    // Expect that at least one stale=true entry exists (the old one)
    assert!(
        s.contains("\"stale\":true"),
        "json should mark old session as stale: {}",
        s
    );
}
