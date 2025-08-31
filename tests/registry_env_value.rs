#[test]
fn test_preferred_registry_prefix_env_value_trailing_slashes() {
    // Set a value with extra trailing slashes; implementation normalizes to a single trailing slash
    std::env::set_var("AIFO_CODER_REGISTRY_PREFIX", "example.com////");
    let rp = aifo_coder::preferred_registry_prefix_quiet();
    let src = aifo_coder::preferred_registry_source();
    assert_eq!(rp, "example.com/", "expected normalized single trailing slash");
    assert_eq!(src, "env", "expected source=env");
}
