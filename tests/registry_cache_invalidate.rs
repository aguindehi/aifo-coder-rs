/*
Targets in src/registry.rs:
- write_registry_cache_disk: cache creation via env-empty path.
- invalidate_registry_cache: removal of cache file.
*/
mod tests {
    use std::env::{remove_var, set_var};
    use tempfile::tempdir;

    #[test]
    fn cache_created_and_invalidate_removes_file() {
        let td = tempdir().expect("tmpdir");
        set_var("XDG_RUNTIME_DIR", td.path());

        // Force a cache write via env-empty (empty string override)
        set_var("AIFO_CODER_REGISTRY_PREFIX", "");
        remove_var("AIFO_CODER_TEST_REGISTRY_PROBE");
        aifo_coder::registry_probe_set_override_for_tests(None);

        let _ = aifo_coder::preferred_registry_prefix();
        let cache = td.path().join("aifo-coder.regprefix");
        assert!(cache.exists(), "cache should be created");

        // Invalidate and ensure removal
        aifo_coder::invalidate_registry_cache();
        assert!(!cache.exists(), "invalidate must remove cache file");

        // Cleanup
        remove_var("AIFO_CODER_REGISTRY_PREFIX");
    }
}
