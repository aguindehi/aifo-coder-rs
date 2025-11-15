/*
Targets in src/registry.rs:
- preferred_registry_prefix: sets OnceCell and writes cache (env non-empty).
- preferred_registry_prefix_quiet: returns cached value after env removal.
- invalidate_registry_cache: disk cache removal does not affect in-process cache.
*/
#[test]
fn unit_cache_persists_in_process_after_env_removed() {
    use std::env::{remove_var, set_var};
    // use std::fs;

    // Unique runtime dir per test file
    let td = tempfile::tempdir().expect("tmpdir");
    set_var("XDG_RUNTIME_DIR", td.path());

    // Clean state
    aifo_coder::registry_probe_set_override_for_tests(None);
    aifo_coder::invalidate_registry_cache();
    remove_var("AIFO_CODER_TEST_REGISTRY_PROBE");
    remove_var("AIFO_CODER_INTERNAL_REGISTRY_PREFIX");

    // Set env override and resolve (populates OnceCell and source)
    set_var("AIFO_CODER_INTERNAL_REGISTRY_PREFIX", "alpha///");
    let pref1 = aifo_coder::preferred_internal_registry_prefix_quiet();
    assert_eq!(pref1, "alpha/");
    assert_eq!(aifo_coder::preferred_internal_registry_source(), "env");

    // Internal registry has no on-disk cache
    let cache_path = td.path().join("aifo-coder.mirrorprefix");
    assert!(
        !cache_path.exists(),
        "internal registry does not use on-disk cache"
    );

    // Remove env override; next resolution should use in-process cache
    remove_var("AIFO_CODER_INTERNAL_REGISTRY_PREFIX");
    let pref2 = aifo_coder::preferred_internal_registry_prefix_quiet();
    assert_eq!(pref2, "alpha/", "cached value should persist in-process");

    // Invalidate disk cache; in-process cache still returns the same value
    aifo_coder::invalidate_registry_cache();
    assert!(
        !cache_path.exists(),
        "disk cache must be removed by invalidate"
    );
    let pref3 = aifo_coder::preferred_internal_registry_prefix_quiet();
    assert_eq!(
        pref3, "alpha/",
        "in-process cache unaffected by disk removal"
    );
}
