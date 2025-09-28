#[test]
fn test_toolchain_env_blocklist_not_forwarded_in_preview() {
    // Skip if docker isn't available on this host (align with other preview tests)
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // Set host envs that should be blocked from passthrough
    std::env::set_var("RUSTUP_TOOLCHAIN", "nightly");
    std::env::set_var("RUSTUP_HOME", "/host/rustup");
    std::env::set_var("CARGO_HOME", "/host/cargo");
    std::env::set_var("CARGO_TARGET_DIR", "/host/target");

    // Use aider as a generic CLI (same as other preview tests)
    let args = vec!["--help".to_string()];
    let (_cmd, preview) =
        aifo_coder::build_docker_cmd("aider", &args, "alpine:3.20", None).expect("preview");

    // Blocked envs should not appear with host values
    let pl = preview.to_ascii_lowercase();
    assert!(
        !pl.contains("rustup_toolchain=nightly"),
        "RUSTUP_TOOLCHAIN should be blocked; preview:\n{}",
        preview
    );
    assert!(
        !pl.contains("rustup_home=/host/rustup"),
        "RUSTUP_HOME from host should be blocked; preview:\n{}",
        preview
    );
    assert!(
        !pl.contains("cargo_home=/host/cargo"),
        "CARGO_HOME from host should be blocked; preview:\n{}",
        preview
    );
    assert!(
        !pl.contains("cargo_target_dir=/host/target"),
        "CARGO_TARGET_DIR from host should be blocked; preview:\n{}",
        preview
    );

    // Normative replacements may only be present for rust toolchain previews; skip asserting them here.
    // Cleanup env
    std::env::remove_var("RUSTUP_TOOLCHAIN");
    std::env::remove_var("RUSTUP_HOME");
    std::env::remove_var("CARGO_HOME");
    std::env::remove_var("CARGO_TARGET_DIR");
}
