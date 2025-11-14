#[test]
fn unit_test_preferred_registry_source_env_probe_tcp() {
    use std::env::{remove_var, set_var};

    // Unique runtime dir per test file
    let td = tempfile::tempdir().expect("tmpdir");
    set_var("XDG_RUNTIME_DIR", td.path());

    // Ensure env probe drives source without using network/processes
    remove_var("AIFO_CODER_REGISTRY_PREFIX");
    set_var("AIFO_CODER_TEST_REGISTRY_PROBE", "tcp-fail");

    let src = aifo_coder::preferred_registry_source();
    assert_eq!(src, "tcp", "env probe tcp-fail should yield source=tcp");

    // Cleanup
    remove_var("AIFO_CODER_TEST_REGISTRY_PROBE");
}
