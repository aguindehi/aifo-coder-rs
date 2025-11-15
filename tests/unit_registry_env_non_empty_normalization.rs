/*
Targets in src/registry.rs:
- preferred_registry_prefix: env non-empty normalization to single '/'.
- write_registry_cache_disk: cache file content "repo/".
- preferred_registry_source: "env".
*/
mod tests {
    use std::env::{remove_var, set_var};
    // use std::fs;
    use tempfile::tempdir;

    #[test]
    fn unit_env_non_empty_normalizes_to_single_trailing_slash() {
        let td = tempdir().expect("tmpdir");
        set_var("XDG_RUNTIME_DIR", td.path());

        // Clean env and state
        remove_var("AIFO_CODER_TEST_REGISTRY_PROBE");
        set_var("AIFO_CODER_INTERNAL_REGISTRY_PREFIX", "repo///");
        aifo_coder::invalidate_registry_cache();
        aifo_coder::registry_probe_set_override_for_tests(None);

        let pref = aifo_coder::preferred_internal_registry_prefix_quiet();
        assert_eq!(pref, "repo/", "normalize to single trailing slash");

        assert_eq!(
            aifo_coder::preferred_internal_registry_source(),
            "env",
            "source must be 'env' for non-empty override"
        );

        let cache = td.path().join("aifo-coder.mirrorprefix");
        assert!(
            !cache.exists(),
            "internal registry does not use on-disk cache"
        );

        // Cleanup
        remove_var("AIFO_CODER_INTERNAL_REGISTRY_PREFIX");
    }
}
