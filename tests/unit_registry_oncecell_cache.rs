#[test]
fn unit_test_registry_oncecell_cache_persists_across_env_clear_and_disk_invalidate() {
    use std::env::{remove_var, set_var};
    use std::fs;

    let td = tempfile::tempdir().expect("tmpdir");
    set_var("XDG_RUNTIME_DIR", td.path());

    // Ensure clean state and no probe overrides
    aifo_coder::registry_probe_set_override_for_tests(None);
    aifo_coder::invalidate_registry_cache();
    remove_var("AIFO_CODER_INTERNAL_REGISTRY_PREFIX");

    // First resolution via env (sets OnceCell and writes cache)
    set_var("AIFO_CODER_INTERNAL_REGISTRY_PREFIX", "alpha///");
    let pref1 = aifo_coder::preferred_internal_registry_prefix_quiet();
    assert_eq!(pref1, "alpha/");
    assert_eq!(aifo_coder::preferred_internal_registry_source(), "env");

    // Internal registry has no on-disk cache
    let cache_path = td.path().join("aifo-coder.mirrorprefix");
    assert!(
        !cache_path.exists(),
        "internal registry has no on-disk cache"
    );

    // Clear env; OnceCell should keep the first resolved value
    remove_var("AIFO_CODER_INTERNAL_REGISTRY_PREFIX");
    let pref2 = aifo_coder::preferred_internal_registry_prefix_quiet();
    assert_eq!(pref2, "alpha/", "cache should preserve initial value");

    // Invalidate disk cache does not affect in-process cache
    aifo_coder::invalidate_registry_cache();
    assert!(!cache_path.exists(), "disk cache removed");
    let pref3 = aifo_coder::preferred_internal_registry_prefix_quiet();
    assert_eq!(pref3, "alpha/", "in-process cache still preserved");
}
