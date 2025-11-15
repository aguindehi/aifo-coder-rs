use std::env;

#[test]
fn int_bootstrap_wrapper_present_on_official_images_and_absent_on_aifo() {
    // For consistency with other tests, skip if docker isn't available
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    let td = tempfile::tempdir().expect("tmpdir");
    let pwd = td.path();

    // Save and set marker to simulate official rust image selection
    env::set_var("AIFO_RUST_OFFICIAL_BOOTSTRAP", "1");

    // Build exec preview for rust; expect bootstrap wrapper (sh -lc and cargo nextest probe)
    let with_bootstrap = aifo_coder::shell_join(&aifo_coder::build_sidecar_exec_preview(
        "tc-rust-bootstrap",
        None,
        pwd,
        "rust",
        &["cargo".to_string(), "--version".to_string()],
    ));

    assert!(
        with_bootstrap.contains(" sh -c ")
            && with_bootstrap.contains("cargo nextest -V")
            && with_bootstrap.contains("rustup component add clippy rustfmt"),
        "expected bootstrap wrapper for official rust images; got:\n{}",
        with_bootstrap
    );

    // Now disable the marker; expect no bootstrap wrapper in preview
    env::remove_var("AIFO_RUST_OFFICIAL_BOOTSTRAP");

    let without_bootstrap = aifo_coder::shell_join(&aifo_coder::build_sidecar_exec_preview(
        "tc-rust-noboot",
        None,
        pwd,
        "rust",
        &["cargo".to_string(), "--version".to_string()],
    ));
    assert!(
        !without_bootstrap.contains(" sh -c ")
            && !without_bootstrap.contains("cargo nextest -V")
            && !without_bootstrap.contains("rustup component add clippy rustfmt"),
        "expected no bootstrap wrapper for non-official images; got:\n{}",
        without_bootstrap
    );
}
