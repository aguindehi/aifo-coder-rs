#[test]
fn test_default_toolchain_image_for_version_unknown_kind_fallback() {
    let img = aifo_coder::default_toolchain_image_for_version("weird", "999");
    assert_eq!(
        img, "node:20-bookworm-slim",
        "unknown kind should fall back to node default image"
    );
}
