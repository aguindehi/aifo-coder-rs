#[test]
fn test_is_official_rust_image_variants() {
    assert!(aifo_coder::is_official_rust_image("rust:1.80-slim"));
    assert!(aifo_coder::is_official_rust_image(
        "registry.local:5000/rust:1.80"
    ));
    assert!(!aifo_coder::is_official_rust_image(
        "registry/rust-toolchain:1.80"
    ));
}

#[test]
fn test_official_rust_image_for_version() {
    // None defaults
    let def = aifo_coder::official_rust_image_for_version(None);
    assert!(
        def.contains("rust:") && def.contains("bookworm"),
        "default should be rust:<ver>-bookworm, got: {}",
        def
    );

    // Some("1.79")
    let v = aifo_coder::official_rust_image_for_version(Some("1.79"));
    assert_eq!(v, "rust:1.79-bookworm");
}
