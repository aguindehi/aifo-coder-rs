#[test]
fn test_preferred_registry_source_unknown_when_no_env_or_override() {
    use std::env::{remove_var, set_var};

    let td = tempfile::tempdir().expect("tmpdir");
    set_var("XDG_RUNTIME_DIR", td.path());

    // Ensure no env override or test-probe override and clean cache
    aifo_coder::registry_probe_set_override_for_tests(None);
    aifo_coder::invalidate_registry_cache();
    remove_var("AIFO_CODER_REGISTRY_PREFIX");
    remove_var("AIFO_CODER_TEST_REGISTRY_PROBE");

    // Without any resolution performed, source should be unknown
    let src = aifo_coder::preferred_registry_source();
    assert_eq!(src, "unknown");
}
