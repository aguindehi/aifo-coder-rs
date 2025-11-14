/*
Targets in src/registry.rs:
- preferred_registry_prefix_quiet: override CurlFail path (early return).
- preferred_registry_source: "unknown" when override set.
- No cache write for override path.
*/
#[test]
fn quiet_override_curl_fail_returns_empty_and_no_cache() {
    use std::env::{remove_var, set_var};

    let td = tempfile::tempdir().expect("tmpdir");
    set_var("XDG_RUNTIME_DIR", td.path());

    // Clean env and state
    remove_var("AIFO_CODER_REGISTRY_PREFIX");
    remove_var("AIFO_CODER_TEST_REGISTRY_PROBE");
    aifo_coder::invalidate_registry_cache();

    // Activate test-only override
    aifo_coder::registry_probe_set_override_for_tests(Some(
        aifo_coder::RegistryProbeTestMode::CurlFail,
    ));

    let pref = aifo_coder::preferred_registry_prefix_quiet();
    assert_eq!(pref, "", "CurlFail should yield empty prefix");

    let src = aifo_coder::preferred_registry_source();
    assert_eq!(src, "unknown");

    // Should not write cache
    let cache = td.path().join("aifo-coder.regprefix");
    assert!(!cache.exists(), "override path must not write cache");

    // Cleanup
    aifo_coder::registry_probe_set_override_for_tests(None);
}
