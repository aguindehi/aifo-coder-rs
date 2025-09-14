use std::fs;
use std::io::{Read, Seek};
use std::os::fd::{FromRawFd, RawFd};
use std::time::{SystemTime, UNIX_EPOCH};


// Capture stdout for the duration of f, returning the captured UTF-8 string.
// Unix-only; safe for our CI matrix (macOS/Linux).
fn capture_stdout<F: FnOnce()>(f: F) -> String {
    use libc::{dup, dup2, fflush, fileno, fopen, STDOUT_FILENO};
    unsafe {
        // Open a temporary file
        let path = std::ffi::CString::new("/tmp/aifo-coder-test-stdout.tmp").unwrap();
        let mode = std::ffi::CString::new("w+").unwrap();
        let file = fopen(path.as_ptr(), mode.as_ptr());
        assert!(!file.is_null(), "failed to open temp file for capture");
        let fd: RawFd = fileno(file);

        // Duplicate current stdout
        let stdout_fd = STDOUT_FILENO;
        let saved = dup(stdout_fd);
        assert!(saved >= 0, "dup(stdout) failed");

        // Redirect stdout to file
        assert!(dup2(fd, stdout_fd) >= 0, "dup2 failed");

        // Run the function
        f();

        // Flush and restore stdout
        fflush(std::ptr::null_mut());
        assert!(dup2(saved, stdout_fd) >= 0, "restore dup2 failed");

        // Read back the file
        let mut f = std::fs::File::from_raw_fd(fd);
        let mut s = String::new();
        f.rewind().ok();
        f.read_to_string(&mut s).expect("read captured");
        // Prevent closing freed file twice
        s
    }
}

#[test]
fn test_public_fork_list_json_golden() {
    // Prepare a fake repo layout with forks under it (no git required)
    let td = tempfile::tempdir().expect("tmpdir");
    let repo = td.path();

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
