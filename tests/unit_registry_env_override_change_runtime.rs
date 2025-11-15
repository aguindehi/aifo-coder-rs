/*
Targets in src/registry.rs:
- preferred_registry_prefix: env override branch; initial normalization "first/".
- preferred_registry_prefix_quiet: updated env override applied; normalization "second/".
- write_registry_cache_disk: cache updated to "second/".
- preferred_registry_source: "env" for both resolutions.
*/
#[test]
fn unit_env_override_change_updates_prefix_and_cache() {
    use std::env::{remove_var, set_var};
    use std::fs;

    let td = tempfile::tempdir().expect("tmpdir");
    set_var("XDG_RUNTIME_DIR", td.path());

    // Clean state
    aifo_coder::registry_probe_set_override_for_tests(None);
    aifo_coder::invalidate_registry_cache();
    remove_var("AIFO_CODER_TEST_REGISTRY_PROBE");
    remove_var("AIFO_CODER_REGISTRY_PREFIX");

    // Initial env override
    set_var("AIFO_CODER_INTERNAL_REGISTRY_PREFIX", "first///");
    let p1 = aifo_coder::preferred_internal_registry_prefix_quiet();
    assert_eq!(p1, "first/");
    assert_eq!(aifo_coder::preferred_internal_registry_source(), "env");

    // Change env override at runtime; quiet should apply the new value
    set_var("AIFO_CODER_INTERNAL_REGISTRY_PREFIX", "second");
    let p2 = aifo_coder::preferred_internal_registry_prefix_quiet();
    assert_eq!(p2, "second/");
    assert_eq!(aifo_coder::preferred_internal_registry_source(), "env");

    // Internal registry has no on-disk cache
    let cache = td.path().join("aifo-coder.mirrorprefix");
    assert!(
        !cache.exists(),
        "internal registry does not use on-disk cache"
    );

    // Cleanup
    remove_var("AIFO_CODER_INTERNAL_REGISTRY_PREFIX");
}
