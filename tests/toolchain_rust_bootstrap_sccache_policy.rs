use std::env;

#[test]
fn test_bootstrap_sccache_policy_warning_in_preview() {
    // Skip if docker isn't available on this host (align with other preview tests)
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    let td = tempfile::tempdir().expect("tmpdir");
    let pwd = td.path();

    // Save and set env to engage bootstrap and sccache policy
    let old_marker = env::var("AIFO_RUST_OFFICIAL_BOOTSTRAP").ok();
    let old_sccache = env::var("AIFO_RUST_SCCACHE").ok();
    env::set_var("AIFO_RUST_OFFICIAL_BOOTSTRAP", "1");
    env::set_var("AIFO_RUST_SCCACHE", "1");

    let preview = aifo_coder::shell_join(&aifo_coder::build_sidecar_exec_preview(
        "tc-rust-sccache-policy",
        None,
        pwd,
        "rust",
        &["cargo".to_string(), "--version".to_string()],
    ));

    // Expect bootstrap wrapper present
    assert!(
        preview.contains(" sh -c "),
        "expected bootstrap wrapper when AIFO_RUST_OFFICIAL_BOOTSTRAP=1; got:\n{}",
        preview
    );
    // Expect sccache policy message present inside the wrapper script
    assert!(
        preview.contains("sccache requested but not installed"),
        "expected sccache policy warning in bootstrap script; got:\n{}",
        preview
    );

    // Restore env
    if let Some(v) = old_marker {
        env::set_var("AIFO_RUST_OFFICIAL_BOOTSTRAP", v);
    } else {
        env::remove_var("AIFO_RUST_OFFICIAL_BOOTSTRAP");
    }
    if let Some(v) = old_sccache {
        env::set_var("AIFO_RUST_SCCACHE", v);
    } else {
        env::remove_var("AIFO_RUST_SCCACHE");
    }
}
