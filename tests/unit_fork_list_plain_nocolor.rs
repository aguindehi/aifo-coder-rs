use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

mod support;
use support::capture_stdout;

#[test]
fn unit_test_fork_list_plain_output_no_color_single_repo() {
    // Prepare a fake repo layout with forks under it (no git required)
    let td = tempfile::tempdir().expect("tmpdir");
    let repo = td.path();

    // Ensure default/no-color and a fixed stale threshold
    std::env::remove_var("AIFO_CODER_COLOR");
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

    // Capture stdout from public API (plain, not JSON)
    let out = capture_stdout(|| {
        let _ = aifo_coder::fork_list(repo, false, false);
    });
    let got = out.trim().to_string();

    // Expect no ANSI color escapes and specific plain output
    assert!(
        !got.contains("\x1b["),
        "plain non-color output must not contain ANSI escapes, got:\n{}",
        got
    );

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
        "single-repo plain output should match exactly"
    );
}

#[test]
fn unit_test_workspace_fork_list_plain_output_no_color() {
    // Prepare a workspace with a single repo containing forks (no git required)
    let ws = tempfile::tempdir().expect("tmpdir");
    let repo = ws.path().join("repoA");
    let forks = repo.join(".aifo-coder").join("forks");
    fs::create_dir_all(&forks).unwrap();

    // Ensure default/no-color and a fixed stale threshold
    std::env::remove_var("AIFO_CODER_COLOR");
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

    // Capture stdout from public API for workspace plain output
    let out = capture_stdout(|| {
        let _ = aifo_coder::fork_list(ws.path(), false, true);
    });
    let got = out.trim().to_string();

    // Expect no ANSI color escapes and specific plain output
    assert!(
        !got.contains("\x1b["),
        "workspace plain non-color output must not contain ANSI escapes, got:\n{}",
        got
    );

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
        "workspace plain output (no color) should match exactly"
    );
}
