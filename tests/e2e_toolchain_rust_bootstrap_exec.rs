use std::env;
#[ignore]
#[test]
// E2E: runs real docker flows; opt-in CI lane only
fn e2e_bootstrap_exec_installs_nextest_and_is_idempotent() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // Prefer AIFO Rust toolchain image and ensure bootstrap engages
    let old_use_official = env::var("AIFO_RUST_TOOLCHAIN_USE_OFFICIAL").ok();
    let old_bootstrap = env::var("AIFO_RUST_OFFICIAL_BOOTSTRAP").ok();
    // Do not force official image; keep only bootstrap enabled (idempotent on AIFO image)
    env::set_var("AIFO_RUST_OFFICIAL_BOOTSTRAP", "1");

    // Ensure Docker daemon reachable and official image present locally (avoid pulls)
    let runtime = aifo_coder::container_runtime_path().expect("runtime");
    let ok = std::process::Command::new(&runtime)
        .arg("ps")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !ok {
        eprintln!("skipping: Docker daemon not reachable");
        if let Some(v) = old_use_official {
            env::set_var("AIFO_RUST_TOOLCHAIN_USE_OFFICIAL", v);
        } else {
            env::remove_var("AIFO_RUST_TOOLCHAIN_USE_OFFICIAL");
        }
        if let Some(v) = old_bootstrap {
            env::set_var("AIFO_RUST_OFFICIAL_BOOTSTRAP", v);
        } else {
            env::remove_var("AIFO_RUST_OFFICIAL_BOOTSTRAP");
        }
        return;
    }
    let rust_image = std::env::var("AIFO_CODER_TEST_RUST_IMAGE")
        .unwrap_or_else(|_| "aifo-coder-toolchain-rust:latest".to_string());
    let present = std::process::Command::new(&runtime)
        .args(["image", "inspect", &rust_image])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !present {
        eprintln!(
            "skipping: {} not present locally (avoid pulling in tests)",
            rust_image
        );
        if let Some(v) = old_use_official {
            env::set_var("AIFO_RUST_TOOLCHAIN_USE_OFFICIAL", v);
        } else {
            env::remove_var("AIFO_RUST_TOOLCHAIN_USE_OFFICIAL");
        }
        if let Some(v) = old_bootstrap {
            env::set_var("AIFO_RUST_OFFICIAL_BOOTSTRAP", v);
        } else {
            env::remove_var("AIFO_RUST_OFFICIAL_BOOTSTRAP");
        }
        return;
    }

    // First run: cargo nextest -V should succeed; bootstrap will install if missing
    let code1 = aifo_coder::toolchain_run(
        "rust",
        &["cargo".to_string(), "nextest".to_string(), "-V".to_string()],
        Some(&rust_image), // use prebuilt AIFO rust toolchain image with nextest
        false,             // no_cache = false (allow default mounts)
        false,             // verbose
        false,             // dry_run
    )
    .expect("toolchain_run nextest -V (first)");
    assert_eq!(
        code1, 0,
        "first run should succeed and install cargo-nextest if missing"
    );

    // Second run: should be fast/idempotent (already installed)
    let code2 = aifo_coder::toolchain_run(
        "rust",
        &["cargo".to_string(), "nextest".to_string(), "-V".to_string()],
        Some(&rust_image),
        false,
        false,
        false,
    )
    .expect("toolchain_run nextest -V (second)");
    assert_eq!(
        code2, 0,
        "second run should also succeed (idempotent bootstrap)"
    );

    // Restore env
    if let Some(v) = old_use_official {
        env::set_var("AIFO_RUST_TOOLCHAIN_USE_OFFICIAL", v);
    } else {
        env::remove_var("AIFO_RUST_TOOLCHAIN_USE_OFFICIAL");
    }
    if let Some(v) = old_bootstrap {
        env::set_var("AIFO_RUST_OFFICIAL_BOOTSTRAP", v);
    } else {
        env::remove_var("AIFO_RUST_OFFICIAL_BOOTSTRAP");
    }
}
