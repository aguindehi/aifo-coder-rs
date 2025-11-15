use std::fs;
mod support;
use std::time::{SystemTime, UNIX_EPOCH};
use support::capture_stdout;

// using tests/support::capture_stdout

#[test]
fn unit_test_workspace_fork_list_plain_output_color_forced() {
    // Prepare a workspace with a single repo containing forks (no git required)
    let ws = tempfile::tempdir().expect("tmpdir");
    let repo = ws.path().join("repoA");
    let forks = repo.join(".aifo-coder").join("forks");
    fs::create_dir_all(&forks).unwrap();

    // Force color regardless of TTY to exercise colored plain output
    std::env::set_var("AIFO_CODER_FORK_LIST_STALE_DAYS", "5");
    std::env::set_var("AIFO_CODER_WORKSPACE_ROOT", ws.path());
    std::env::set_var("AIFO_CODER_COLOR", "always");

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

    // Capture stdout from public API for workspace plain output
    let out = capture_stdout(|| {
        // repo_root argument is ignored in --all-repos mode
        let _ = aifo_coder::fork_list(ws.path(), false, true);
    });
    let got = out;

    // Expect colored header and colored fields
    assert!(
        got.contains("\x1b[36;1maifo-coder: fork sessions under\x1b[0m"),
        "expected cyan bold header in colored plain workspace output, got:\n{}",
        got
    );
    assert!(
        got.contains("\x1b[34;1m"),
        "expected bold blue path or base label in colored plain workspace output, got:\n{}",
        got
    );
    assert!(
        got.contains("(stale)"),
        "expected '(stale)' marker for old session in workspace output, got:\n{}",
        got
    );
}
