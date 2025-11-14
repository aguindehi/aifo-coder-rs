#[test]
fn test_registry_quiet_env_empty_writes_cache_and_invalidate_removes() {
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
    set_var("AIFO_CODER_REGISTRY_PREFIX", "   ");
    let pref = aifo_coder::preferred_registry_prefix_quiet();
    assert_eq!(pref, "", "env-empty should yield empty prefix (quiet)");

    let src = aifo_coder::preferred_registry_source();
    assert_eq!(src, "env-empty", "source should be env-empty");

    // Verify cache file is written with empty content
    let cache_path: PathBuf = rt.join("aifo-coder.regprefix");
    let content = fs::read_to_string(&cache_path).expect("cache should exist");
    assert_eq!(content, "", "cache content should be empty");

    // Invalidate should remove cache file
    aifo_coder::invalidate_registry_cache();
    assert!(
        !cache_path.exists(),
        "cache should be removed by invalidate_registry_cache"
    );

    // Cleanup
    remove_var("AIFO_CODER_REGISTRY_PREFIX");
}
