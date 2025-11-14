/*
Targets in src/registry.rs:
- preferred_registry_prefix: trims spaces and normalizes trailing slash.
- write_registry_cache_disk: cache file content "beta/".
- preferred_registry_source: "env".
*/
#[test]
fn env_override_trailing_spaces_normalizes_and_writes_cache() {
    use std::env::{remove_var, set_var};
    use std::fs;

    let td = tempfile::tempdir().expect("tmpdir");
    set_var("XDG_RUNTIME_DIR", td.path());

    // Clean env and state
    aifo_coder::registry_probe_set_override_for_tests(None);
    aifo_coder::invalidate_registry_cache();
    remove_var("AIFO_CODER_TEST_REGISTRY_PROBE");

    // Override with trailing spaces should be trimmed and normalized
    set_var("AIFO_CODER_REGISTRY_PREFIX", "beta   ");
    let pref = aifo_coder::preferred_registry_prefix();
    assert_eq!(pref, "beta/", "trailing spaces trimmed; single slash added");

    assert_eq!(
        aifo_coder::preferred_registry_source(),
        "env",
        "source must be 'env' for non-empty override"
    );

    // Cache should contain normalized value
    let cache = td.path().join("aifo-coder.regprefix");
    assert!(cache.exists(), "cache file must exist");
    let content = fs::read_to_string(&cache).expect("read cache");
    assert_eq!(content, "beta/");
}
