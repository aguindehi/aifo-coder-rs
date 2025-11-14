/*
Targets in src/registry.rs:
- preferred_registry_prefix_quiet: env-empty exact override ("" → empty).
- write_registry_cache_disk: cache file presence and empty content.
- preferred_registry_source: "env-empty".
*/
#[test]
fn unit_quiet_env_empty_exact_writes_cache_and_source_env_empty() {
    use std::env::{remove_var, set_var};
    use std::fs;

    let td = tempfile::tempdir().expect("tmpdir");
    set_var("XDG_RUNTIME_DIR", td.path());

    // Clean env and state
    remove_var("AIFO_CODER_TEST_REGISTRY_PROBE");
    aifo_coder::invalidate_registry_cache();
    aifo_coder::registry_probe_set_override_for_tests(None);

    // Exact empty string override
    set_var("AIFO_CODER_REGISTRY_PREFIX", "");
    let pref = aifo_coder::preferred_registry_prefix_quiet();
    assert_eq!(pref, "", "env-empty exact should yield empty prefix");

    assert_eq!(
        aifo_coder::preferred_registry_source(),
        "env-empty",
        "source must reflect env-empty override"
    );

    let cache = td.path().join("aifo-coder.regprefix");
    assert!(cache.exists(), "cache file must exist");
    let content = fs::read_to_string(&cache).expect("read cache");
    assert_eq!(content, "", "cache content should be empty");

    // Cleanup
    remove_var("AIFO_CODER_REGISTRY_PREFIX");
}
