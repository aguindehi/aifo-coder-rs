#[test]
fn unit_test_env_override_wins_over_env_probe_and_persists() {
    use std::env::{remove_var, set_var};

    let td = tempfile::tempdir().expect("tmpdir");
    set_var("XDG_RUNTIME_DIR", td.path());

    // Clean state
    aifo_coder::registry_probe_set_override_for_tests(None);
    aifo_coder::invalidate_registry_cache();
    remove_var("AIFO_CODER_REGISTRY_PREFIX");
    remove_var("AIFO_CODER_TEST_REGISTRY_PROBE");

    // Set env override and resolve (populates OnceCell and source)
    set_var("AIFO_CODER_INTERNAL_REGISTRY_PREFIX", "zeta///");
    let pref1 = aifo_coder::preferred_internal_registry_prefix_quiet();
    assert_eq!(pref1, "zeta/");
    assert_eq!(aifo_coder::preferred_internal_registry_source(), "env");

    // Now set env-probe; OnceCell must keep the original env-derived value
    set_var("AIFO_CODER_TEST_REGISTRY_PROBE", "tcp-ok");
    let pref2 = aifo_coder::preferred_internal_registry_prefix_quiet();
    assert_eq!(pref2, "zeta/");
    assert_eq!(
        aifo_coder::preferred_internal_registry_source(),
        "env",
        "internal registry source remains 'env' and is unaffected by mirror env-probe"
    );

    // Cleanup
    remove_var("AIFO_CODER_TEST_REGISTRY_PROBE");
    remove_var("AIFO_CODER_INTERNAL_REGISTRY_PREFIX");
}
