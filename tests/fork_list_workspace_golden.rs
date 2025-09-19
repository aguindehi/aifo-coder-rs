use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn test_workspace_fork_list_json_golden_single_repo() {
    // Prepare a workspace with a single repo containing forks (no git required)
    let ws = tempfile::tempdir().expect("tmpdir");
    let repo = ws.path().join("repoA");
    let forks = repo.join(".aifo-coder").join("forks");
    fs::create_dir_all(&forks).unwrap();

    // Ensure stale threshold matches expected values in this test
    std::env::set_var("AIFO_CODER_FORK_LIST_STALE_DAYS", "5");
    std::env::set_var("AIFO_CODER_WORKSPACE_ROOT", ws.path());

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

    // Obtain JSON output string from public API for workspace mode
    let out = aifo_coder::fork_list_to_string(ws.path(), true, true).expect("fork_list_to_string");
    let got = out.trim();

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

    assert_eq!(
        got, expected,
        "workspace fork_list JSON should match exactly"
    );
}

#[test]
fn test_workspace_fork_list_plain_single_repo() {
    // Prepare a workspace with a single repo containing forks (no git required)
    let ws = tempfile::tempdir().expect("tmpdir");
    let repo = ws.path().join("repoA");
    let forks = repo.join(".aifo-coder").join("forks");
    fs::create_dir_all(&forks).unwrap();

    // Ensure stale threshold matches expected values in this test
    std::env::set_var("AIFO_CODER_FORK_LIST_STALE_DAYS", "5");
    std::env::set_var("AIFO_CODER_WORKSPACE_ROOT", ws.path());

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| std::time::Duration::from_secs(0))
        .as_secs();

    // Sessions
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

    // Obtain plain output string from public API for workspace mode
    let out = aifo_coder::fork_list_to_string(ws.path(), false, true).expect("fork_list_to_string");
    let got = out.trim();

    let header = format!(
        "aifo-coder: fork sessions under {}/.aifo-coder/forks",
        repo.display()
    );
    let expected = format!(
        "{}\n  sid-old  panes=1  age=10d  base=main  (stale)\n  sid-new  panes=1  age=2d  base=dev",
        header
    );

    assert_eq!(
        got, expected,
        "workspace fork_list plain output should match exactly"
    );
}
