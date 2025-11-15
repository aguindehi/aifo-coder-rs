#[test]
fn unit_test_quiet_probe_override_curl_ok_yields_prefix_and_no_cache() {
    use std::env::{remove_var, set_var};

    // Unique runtime dir per test file
    let td = tempfile::tempdir().expect("tmpdir");
    set_var("XDG_RUNTIME_DIR", td.path());

    // Clean state
    aifo_coder::invalidate_registry_cache();
    remove_var("AIFO_CODER_REGISTRY_PREFIX");
    remove_var("AIFO_CODER_TEST_REGISTRY_PROBE");

    // Quiet variant with override CurlOk should yield migros prefix
    aifo_coder::registry_probe_set_override_for_tests(Some(
        aifo_coder::RegistryProbeTestMode::CurlOk,
    ));
    let pref = aifo_coder::preferred_mirror_registry_prefix_quiet();
    assert_eq!(
        pref, "repository.migros.net/",
        "CurlOk override should yield migros prefix (quiet)"
    );

    let src = aifo_coder::preferred_mirror_registry_source();
    assert_eq!(src, "unknown", "source should be unknown under override");

    // Override path should not write cache
    let cache_path = td.path().join("aifo-coder.mirrorprefix");
    assert!(!cache_path.exists(), "override should not write cache");

    // Cleanup
    aifo_coder::registry_probe_set_override_for_tests(None);
}
