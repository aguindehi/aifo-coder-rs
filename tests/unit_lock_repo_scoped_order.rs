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
    // Prefer actual git init if available (more realistic)
    let _ = std::process::Command::new("git")
        .arg("init")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

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
    assert_eq!(
        paths.first(),
        Some(&first),
        "first candidate must be in-repo lock path"
    );
    assert_eq!(
        paths.get(1),
        Some(&second_base),
        "second candidate must be hashed runtime-scoped lock path"
    );

    // Restore env and cwd
    if let Some(v) = old_xdg {
        std::env::set_var("XDG_RUNTIME_DIR", v);
    } else {
        std::env::remove_var("XDG_RUNTIME_DIR");
    }
    std::env::set_current_dir(old_cwd).ok();
}
