use std::process::Command;
#[ignore]
#[test]
#[ignore] // E2E: touches real docker volumes and containers
fn e2e_rust_named_volume_ownership_init_creates_stamp_files() {
    // Skip if docker isn't available on this host
    let runtime = match aifo_coder::container_runtime_path() {
        Ok(p) => p,
        Err(_) => {
            eprintln!("skipping: docker not found in PATH");
            return;
        }
    };

    // Best-effort: remove volumes first to ensure a clean slate (ignore errors)
    let _ = Command::new(&runtime)
        .args([
            "volume",
            "rm",
            "-f",
            "aifo-cargo-registry",
            "aifo-cargo-git",
        ])
        .status();

    // Force named volumes; use official rust image for availability
    let code = aifo_coder::toolchain_run(
        "rust",
        &["cargo".to_string(), "--version".to_string()],
        Some("rust:1.80-slim"),
        false, // no_cache
        true,  // verbose (prints helper docker run command)
        false, // dry_run (must be real to trigger init)
    )
    .expect("toolchain_run returned io::Result");
    assert_eq!(code, 0, "toolchain run should succeed");

    // Inspect volumes for stamp files using the same image to avoid extra pulls
    let check = |subdir: &str| -> bool {
        let mount = format!("aifo-cargo-{}:/home/coder/.cargo/{}", subdir, subdir);
        let out = Command::new(&runtime)
            .arg("run")
            .arg("--rm")
            .arg("-v")
            .arg(mount)
            .arg("rust:1.80-slim")
            .arg("sh")
            .arg("-lc")
            .arg(format!(
                "test -f /home/coder/.cargo/{}/.aifo-init-done",
                subdir
            ))
            .status()
            .expect("docker run check");
        out.success()
    };
    assert!(
        check("registry"),
        "expected stamp file in cargo registry named volume"
    );
    assert!(
        check("git"),
        "expected stamp file in cargo git named volume"
    );

    // Second run should remain successful (idempotent)
    let code2 = aifo_coder::toolchain_run(
        "rust",
        &["cargo".to_string(), "--version".to_string()],
        Some("rust:1.80-slim"),
        false,
        false,
        false,
    )
    .expect("second run");
    assert_eq!(code2, 0, "second run should succeed (idempotent)");
}
