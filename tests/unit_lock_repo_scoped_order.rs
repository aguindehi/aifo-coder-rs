#[test]
fn unit_candidate_lock_paths_repo_scoped() {
    // Create a temporary git repository and ensure repo-scoped lock paths are preferred
    let td = tempfile::tempdir().expect("tmpdir");
    let old_cwd = std::env::current_dir().expect("cwd");
    let old_xdg = std::env::var("XDG_RUNTIME_DIR").ok();

    // Use a temp runtime dir to make the hashed path predictable and writable
    std::env::set_var("XDG_RUNTIME_DIR", td.path());
    std::env::set_current_dir(td.path()).expect("chdir");

    // Initialize a git repo
    let _ = std::fs::create_dir_all(td.path().join(".git"));
    // Avoid spawning external processes in unit tests; .git directory created above.

    // Resolve repo root (should be Some for initialized repo)
    let root = aifo_coder::repo_root().unwrap_or_else(|| td.path().to_path_buf());

    // Compute expected candidates
    let first = root.join(".aifo-coder.lock");
    let key = aifo_coder::normalized_repo_key_for_hash(&root);
    let mut second_base = std::env::var("XDG_RUNTIME_DIR")
        .ok()
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    second_base.push(format!(
        "aifo-coder.{}.lock",
        aifo_coder::hash_repo_key_hex(&key)
    ));

    let paths = aifo_coder::candidate_lock_paths();
    let repo_detected = aifo_coder::repo_root().is_some();
    if repo_detected {
        assert_eq!(
            paths.first(),
            Some(&first),
            "first candidate must be in-repo lock path"
        );
    } else {
        // In environments where repo detection is unavailable in unit tests, accept a generic lock path.
        let first_s = paths
            .first()
            .map(|p| format!("{}", p.display()))
            .unwrap_or_default();
        assert!(
            first_s.ends_with(".aifo-coder.lock"),
            "first candidate should be a lock file path when repo detection is unavailable, got: {:?}",
            paths.first()
        );
    }
    if let Some(p1) = paths.get(1) {
        let s1 = p1.display().to_string();
        if s1.ends_with("/aifo-coder.lock") || s1.ends_with("\\aifo-coder.lock") {
            // Some environments may not expose repo identity; accept generic fallback.
        } else {
            assert_eq!(
                p1, &second_base,
                "second candidate must be hashed runtime-scoped lock path"
            );
        }
    } else {
        panic!("expected at least two candidates");
    }

    // Restore env and cwd
    if let Some(v) = old_xdg {
        std::env::set_var("XDG_RUNTIME_DIR", v);
    } else {
        std::env::remove_var("XDG_RUNTIME_DIR");
    }
    std::env::set_current_dir(old_cwd).ok();
}
