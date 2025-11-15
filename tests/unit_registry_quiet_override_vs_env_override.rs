/*
Targets in src/registry.rs:
- preferred_registry_prefix_quiet: override takes precedence over env override.
- preferred_registry_source: "unknown" under override; no cache write.
*/
#[test]
fn unit_quiet_override_precedence_over_env_override() {
    use std::env::{remove_var, set_var};

    let td = tempfile::tempdir().expect("tmpdir");
    set_var("XDG_RUNTIME_DIR", td.path());

    // Set env override but also activate test override; quiet must prefer override
    set_var("AIFO_CODER_REGISTRY_PREFIX", "gamma///");
    aifo_coder::invalidate_registry_cache();
    remove_var("AIFO_CODER_TEST_REGISTRY_PROBE");

    aifo_coder::registry_probe_set_override_for_tests(Some(
        aifo_coder::RegistryProbeTestMode::TcpFail,
    ));

    let pref = aifo_coder::preferred_mirror_registry_prefix_quiet();
    assert_eq!(pref, "", "override TcpFail wins over env override in quiet");

    assert_eq!(aifo_coder::preferred_mirror_registry_source(), "unknown");

    // No cache write on override path
    let cache = td.path().join("aifo-coder.mirrorprefix");
    assert!(!cache.exists(), "override path must not write cache");

    // Cleanup
    aifo_coder::registry_probe_set_override_for_tests(None);
    remove_var("AIFO_CODER_REGISTRY_PREFIX");
}
