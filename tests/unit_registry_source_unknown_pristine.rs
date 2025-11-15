/*
Targets in src/registry.rs:
- preferred_mirror_registry_source: "unknown" with no prior resolution or envs.
Note: We avoid calling prefix resolution to keep OnceCell unset.
*/
#[test]
fn unit_source_unknown_in_pristine_state() {
    use std::env::{remove_var, set_var};

    let td = tempfile::tempdir().expect("tmpdir");
    set_var("XDG_RUNTIME_DIR", td.path());

    // Clean env and state; don't resolve prefix to keep OnceCell empty
    remove_var("AIFO_CODER_REGISTRY_PREFIX");
    remove_var("AIFO_CODER_TEST_REGISTRY_PROBE");
    aifo_coder::invalidate_registry_cache();
    aifo_coder::registry_probe_set_override_for_tests(None);

    // In pristine state, source reports "unknown"
    assert_eq!(aifo_coder::preferred_mirror_registry_source(), "unknown");
}
