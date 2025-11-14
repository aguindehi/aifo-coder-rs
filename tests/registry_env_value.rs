#[test]
fn test_registry_env_value_normalizes_and_writes_cache() {
    use std::env::{remove_var, set_var};
    use std::fs;
    use std::path::PathBuf;

    // Unique runtime dir per test file
    let td = tempfile::tempdir().expect("tmpdir");
    let rt = td.path().to_path_buf();
    set_var("XDG_RUNTIME_DIR", &rt);

    // Ensure no override mode is active
    aifo_coder::registry_probe_set_override_for_tests(None);

    // Set env override with trailing slashes; expect single trailing slash
    aifo_coder::invalidate_registry_cache();
    set_var("AIFO_CODER_REGISTRY_PREFIX", "example.com////");

    // Quiet variant also writes cache and sets source
    let pref = aifo_coder::preferred_registry_prefix_quiet();
    assert_eq!(
        pref, "example.com/",
        "normalized env prefix should end with single '/'"
    );

    let src = aifo_coder::preferred_registry_source();
    assert_eq!(src, "env", "source should be env");

    // Verify cache file contains normalized value
    let cache_path: PathBuf = rt.join("aifo-coder.regprefix");
    let content = fs::read_to_string(&cache_path).expect("cache should exist");
    assert_eq!(
        content, "example.com/",
        "cache should match normalized prefix"
    );

    // Cleanup
    remove_var("AIFO_CODER_REGISTRY_PREFIX");
    aifo_coder::invalidate_registry_cache();
}
