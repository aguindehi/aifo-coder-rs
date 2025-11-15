#[test]
fn unit_test_registry_quiet_env_probe_curl_ok_prefix_and_source() {
    use std::env::{remove_var, set_var};

    let td = tempfile::tempdir().expect("tmpdir");
    set_var("XDG_RUNTIME_DIR", td.path());

    // Clean env and use env-probe to avoid external processes
    remove_var("AIFO_CODER_REGISTRY_PREFIX");
    aifo_coder::registry_probe_set_override_for_tests(None);
    aifo_coder::invalidate_registry_cache();
    set_var("AIFO_CODER_TEST_REGISTRY_PROBE", "curl-ok");

    let pref = aifo_coder::preferred_mirror_registry_prefix_quiet();
    assert_eq!(
        pref, "repository.migros.net/",
        "curl-ok should yield migros registry prefix (quiet)"
    );

    let src = aifo_coder::preferred_mirror_registry_source();
    assert_eq!(src, "curl", "source should be 'curl' for curl-ok env probe");

    // Env-probe branch returns immediately; cache file should not be written
    let cache_path = td.path().join("aifo-coder.mirrorprefix");
    assert!(!cache_path.exists(), "env-probe should not write cache");

    remove_var("AIFO_CODER_TEST_REGISTRY_PROBE");
}
