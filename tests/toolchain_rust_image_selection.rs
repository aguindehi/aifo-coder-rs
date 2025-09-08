use std::env;

#[test]
fn test_rust_image_selection_default_and_official() {
    // Preserve old env
    let old_official = env::var("AIFO_RUST_TOOLCHAIN_USE_OFFICIAL").ok();

    // Default mapping: our image for versioned rust
    env::remove_var("AIFO_RUST_TOOLCHAIN_USE_OFFICIAL");
    let img_default = aifo_coder::default_toolchain_image_for_version("rust", "1.80");
    assert_eq!(
        img_default, "rust:1.80-slim",
        "expected official rust image mapping by default"
    );

    // Official mapping when requested
    env::set_var("AIFO_RUST_TOOLCHAIN_USE_OFFICIAL", "1");
    let img_official = aifo_coder::default_toolchain_image_for_version("rust", "1.80");
    assert_eq!(
        img_official, "rust:1.80-slim",
        "expected official rust image when AIFO_RUST_TOOLCHAIN_USE_OFFICIAL=1"
    );

    // Restore env
    if let Some(v) = old_official {
        env::set_var("AIFO_RUST_TOOLCHAIN_USE_OFFICIAL", v);
    } else {
        env::remove_var("AIFO_RUST_TOOLCHAIN_USE_OFFICIAL");
    }
}

#[test]
fn test_exec_preview_includes_bootstrap_marker_when_set_for_rust_only() {
    // Preserve and set marker
    let old_bootstrap = env::var("AIFO_RUST_OFFICIAL_BOOTSTRAP").ok();
    env::set_var("AIFO_RUST_OFFICIAL_BOOTSTRAP", "1");

    let td = tempfile::tempdir().expect("tmpdir");
    let pwd = td.path();

    // Rust exec preview should include the bootstrap env
    let args_rust = aifo_coder::build_sidecar_exec_preview(
        "tc-rust",
        None,
        pwd,
        "rust",
        &["cargo".to_string(), "--version".to_string()],
    );
    let has_marker = args_rust
        .iter()
        .any(|s| s == "AIFO_RUST_OFFICIAL_BOOTSTRAP=1");
    assert!(
        has_marker,
        "expected AIFO_RUST_OFFICIAL_BOOTSTRAP=1 in rust exec preview: {:?}",
        args_rust
    );

    // Node exec preview should NOT include the rust-specific marker
    let args_node = aifo_coder::build_sidecar_exec_preview(
        "tc-node",
        None,
        pwd,
        "node",
        &["node".to_string(), "--version".to_string()],
    );
    let has_marker_node = args_node
        .iter()
        .any(|s| s == "AIFO_RUST_OFFICIAL_BOOTSTRAP=1");
    assert!(
        !has_marker_node,
        "marker must not be injected for non-rust exec preview: {:?}",
        args_node
    );

    // Restore env
    if let Some(v) = old_bootstrap {
        env::set_var("AIFO_RUST_OFFICIAL_BOOTSTRAP", v);
    } else {
        env::remove_var("AIFO_RUST_OFFICIAL_BOOTSTRAP");
    }
}
