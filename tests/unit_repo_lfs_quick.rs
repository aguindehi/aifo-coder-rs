#[test]
fn unit_test_repo_uses_lfs_quick_top_level_gitattributes() {
    let td = tempfile::tempdir().expect("tmpdir");
    let repo = td.path();
    // Create top-level .gitattributes with lfs filter
    std::fs::write(
        repo.join(".gitattributes"),
        "*.bin filter=lfs diff=lfs merge=lfs -text\n",
    )
    .unwrap();
    assert!(
        aifo_coder::repo_uses_lfs_quick(repo),
        "expected repo_uses_lfs_quick to detect top-level filter=lfs"
    );
}

#[test]
fn unit_test_repo_uses_lfs_quick_nested_gitattributes() {
    let td = tempfile::tempdir().expect("tmpdir");
    let repo = td.path();
    let nested = repo.join("assets").join("media");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(
        nested.join(".gitattributes"),
        "*.png filter=lfs diff=lfs merge=lfs -text\n",
    )
    .unwrap();
    assert!(
        aifo_coder::repo_uses_lfs_quick(repo),
        "expected repo_uses_lfs_quick to detect nested filter=lfs"
    );
}

#[test]
fn unit_test_repo_uses_lfs_quick_lfsconfig_present() {
    let td = tempfile::tempdir().expect("tmpdir");
    let repo = td.path();
    std::fs::write(
        repo.join(".lfsconfig"),
        "[lfs]\nurl = https://example.com/lfs\n",
    )
    .unwrap();
    assert!(
        aifo_coder::repo_uses_lfs_quick(repo),
        "expected repo_uses_lfs_quick to detect .lfsconfig presence"
    );
}
