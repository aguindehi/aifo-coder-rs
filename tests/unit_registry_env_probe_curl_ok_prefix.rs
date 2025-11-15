/*
Targets in src/registry.rs:
- preferred_registry_prefix: env-probe curl-ok path (early return).
- preferred_registry_source: "curl".
- No cache write for env-probe path.
*/
#[test]
fn unit_env_probe_curl_ok_returns_prefix_and_no_cache_non_quiet() {
    use std::env::{remove_var, set_var};

    let td = tempfile::tempdir().expect("tmpdir");
    set_var("XDG_RUNTIME_DIR", td.path());

    // No env override; use env-probe to avoid external network/process
    remove_var("AIFO_CODER_REGISTRY_PREFIX");
    aifo_coder::invalidate_registry_cache();
    aifo_coder::registry_probe_set_override_for_tests(None);
    set_var("AIFO_CODER_TEST_REGISTRY_PROBE", "curl-ok");

    let pref = aifo_coder::preferred_mirror_registry_prefix_quiet();
    assert_eq!(pref, "repository.migros.net/");

    let src = aifo_coder::preferred_mirror_registry_source();
    assert_eq!(src, "curl");

    // Should not write cache
    let cache = td.path().join("aifo-coder.mirrorprefix");
    assert!(!cache.exists(), "env-probe path must not write cache");

    // Cleanup
    remove_var("AIFO_CODER_TEST_REGISTRY_PROBE");
}
