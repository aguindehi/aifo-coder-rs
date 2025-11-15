/*
Targets in src/registry.rs:
- preferred_registry_source: "unknown" when override is set.
- preferred_registry_prefix_quiet: override path returns early and avoids cache.
*/
#[test]
fn unit_source_unknown_when_override_active() {
    use std::env::{remove_var, set_var};

    let td = tempfile::tempdir().expect("tmpdir");
    set_var("XDG_RUNTIME_DIR", td.path());

    // Clean env and state
    remove_var("AIFO_CODER_REGISTRY_PREFIX");
    remove_var("AIFO_CODER_TEST_REGISTRY_PROBE");
    aifo_coder::invalidate_registry_cache();

    // Activate test-only override
    aifo_coder::registry_probe_set_override_for_tests(Some(
        aifo_coder::RegistryProbeTestMode::TcpOk,
    ));

    // Source is unknown under override; prefix is returned; no cache written
    assert_eq!(aifo_coder::preferred_mirror_registry_source(), "unknown");
    let pref = aifo_coder::preferred_mirror_registry_prefix_quiet();
    assert_eq!(pref, "repository.migros.net/");

    let cache = td.path().join("aifo-coder.mirrorprefix");
    assert!(!cache.exists(), "override path must not write cache");

    // Cleanup
    aifo_coder::registry_probe_set_override_for_tests(None);
}
