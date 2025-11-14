#[test]
fn unit_test_registry_probe_override_curl_fail() {
    use std::env::{remove_var, set_var};

    // Unique runtime dir per test file
    let td = tempfile::tempdir().expect("tmpdir");
    set_var("XDG_RUNTIME_DIR", td.path());

    // No env override; use test-only probe override
    remove_var("AIFO_CODER_REGISTRY_PREFIX");
    aifo_coder::invalidate_registry_cache();

    aifo_coder::registry_probe_set_override_for_tests(Some(
        aifo_coder::RegistryProbeTestMode::CurlFail,
    ));

    let pref = aifo_coder::preferred_registry_prefix();
    assert_eq!(pref, "", "CurlFail override should yield empty prefix");

    let src = aifo_coder::preferred_registry_source();
    assert_eq!(
        src, "unknown",
        "source should be unknown when override is used"
    );

    // Override should not write cache in this runtime dir
    let cache_path = td.path().join("aifo-coder.regprefix");
    assert!(!cache_path.exists(), "override should not write cache");

    // Cleanup
    aifo_coder::registry_probe_set_override_for_tests(None);
}
