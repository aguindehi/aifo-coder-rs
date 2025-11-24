#[test]
fn unit_test_normalize_toolchain_kind_aliases() {
    assert_eq!(aifo_coder::normalize_toolchain_kind("rust"), "rust");
    assert_eq!(aifo_coder::normalize_toolchain_kind("node"), "node");
    assert_eq!(aifo_coder::normalize_toolchain_kind("ts"), "node");
    assert_eq!(aifo_coder::normalize_toolchain_kind("TypeScript"), "node");
    assert_eq!(aifo_coder::normalize_toolchain_kind("py"), "python");
    assert_eq!(aifo_coder::normalize_toolchain_kind("python"), "python");
    assert_eq!(aifo_coder::normalize_toolchain_kind("c"), "c-cpp");
    assert_eq!(aifo_coder::normalize_toolchain_kind("cpp"), "c-cpp");
    assert_eq!(aifo_coder::normalize_toolchain_kind("c-cpp"), "c-cpp");
    assert_eq!(aifo_coder::normalize_toolchain_kind("c_cpp"), "c-cpp");
    assert_eq!(aifo_coder::normalize_toolchain_kind("c++"), "c-cpp");
    assert_eq!(aifo_coder::normalize_toolchain_kind("golang"), "go");
    assert_eq!(aifo_coder::normalize_toolchain_kind("go"), "go");
    // Unknowns pass through lowercased
    assert_eq!(aifo_coder::normalize_toolchain_kind("WeIrD"), "weird");
}

#[test]
fn unit_test_default_toolchain_image_for_version_mapping() {
    // Isolate registry state to keep expectations unprefixed and deterministic
    use std::env::{remove_var, set_var};
    let td = tempfile::tempdir().expect("tmpdir");
    set_var("XDG_RUNTIME_DIR", td.path());
    remove_var("AIFO_CODER_REGISTRY_PREFIX");
    remove_var("AIFO_CODER_INTERNAL_REGISTRY_PREFIX");
    remove_var("AIFO_CODER_TEST_REGISTRY_PROBE");
    aifo_coder::registry_probe_set_override_for_tests(None);
    aifo_coder::invalidate_registry_cache();

    assert_eq!(
        aifo_coder::default_toolchain_image_for_version("rust", "1.80"),
        "aifo-coder-toolchain-rust:1.80"
    );
    assert_eq!(
        aifo_coder::default_toolchain_image_for_version("node", "20"),
        "aifo-coder-toolchain-node:20"
    );
    assert_eq!(
        aifo_coder::default_toolchain_image_for_version("typescript", "20"),
        "aifo-coder-toolchain-node:20"
    );
    assert_eq!(
        aifo_coder::default_toolchain_image_for_version("python", "3.12"),
        "python:3.12-slim"
    );
    assert_eq!(
        aifo_coder::default_toolchain_image_for_version("go", "1.22"),
        "golang:1.22-bookworm"
    );
    // c-cpp does not support versions and stays at latest toolchain image
    assert_eq!(
        aifo_coder::default_toolchain_image_for_version("c-cpp", "any"),
        "aifo-coder-toolchain-cpp:latest"
    );
}
