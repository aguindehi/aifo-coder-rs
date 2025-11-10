/*
Targets in src/registry.rs:
- preferred_registry_prefix_quiet: preserves existing trailing slash "omega/".
- write_registry_cache_disk: cache content "omega/".
- preferred_registry_source: "env".
*/
#[test]
fn env_override_trailing_slash_is_preserved_and_cached() {
    use std::env::{remove_var, set_var};
    use std::fs;

    let td = tempfile::tempdir().expect("tmpdir");
    set_var("XDG_RUNTIME_DIR", td.path());

    // Clean state
    aifo_coder::registry_probe_set_override_for_tests(None);
    aifo_coder::invalidate_registry_cache();
    remove_var("AIFO_CODER_TEST_REGISTRY_PROBE");
    remove_var("AIFO_CODER_REGISTRY_PREFIX");

    set_var("AIFO_CODER_REGISTRY_PREFIX", "omega/");
    let pref = aifo_coder::preferred_registry_prefix_quiet();
    assert_eq!(pref, "omega/", "existing trailing slash should be preserved");
    assert_eq!(aifo_coder::preferred_registry_source(), "env");

    let cache = td.path().join("aifo-coder.regprefix");
    let content = fs::read_to_string(&cache).expect("cache should exist");
    assert_eq!(content, "omega/");

    remove_var("AIFO_CODER_REGISTRY_PREFIX");
}
