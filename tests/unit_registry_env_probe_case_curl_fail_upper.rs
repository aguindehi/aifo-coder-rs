/*
Targets in src/registry.rs:
- preferred_registry_prefix_quiet: env-probe case-insensitive "CURL-FAIL".
- preferred_registry_source: "curl".
- No cache write for env-probe path.
*/
#[test]
fn unit_env_probe_curl_fail_upper_returns_empty_and_no_cache_quiet() {
    use std::env::{remove_var, set_var};

    let td = tempfile::tempdir().expect("tmpdir");
    set_var("XDG_RUNTIME_DIR", td.path());

    // Ensure no env override and no test override; use env-probe (uppercase)
    remove_var("AIFO_CODER_REGISTRY_PREFIX");
    aifo_coder::registry_probe_set_override_for_tests(None);
    aifo_coder::invalidate_registry_cache();
    set_var("AIFO_CODER_TEST_REGISTRY_PROBE", "CURL-FAIL");

    let pref = aifo_coder::preferred_mirror_registry_prefix_quiet();
    assert_eq!(pref, "", "curl-fail should yield empty prefix");

    let src = aifo_coder::preferred_mirror_registry_source();
    assert_eq!(src, "curl");

    // Env-probe should not write cache
    let cache = td.path().join("aifo-coder.mirrorprefix");
    assert!(!cache.exists(), "env-probe must not write cache");

    // Cleanup
    remove_var("AIFO_CODER_TEST_REGISTRY_PROBE");
}
