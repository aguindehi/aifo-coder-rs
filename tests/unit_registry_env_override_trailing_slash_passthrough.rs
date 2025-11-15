/*
Targets in src/registry.rs:
- preferred_registry_prefix_quiet: preserves existing trailing slash "omega/".
- write_registry_cache_disk: cache content "omega/".
- preferred_registry_source: "env".
*/
#[test]
fn unit_env_override_trailing_slash_is_preserved_and_cached() {
    use std::env::{remove_var, set_var};
    // use std::fs;

    let td = tempfile::tempdir().expect("tmpdir");
    set_var("XDG_RUNTIME_DIR", td.path());

    // Clean state
    aifo_coder::registry_probe_set_override_for_tests(None);
    aifo_coder::invalidate_registry_cache();
    remove_var("AIFO_CODER_TEST_REGISTRY_PROBE");
    remove_var("AIFO_CODER_INTERNAL_REGISTRY_PREFIX");

    set_var("AIFO_CODER_INTERNAL_REGISTRY_PREFIX", "omega/");
    let pref = aifo_coder::preferred_internal_registry_prefix_quiet();
    assert_eq!(
        pref, "omega/",
        "existing trailing slash should be preserved"
    );
    assert_eq!(aifo_coder::preferred_internal_registry_source(), "env");

    let cache = td.path().join("aifo-coder.mirrorprefix");
    assert!(
        !cache.exists(),
        "internal registry does not use on-disk cache"
    );

    remove_var("AIFO_CODER_REGISTRY_PREFIX");
}
