#[test]
fn unit_test_registry_env_empty_uses_hub_and_writes_cache_then_invalidate_removes() {
    use std::env::{remove_var, set_var};
    use std::fs;
    use std::path::PathBuf;

    // Unique runtime dir per test file
    let td = tempfile::tempdir().expect("tmpdir");
    let rt = td.path().to_path_buf();
    set_var("XDG_RUNTIME_DIR", &rt);

    // No override mode
    aifo_coder::registry_probe_set_override_for_tests(None);

    // Start clean and set env override to whitespace (treated as empty)
    aifo_coder::invalidate_registry_cache();
    set_var("AIFO_CODER_REGISTRY_PREFIX", "   ");

    // Prefer non-quiet for parity; both variants write cache
    let pref = aifo_coder::preferred_registry_prefix();
    assert_eq!(pref, "", "env-empty should yield empty prefix");

    let src = aifo_coder::preferred_registry_source();
    assert_eq!(src, "env-empty", "source should be env-empty");

    // Verify cache file content is empty string
    let cache_path: PathBuf = rt.join("aifo-coder.regprefix");
    let content = fs::read_to_string(&cache_path).expect("cache should exist");
    assert_eq!(content, "", "cache content should be empty");

    // Invalidate should remove cache file
    aifo_coder::invalidate_registry_cache();
    assert!(
        !cache_path.exists(),
        "cache should be removed by invalidate_registry_cache"
    );

    // Cleanup env
    remove_var("AIFO_CODER_REGISTRY_PREFIX");
}
