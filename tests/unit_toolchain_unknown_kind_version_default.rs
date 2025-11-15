#[test]
fn unit_test_default_toolchain_image_for_version_unknown_kind_fallback() {
    let img = aifo_coder::default_toolchain_image_for_version("weird", "999");
    let prefix = "node:";
    let suffix = "-bookworm-slim";
    assert!(
        img.starts_with(prefix) && img.ends_with(suffix) && {
            let ver = &img[prefix.len()..img.len() - suffix.len()];
            let b = ver.as_bytes();
            b.len() == 2
                && (b[0] as char >= '2' && b[0] as char <= '9')
                && (b[1] as char).is_ascii_digit()
        },
        "unknown kind should fall back to node:[2-9][0-9]-bookworm-slim, got {}",
        img
    );
}
