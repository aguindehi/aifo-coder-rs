/*
Targets in src/registry.rs:
- preferred_registry_prefix: trims leading spaces; adds single trailing slash.
- write_registry_cache_disk: cache file content "gamma/".
- preferred_registry_source: "env".
*/
#[test]
fn unit_env_override_leading_spaces_normalizes_and_writes_cache() {
    use std::env::{remove_var, set_var};
    use std::fs;

    let td = tempfile::tempdir().expect("tmpdir");
    set_var("XDG_RUNTIME_DIR", td.path());

    // Clean env and state
    aifo_coder::registry_probe_set_override_for_tests(None);
    aifo_coder::invalidate_registry_cache();
    remove_var("AIFO_CODER_TEST_REGISTRY_PROBE");

    // Leading spaces should be trimmed; trailing slash added
    set_var("AIFO_CODER_REGISTRY_PREFIX", "   gamma");
    let pref = aifo_coder::preferred_registry_prefix();
    assert_eq!(pref, "gamma/", "leading spaces trimmed; single slash added");

    assert_eq!(aifo_coder::preferred_registry_source(), "env");

    // Cache should contain normalized value
    let cache = td.path().join("aifo-coder.regprefix");
    let content = fs::read_to_string(&cache).expect("read cache");
    assert_eq!(content, "gamma/");
}
