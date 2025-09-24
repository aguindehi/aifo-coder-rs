use serde_json::Value;
use std::fs;
mod support;
use support::capture_stdout;
use std::time::{SystemTime, UNIX_EPOCH};

 // using tests/support::capture_stdout

#[test]
fn test_workspace_fork_list_json_multiple_repos_order_insensitive() {
    // Prepare a workspace with two repos containing forks (no git required)
    let ws = tempfile::tempdir().expect("tmpdir");
    let repo_a = ws.path().join("repoA");
    let repo_b = ws.path().join("repoB");
    let forks_a = repo_a.join(".aifo-coder").join("forks");
    let forks_b = repo_b.join(".aifo-coder").join("forks");
    fs::create_dir_all(&forks_a).unwrap();
    fs::create_dir_all(&forks_b).unwrap();

    // Ensure stale threshold is constant and workspace root is configured
    std::env::set_var("AIFO_CODER_FORK_LIST_STALE_DAYS", "5");
    std::env::set_var("AIFO_CODER_WORKSPACE_ROOT", ws.path());

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| std::time::Duration::from_secs(0))
        .as_secs();

    // Repo A: older (stale)
    let s1 = forks_a.join("sid-old-a");
    fs::create_dir_all(s1.join("pane-1")).unwrap();
    fs::write(
        s1.join(".meta.json"),
        format!(
            "{{ \"created_at\": {}, \"base_label\": \"main\" }}",
            now.saturating_sub(10 * 86400)
        ),
    )
    .unwrap();

    // Repo B: recent (not stale)
    let s2 = forks_b.join("sid-new-b");
    fs::create_dir_all(s2.join("pane-1")).unwrap();
    fs::write(
        s2.join(".meta.json"),
        format!(
            "{{ \"created_at\": {}, \"base_label\": \"dev\" }}",
            now.saturating_sub(2 * 86400)
        ),
    )
    .unwrap();

    // Capture stdout from public API for workspace JSON
    let out = capture_stdout(|| {
        // repo_root argument is ignored in --all-repos mode
        let _ = aifo_coder::fork_list(ws.path(), true, true);
    });
    let got = out.trim().to_string();

    // Parse JSON and assert contents order-insensitively
    let v: Value = serde_json::from_str(&got).expect("valid JSON");
    let arr = v.as_array().expect("array");
    assert_eq!(arr.len(), 2, "expected two rows across workspace repos");
    let mut normalized: Vec<(String, String, u64, bool)> = Vec::new();
    for row in arr {
        let obj = row.as_object().expect("obj");
        let repo_root = obj.get("repo_root").and_then(|x| x.as_str()).unwrap_or("");
        let sid = obj.get("sid").and_then(|x| x.as_str()).unwrap_or("");
        let created_at = obj.get("created_at").and_then(|x| x.as_u64()).unwrap_or(0);
        let stale = obj.get("stale").and_then(|x| x.as_bool()).unwrap_or(false);
        normalized.push((repo_root.to_string(), sid.to_string(), created_at, stale));
    }
    normalized.sort_by(|a, b| a.0.cmp(&b.0));

    let repo_a_s = repo_a.display().to_string();
    let repo_b_s = repo_b.display().to_string();
    assert_eq!(normalized[0].0, repo_a_s);
    assert_eq!(normalized[0].1, "sid-old-a");
    assert!(normalized[0].3, "repo A entry should be stale");
    assert_eq!(normalized[1].0, repo_b_s);
    assert_eq!(normalized[1].1, "sid-new-b");
    assert!(!normalized[1].3, "repo B entry should not be stale");
}
