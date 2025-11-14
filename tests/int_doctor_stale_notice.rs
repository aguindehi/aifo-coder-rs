use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
mod support;

#[test]
fn int_test_doctor_prints_stale_notice_and_status() {
    let td = tempfile::tempdir().expect("tmpdir");
    let root = td.path().to_path_buf();
    let _ = support::init_repo_with_default_user(&root);

    // Create an old session beyond threshold 1d
    let forks = root.join(".aifo-coder").join("forks");
    std::fs::create_dir_all(&forks).unwrap();
    let sid = "sid-old-doc";
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
    let old_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        - 10 * 86400;
    let meta = format!(
        "{{ \"created_at\": {}, \"base_label\": \"main\", \"base_ref_or_sha\": \"main\", \"base_commit_sha\": \"{}\", \"panes\": 1, \"pane_dirs\": [\"{}\"], \"branches\": [\"fork/main/{}-1\"], \"layout\": \"tiled\" }}",
        old_secs, head, pane.display(), sid
    );
    std::fs::write(sd.join(".meta.json"), meta).unwrap();

    // Run doctor with threshold set low to ensure notice
    let bin = env!("CARGO_BIN_EXE_aifo-coder");
    let out = Command::new(bin)
        .arg("doctor")
        .env("AIFO_CODER_FORK_STALE_DAYS", "1")
        .current_dir(&root)
        .output()
        .expect("run doctor");
    assert!(
        out.status.success(),
        "doctor should succeed; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("Found 1 old fork sessions"),
        "doctor stderr should contain stale notice, got:\n{}",
        err
    );
    assert!(
        err.contains("Found 1 old fork sessions (oldest 10d). Consider: aifo-coder fork clean --older-than 1"),
        "doctor stderr should contain stale notice with threshold and suggestion, got:\n{}",
        err
    );
}
