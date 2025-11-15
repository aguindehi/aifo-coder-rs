#[test]
fn unit_test_registry_quiet_env_empty_writes_cache_and_invalidate_removes() {
    use std::env::{remove_var, set_var};
    use std::fs;
    use std::path::PathBuf;

    // Unique runtime dir per test file
    let td = tempfile::tempdir().expect("tmpdir");
    let rt = td.path().to_path_buf();
    set_var("XDG_RUNTIME_DIR", &rt);

    // Ensure clean state and no overrides
    aifo_coder::registry_probe_set_override_for_tests(None);
    aifo_coder::invalidate_registry_cache();
    remove_var("AIFO_CODER_TEST_REGISTRY_PROBE");

    // Whitespace env override is treated as empty (Docker Hub)
    set_var("AIFO_CODER_INTERNAL_REGISTRY_PREFIX", "   ");
    let pref = aifo_coder::preferred_internal_registry_prefix_quiet();
    assert_eq!(pref, "", "env-empty should yield empty prefix (quiet)");

    let src = aifo_coder::preferred_internal_registry_source();
    assert_eq!(src, "env-empty", "source should be env-empty");

    // Internal registry has no on-disk cache
    let cache_path: PathBuf = rt.join("aifo-coder.mirrorprefix");
    assert!(
        !cache_path.exists(),
        "internal registry does not use on-disk cache"
    );

    // Invalidate should remove cache file
    aifo_coder::invalidate_registry_cache();
    assert!(
        !cache_path.exists(),
        "cache should be removed by invalidate_registry_cache"
    );

    // Cleanup
    remove_var("AIFO_CODER_REGISTRY_PREFIX");
}
