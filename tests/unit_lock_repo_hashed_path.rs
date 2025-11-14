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
    std::env::set_current_dir(&repo_a).unwrap();

    // repo B
    let repo_b = ws.join("repo-b");
    std::fs::create_dir_all(&repo_b).unwrap();
    std::env::set_current_dir(&repo_b).unwrap();

    // Compute normalized repo keys and ensure hashes differ (independent of git CLI)
    let key_a = aifo_coder::normalized_repo_key_for_hash(&repo_a);
    let key_b = aifo_coder::normalized_repo_key_for_hash(&repo_b);
    let hash_a = aifo_coder::hash_repo_key_hex(&key_a);
    let hash_b = aifo_coder::hash_repo_key_hex(&key_b);

    assert_ne!(
        hash_a, hash_b,
        "hashed repo keys should differ across repos: A={} B={}",
        hash_a, hash_b
    );

    // restore env/cwd
    if let Some(v) = old_xdg {
        std::env::set_var("XDG_RUNTIME_DIR", v);
    } else {
        std::env::remove_var("XDG_RUNTIME_DIR");
    }
}
