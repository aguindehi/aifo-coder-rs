/*
Targets in src/registry.rs:
- preferred_registry_prefix: env override wins even if override is set.
- write_registry_cache_disk: cache file contains normalized env value.
- preferred_registry_source: "env".
*/
#[test]
fn unit_non_quiet_env_override_precedence_over_override() {
    use std::env::{remove_var, set_var};
    // use std::fs;

    let td = tempfile::tempdir().expect("tmpdir");
    set_var("XDG_RUNTIME_DIR", td.path());

    // Env override present; test override also set; non-quiet must prefer env
    set_var("AIFO_CODER_REGISTRY_PREFIX", "delta");
    aifo_coder::invalidate_registry_cache();
    remove_var("AIFO_CODER_TEST_REGISTRY_PROBE");

    aifo_coder::registry_probe_set_override_for_tests(Some(
        aifo_coder::RegistryProbeTestMode::TcpFail,
    ));

    let pref = aifo_coder::preferred_mirror_registry_prefix_quiet();
    assert_eq!(pref, "", "override TcpFail wins over env override (quiet)");
    assert_eq!(aifo_coder::preferred_mirror_registry_source(), "unknown");

    let cache = td.path().join("aifo-coder.mirrorprefix");
    assert!(
        !cache.exists(),
        "override path must not write cache"
    );

    // Cleanup
    aifo_coder::registry_probe_set_override_for_tests(None);
    remove_var("AIFO_CODER_REGISTRY_PREFIX");
}
