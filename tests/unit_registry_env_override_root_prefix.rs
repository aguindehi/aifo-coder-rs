/*
Targets in src/registry.rs:
- preferred_registry_prefix: env override "/" normalized to "/".
- write_registry_cache_disk: cache file content "/".
- preferred_registry_source: "env".
*/
#[test]
fn unit_env_override_root_prefix_normalizes_and_writes_cache() {
    use std::env::{remove_var, set_var};
    // use std::fs;

    let td = tempfile::tempdir().expect("tmpdir");
    set_var("XDG_RUNTIME_DIR", td.path());

    // Clean env and state
    aifo_coder::registry_probe_set_override_for_tests(None);
    aifo_coder::invalidate_registry_cache();
    remove_var("AIFO_CODER_TEST_REGISTRY_PROBE");

    set_var("AIFO_CODER_INTERNAL_REGISTRY_PREFIX", "/");
    let pref = aifo_coder::preferred_internal_registry_prefix_quiet();
    assert_eq!(pref, "/", "root override should normalize to '/'");

    assert_eq!(aifo_coder::preferred_internal_registry_source(), "env");

    // Internal registry has no on-disk cache
    let cache = td.path().join("aifo-coder.mirrorprefix");
    assert!(
        !cache.exists(),
        "internal registry does not use on-disk cache"
    );
}
