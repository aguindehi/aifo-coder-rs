#[cfg(not(windows))]
#[test]
fn unit_hashed_lock_path_diff_for_two_repos() {
    // Create two separate repos and ensure their hashed XDG lock paths differ
    let td = tempfile::tempdir().expect("tmpdir");
    let ws = td.path().to_path_buf();
    let old_xdg = std::env::var("XDG_RUNTIME_DIR").ok();
    std::env::set_var("XDG_RUNTIME_DIR", &ws);

    // repo A
    let repo_a = ws.join("repo-a");
    std::fs::create_dir_all(&repo_a).unwrap();
    let _ = std::process::Command::new("git")
        .arg("init")
        .current_dir(&repo_a)
        .status();
    std::env::set_current_dir(&repo_a).unwrap();
    let paths_a = aifo_coder::candidate_lock_paths();
    assert!(
        paths_a.len() >= 2,
        "expected at least two candidates for repo A"
    );
    let hashed_a = paths_a[1].clone();

    // repo B
    let repo_b = ws.join("repo-b");
    std::fs::create_dir_all(&repo_b).unwrap();
    let _ = std::process::Command::new("git")
        .arg("init")
        .current_dir(&repo_b)
        .status();
    std::env::set_current_dir(&repo_b).unwrap();
    let paths_b = aifo_coder::candidate_lock_paths();
    assert!(
        paths_b.len() >= 2,
        "expected at least two candidates for repo B"
    );
    let hashed_b = paths_b[1].clone();

    assert_ne!(
        hashed_a,
        hashed_b,
        "hashed runtime lock path should differ across repos: A={} B={}",
        hashed_a.display(),
        hashed_b.display()
    );

    // restore env/cwd
    if let Some(v) = old_xdg {
        std::env::set_var("XDG_RUNTIME_DIR", v);
    } else {
        std::env::remove_var("XDG_RUNTIME_DIR");
    }
}
