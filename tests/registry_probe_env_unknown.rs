/*
Targets in src/registry.rs:
- preferred_registry_prefix: env-probe unknown path (early return).
- preferred_registry_source: "unknown".
- No cache write for env-probe path.
*/
#[test]
fn env_probe_unknown_returns_empty_and_source_unknown_non_quiet() {
    use std::env::{remove_var, set_var};

    let td = tempfile::tempdir().expect("tmpdir");
    set_var("XDG_RUNTIME_DIR", td.path());

    // No env override; use unknown env-probe to avoid external IO
    remove_var("AIFO_CODER_REGISTRY_PREFIX");
    aifo_coder::registry_probe_set_override_for_tests(None);
    aifo_coder::invalidate_registry_cache();
    set_var("AIFO_CODER_TEST_REGISTRY_PROBE", "weird-mode");

    let pref = aifo_coder::preferred_registry_prefix();
    assert_eq!(pref, "", "unknown env-probe should yield empty prefix (non-quiet)");

    let src = aifo_coder::preferred_registry_source();
    assert_eq!(src, "unknown");

    // Should not write cache
    let cache = td.path().join("aifo-coder.regprefix");
    assert!(
        !cache.exists(),
        "env-probe unknown path must not write cache"
    );

    // Cleanup
    remove_var("AIFO_CODER_TEST_REGISTRY_PROBE");
}
