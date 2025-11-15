/*
Targets in src/registry.rs:
- preferred_registry_prefix: env-empty branch (trimmed empty).
- write_registry_cache_disk: cache file presence and content "".
- preferred_registry_source: "env-empty".
*/
mod tests {
    use std::env::{remove_var, set_var};
    // use std::fs;
    use tempfile::tempdir;

    #[test]
    fn unit_env_empty_writes_cache_and_sets_source() {
        // Unique runtime dir per test file
        let td = tempdir().expect("tmpdir");
        set_var("XDG_RUNTIME_DIR", td.path());

        // Clean env and state
        remove_var("AIFO_CODER_TEST_REGISTRY_PROBE");
        set_var("AIFO_CODER_INTERNAL_REGISTRY_PREFIX", "   "); // whitespace → empty override
        aifo_coder::invalidate_registry_cache();
        aifo_coder::registry_probe_set_override_for_tests(None);

        // Resolve prefix
        let pref = aifo_coder::preferred_internal_registry_prefix_quiet();
        assert_eq!(pref, "", "env-empty forces Docker Hub");

        // Source tracking
        assert_eq!(
            aifo_coder::preferred_internal_registry_source(),
            "env-empty",
            "source must reflect env-empty override"
        );

        // Internal registry has no on-disk cache
        let cache = td.path().join("aifo-coder.mirrorprefix");
        assert!(
            !cache.exists(),
            "internal registry does not use on-disk cache"
        );

        // Cleanup env for safety
        remove_var("AIFO_CODER_REGISTRY_PREFIX");
    }
}
