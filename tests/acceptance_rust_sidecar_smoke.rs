use std::env;

#[test]
#[ignore] // E2E: runs real docker flows; enable in CI lanes intentionally
fn acceptance_rust_sidecar_smoke() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // Prefer our toolchain image; allow override via env
    let image = env::var("AIFO_CODER_TEST_RUST_IMAGE")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "aifo-rust-toolchain:latest".to_string());

    // Run a few basic commands inside the rust sidecar; ensure they succeed.
    // These are lighter than a full project test but catch image/runtime regressions.
    for cmd in [
        vec!["cargo".to_string(), "--version".to_string()],
        vec!["rustc".to_string(), "--version".to_string()],
        vec!["cargo".to_string(), "nextest".to_string(), "-V".to_string()],
    ] {
        let code = aifo_coder::toolchain_run(
            "rust",
            &cmd,
            Some(&image),
            false, // allow caches
            false,
            false,
        )
        .expect("toolchain_run returned io::Result");
        assert_eq!(
            code, 0,
            "command {:?} failed inside rust sidecar (image={})",
            cmd, image
        );
    }
}
