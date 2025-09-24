use std::fs;
mod support;
use support::capture_stdout;
use std::time::{SystemTime, UNIX_EPOCH};

 // using tests/support::capture_stdout

#[test]
fn test_fork_list_plain_output_color_forced() {
    // Prepare a fake repo layout with forks under it (no git required)
    let td = tempfile::tempdir().expect("tmpdir");
    let repo = td.path();

    // Force color regardless of TTY to exercise colored plain output
    std::env::set_var("AIFO_CODER_FORK_LIST_STALE_DAYS", "5");
    std::env::set_var("AIFO_CODER_COLOR", "always");

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
    let got = out;

    // Expect colored header and colored fields
    assert!(
        got.contains("\x1b[36;1maifo-coder: fork sessions under\x1b[0m"),
        "expected cyan bold header in colored plain output, got:\n{}",
        got
    );
    assert!(
        got.contains("\x1b[34;1m"),
        "expected bold blue path or base label in colored plain output, got:\n{}",
        got
    );
    assert!(
        got.contains("(stale)"),
        "expected '(stale)' marker for old session in output, got:\n{}",
        got
    );
}
