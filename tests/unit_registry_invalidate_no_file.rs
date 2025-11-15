#[test]
fn unit_test_registry_invalidate_no_file_is_safe_noop() {
    use std::env::set_var;

    let td = tempfile::tempdir().expect("tmpdir");
    set_var("XDG_RUNTIME_DIR", td.path());

    // Ensure cache file does not exist and invalidate does not panic
    let cache_path = td.path().join("aifo-coder.mirrorprefix");
    assert!(
        !cache_path.exists(),
        "precondition: cache file should not exist"
    );
    aifo_coder::invalidate_registry_cache();
    assert!(
        !cache_path.exists(),
        "invalidate should leave non-existent cache unchanged"
    );
}
