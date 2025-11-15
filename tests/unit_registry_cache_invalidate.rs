/*
Targets in src/registry.rs:
- write_registry_cache_disk: cache creation via env-empty path.
- invalidate_registry_cache: removal of cache file.
*/
mod tests {
    use std::env::{remove_var, set_var};
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn unit_cache_created_and_invalidate_removes_file() {
        let td = tempdir().expect("tmpdir");
        set_var("XDG_RUNTIME_DIR", td.path());

        // Seed the mirror cache file explicitly (IR has no disk cache; env-probe does not write)
        remove_var("AIFO_CODER_TEST_REGISTRY_PROBE");
        aifo_coder::registry_probe_set_override_for_tests(None);

        let cache = td.path().join("aifo-coder.mirrorprefix");
        std::fs::write(&cache, "").expect("seed cache file");
        assert!(cache.exists(), "cache should be created");

        // Invalidate and ensure removal
        aifo_coder::invalidate_registry_cache();
        assert!(!cache.exists(), "invalidate must remove cache file");

        // Cleanup
        remove_var("AIFO_CODER_INTERNAL_REGISTRY_PREFIX");
    }
}
