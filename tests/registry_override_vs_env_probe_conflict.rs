/*
Targets in src/registry.rs:
- preferred_registry_prefix_quiet: override wins even if env-probe is set.
- preferred_registry_source: "unknown" under override; no cache write.
*/
#[test]
fn override_wins_over_env_probe_conflict() {
    use std::env::{remove_var, set_var};

    let td = tempfile::tempdir().expect("tmpdir");
    set_var("XDG_RUNTIME_DIR", td.path());

    // Set conflicting env-probe and override; override must win
    remove_var("AIFO_CODER_REGISTRY_PREFIX");
    aifo_coder::invalidate_registry_cache();
    aifo_coder::registry_probe_set_override_for_tests(Some(
        aifo_coder::RegistryProbeTestMode::CurlOk,
    ));
    set_var("AIFO_CODER_TEST_REGISTRY_PROBE", "tcp-fail");

    let pref = aifo_coder::preferred_registry_prefix_quiet();
    assert_eq!(pref, "repository.migros.net/", "override must win");

    let src = aifo_coder::preferred_registry_source();
    assert_eq!(src, "unknown", "source should be unknown under override");

    // No cache write for override path
    let cache = td.path().join("aifo-coder.regprefix");
    assert!(!cache.exists(), "override path must not write cache");

    // Cleanup
    aifo_coder::registry_probe_set_override_for_tests(None);
    remove_var("AIFO_CODER_TEST_REGISTRY_PROBE");
}
