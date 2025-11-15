/*
Targets in src/registry.rs:
- preferred_registry_prefix: env-probe tcp-fail path (early return).
- preferred_registry_source: "tcp".
- No cache write for env-probe path.
*/
#[test]
fn unit_env_probe_tcp_fail_returns_empty_and_no_cache_non_quiet() {
    use std::env::{remove_var, set_var};

    let td = tempfile::tempdir().expect("tmpdir");
    set_var("XDG_RUNTIME_DIR", td.path());

    // No env override; use env-probe to avoid external network/process
    remove_var("AIFO_CODER_REGISTRY_PREFIX");
    aifo_coder::invalidate_registry_cache();
    aifo_coder::registry_probe_set_override_for_tests(None);
    set_var("AIFO_CODER_TEST_REGISTRY_PROBE", "tcp-fail");

    let pref = aifo_coder::preferred_mirror_registry_prefix_quiet();
    assert_eq!(pref, "", "tcp-fail should yield empty prefix (non-quiet)");

    let src = aifo_coder::preferred_mirror_registry_source();
    assert_eq!(src, "tcp");

    // Should not write cache
    let cache = td.path().join("aifo-coder.regprefix");
    assert!(!cache.exists(), "env-probe path must not write cache");

    // Cleanup
    remove_var("AIFO_CODER_TEST_REGISTRY_PROBE");
}
