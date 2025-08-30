#[test]
fn test_preferred_registry_prefix_env_empty() {
    // Ensure the environment override is set to empty, forcing Docker Hub (no prefix)
    std::env::set_var("AIFO_CODER_REGISTRY_PREFIX", "");
    let rp = aifo_coder::preferred_registry_prefix_quiet();
    let src = aifo_coder::preferred_registry_source();
    assert_eq!(rp, "", "expected empty registry prefix for env-empty override");
    assert_eq!(src, "env-empty", "expected source=env-empty");
}
