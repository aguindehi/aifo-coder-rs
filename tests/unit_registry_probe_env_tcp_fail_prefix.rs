#[test]
fn unit_test_registry_env_probe_tcp_fail_prefix_and_source() {
    use std::env::{remove_var, set_var};

    let td = tempfile::tempdir().expect("tmpdir");
    set_var("XDG_RUNTIME_DIR", td.path());

    remove_var("AIFO_CODER_REGISTRY_PREFIX");
    aifo_coder::registry_probe_set_override_for_tests(None);
    aifo_coder::invalidate_registry_cache();
    set_var("AIFO_CODER_TEST_REGISTRY_PROBE", "tcp-fail");

    let pref = aifo_coder::preferred_registry_prefix();
    assert_eq!(pref, "");

    let src = aifo_coder::preferred_registry_source();
    assert_eq!(src, "tcp");

    let cache_path = td.path().join("aifo-coder.regprefix");
    assert!(!cache_path.exists());

    remove_var("AIFO_CODER_TEST_REGISTRY_PROBE");
}
