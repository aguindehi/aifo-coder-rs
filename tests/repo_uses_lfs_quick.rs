use std::fs;

#[test]
fn test_repo_uses_lfs_quick_top_level() {
    let td = tempfile::tempdir().expect("tmpdir");
    let root = td.path();
    fs::write(
        root.join(".gitattributes"),
        "*.bin filter=lfs diff=lfs merge=lfs -text\n",
    )
    .unwrap();
    assert!(
        aifo_coder::repo_uses_lfs_quick(root),
        "expected top-level .gitattributes with filter=lfs to be detected"
    );
}

#[test]
fn test_repo_uses_lfs_quick_nested() {
    let td = tempfile::tempdir().expect("tmpdir");
    let root = td.path();
    let nested = root.join("nested");
    fs::create_dir_all(&nested).unwrap();
    fs::write(
        nested.join(".gitattributes"),
        "*.dat filter=lfs diff=lfs merge=lfs -text\n",
    )
    .unwrap();
    assert!(
        aifo_coder::repo_uses_lfs_quick(root),
        "expected nested .gitattributes with filter=lfs to be detected"
    );
}

#[test]
fn test_repo_uses_lfs_quick_absent() {
    let td = tempfile::tempdir().expect("tmpdir");
    let root = td.path();
    assert!(
        !aifo_coder::repo_uses_lfs_quick(root),
        "expected false when no .lfsconfig or .gitattributes declares filter=lfs"
    );
}
