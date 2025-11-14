#[test]
fn test_registry_cache_retrieval_path_in_non_quiet_variant() {
    use std::env::{remove_var, set_var};

    // Unique runtime dir per test file
    let td = tempfile::tempdir().expect("tmpdir");
    set_var("XDG_RUNTIME_DIR", td.path());

    // Clean state: no overrides and empty cache
    aifo_coder::registry_probe_set_override_for_tests(None);
    aifo_coder::invalidate_registry_cache();
    remove_var("AIFO_CODER_REGISTRY_PREFIX");
    remove_var("AIFO_CODER_TEST_REGISTRY_PROBE");

    // First, set env to populate OnceCell and disk cache
    set_var("AIFO_CODER_REGISTRY_PREFIX", "gamma///");
    let first = aifo_coder::preferred_registry_prefix_quiet();
    assert_eq!(first, "gamma/");
    assert_eq!(aifo_coder::preferred_registry_source(), "env");

    // Now clear env and env-probe; non-quiet variant should return cached value via get()
    remove_var("AIFO_CODER_REGISTRY_PREFIX");
    remove_var("AIFO_CODER_TEST_REGISTRY_PROBE");
    let second = aifo_coder::preferred_registry_prefix();
    assert_eq!(
        second, "gamma/",
        "non-quiet should hit REGISTRY_PREFIX_CACHE.get() and return cached value"
    );

    // Source remains from initial resolution
    assert_eq!(aifo_coder::preferred_registry_source(), "env");
}
