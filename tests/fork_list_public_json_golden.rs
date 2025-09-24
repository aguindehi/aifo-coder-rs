use std::fs;
mod support;
use support::capture_stdout;
use std::time::{SystemTime, UNIX_EPOCH};

 // using tests/support::capture_stdout

#[test]
fn test_public_fork_list_json_golden() {
    // Prepare a fake repo layout with forks under it (no git required)
    let td = tempfile::tempdir().expect("tmpdir");
    let repo = td.path();
    // Ensure stale threshold matches expected values in this test
    std::env::set_var("AIFO_CODER_FORK_LIST_STALE_DAYS", "5");

    let forks = repo.join(".aifo-coder").join("forks");
    fs::create_dir_all(&forks).unwrap();

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| std::time::Duration::from_secs(0))
        .as_secs();

    // Older session (stale)
    let s1 = forks.join("sid-old");
    fs::create_dir_all(s1.join("pane-1")).unwrap();
    fs::write(
        s1.join(".meta.json"),
        format!(
            "{{ \"created_at\": {}, \"base_label\": \"main\" }}",
            now.saturating_sub(10 * 86400)
        ),
    )
    .unwrap();

    // Recent session (not stale)
    let s2 = forks.join("sid-new");
    fs::create_dir_all(s2.join("pane-1")).unwrap();
    fs::write(
        s2.join(".meta.json"),
        format!(
            "{{ \"created_at\": {}, \"base_label\": \"dev\" }}",
            now.saturating_sub(2 * 86400)
        ),
    )
    .unwrap();

    // Capture stdout from public API
    let out = capture_stdout(|| {
        let _ = aifo_coder::fork_list(repo, true, false);
    });
    let got = out.trim();

    // Build expected JSON (order by created_at ascending)
    let repo_s = repo.display().to_string();
    let expected = format!(
        "[{{\"repo_root\":{},\"sid\":\"sid-old\",\"panes\":1,\"created_at\":{},\"age_days\":10,\"base_label\":{},\"stale\":true}},{{\"repo_root\":{},\"sid\":\"sid-new\",\"panes\":1,\"created_at\":{},\"age_days\":2,\"base_label\":{},\"stale\":false}}]",
        aifo_coder::json_escape(&repo_s),
        now.saturating_sub(10*86400),
        aifo_coder::json_escape("main"),
        aifo_coder::json_escape(&repo_s),
        now.saturating_sub(2*86400),
        aifo_coder::json_escape("dev")
    );

    assert_eq!(got, expected, "fork_list JSON should match exactly");
}
