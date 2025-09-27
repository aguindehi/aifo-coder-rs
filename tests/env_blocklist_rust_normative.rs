#[test]
fn test_rust_env_normative_replacements_present_in_preview() {
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

    // Use rust tool and official image to ensure normative env replacements are applied
    let args = vec!["--help".to_string()];
    let (_cmd, preview) =
        aifo_coder::build_docker_cmd("cargo", &args, "rust:1.80-bookworm", None).expect("preview");

    let pl = preview.to_ascii_lowercase();

    // Blocked host values must not appear
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

    // Normative replacements should be present for rust previews
    assert!(
        pl.contains("rustup_home=/usr/local/rustup")
            || pl.contains("rustup_home=/home/coder/.rustup"),
        "expected normative RUSTUP_HOME path in preview:\n{}",
        preview
    );
    assert!(
        pl.contains("cargo_home=/usr/local/cargo") || pl.contains("cargo_home=/home/coder/.cargo"),
        "expected normative CARGO_HOME path in preview:\n{}",
        preview
    );

    // Cleanup env
    std::env::remove_var("RUSTUP_TOOLCHAIN");
    std::env::remove_var("RUSTUP_HOME");
    std::env::remove_var("CARGO_HOME");
    std::env::remove_var("CARGO_TARGET_DIR");
}
