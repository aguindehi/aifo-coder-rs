#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
#[ignore]
#[test]
fn node_named_cache_ownership_stamp_files() {
    // Skip if docker isn't available on this host
    let rt = match aifo_coder::container_runtime_path() {
        Ok(p) => p,
        Err(_) => {
            eprintln!("skipping: docker not found in PATH");
            return;
        }
    };

    // Start node sidecar and run a harmless command to init cache
    let image = std::env::var("AIFO_CODER_TEST_NODE_IMAGE")
        .unwrap_or_else(|_| "node:20-bookworm-slim".into());
    let img_ok = std::process::Command::new(&rt)
        .args(["image", "inspect", &image])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !img_ok {
        eprintln!("skipping: node image '{}' not present locally", image);
        return;
    }

    let kinds = vec!["node".to_string()];
    let overrides = vec![("node".to_string(), image.clone())];
    let sid =
        aifo_coder::toolchain_start_session(&kinds, &overrides, false, true).expect("sidecar");
    // Best-effort: run a simple command to trigger cache init
    let (_cmd, preview) =
        aifo_coder::build_docker_cmd("node", &vec!["--version".into()], &image, None)
            .expect("preview");
    eprintln!("preview: {}", preview);

    // Inspect named cache volume for stamp file
    let status = std::process::Command::new(&rt)
        .args([
            "run",
            "--rm",
            "-v",
            "aifo-node-cache:/home/coder/.cache",
            "alpine:3.20",
            "sh",
            "-lc",
            "test -f /home/coder/.cache/.aifo-init-done",
        ])
        .status()
        .expect("inspect volume");
    assert!(
        status.success(),
        "expected stamp file in aifo-node-cache volume"
    );

    // Cleanup session
    aifo_coder::toolchain_cleanup_session(&sid, true);
}
