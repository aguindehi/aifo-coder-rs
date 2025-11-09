#[test]
fn test_probe_override_wins_over_env_probe_and_keeps_source_unknown() {
    use std::env::{remove_var, set_var};

    // Unique runtime dir per test file
    let td = tempfile::tempdir().expect("tmpdir");
    set_var("XDG_RUNTIME_DIR", td.path());

    // Clean state
    aifo_coder::invalidate_registry_cache();
    remove_var("AIFO_CODER_REGISTRY_PREFIX");
    remove_var("AIFO_CODER_TEST_REGISTRY_PROBE");

    // Set both: override and env-probe; override should win and source be 'unknown'
    aifo_coder::registry_probe_set_override_for_tests(Some(
        aifo_coder::RegistryProbeTestMode::CurlOk,
    ));
    set_var("AIFO_CODER_TEST_REGISTRY_PROBE", "tcp-fail");

    let pref = aifo_coder::preferred_registry_prefix();
    assert_eq!(pref, "repository.migros.net/", "override should win");

    let src = aifo_coder::preferred_registry_source();
    assert_eq!(src, "unknown", "source should be unknown under override");

    // Override path should not write cache
    let cache_path = td.path().join("aifo-coder.regprefix");
    assert!(!cache_path.exists(), "override should not write cache");

    // Cleanup
    aifo_coder::registry_probe_set_override_for_tests(None);
    remove_var("AIFO_CODER_TEST_REGISTRY_PROBE");
}
