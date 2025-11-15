/*
Targets in src/registry.rs:
- preferred_registry_prefix: nested path normalization trims trailing slashes.
- write_registry_cache_disk: cache content "acme/registry/".
- preferred_registry_source: "env".
*/
#[test]
fn unit_env_non_empty_nested_path_normalizes() {
    use std::env::{remove_var, set_var};
    // use std::fs;

    let td = tempfile::tempdir().expect("tmpdir");
    set_var("XDG_RUNTIME_DIR", td.path());

    remove_var("AIFO_CODER_TEST_REGISTRY_PROBE");
    aifo_coder::invalidate_registry_cache();
    aifo_coder::registry_probe_set_override_for_tests(None);

    set_var("AIFO_CODER_INTERNAL_REGISTRY_PREFIX", "acme/registry///");
    let pref = aifo_coder::preferred_internal_registry_prefix_quiet();
    assert_eq!(
        pref, "acme/registry/",
        "single trailing slash normalization"
    );

    assert_eq!(aifo_coder::preferred_internal_registry_source(), "env");

    let cache = td.path().join("aifo-coder.mirrorprefix");
    assert!(
        !cache.exists(),
        "internal registry does not use on-disk cache"
    );

    remove_var("AIFO_CODER_INTERNAL_REGISTRY_PREFIX");
}
