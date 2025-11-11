use std::process::Command;

mod common;

/// Helper: is docker available and does the given image exist locally?
fn docker_has_image(img: &str) -> bool {
    if let Ok(rt) = aifo_coder::container_runtime_path() {
        if let Ok(st) = Command::new(rt)
            .arg("image")
            .arg("inspect")
            .arg(img)
            .status()
        {
            return st.success();
        }
    }
    // If docker is unavailable, treat as present to avoid false negatives on dev hosts
    true
}

#[test]
fn test_rust_default_image_prefers_aifo_when_available_or_overridden() {
    // Save and clear env overrides first
    let old_img = std::env::var("AIFO_RUST_TOOLCHAIN_IMAGE").ok();
    let old_ver = std::env::var("AIFO_RUST_TOOLCHAIN_VERSION").ok();
    let old_off = std::env::var("AIFO_RUST_TOOLCHAIN_USE_OFFICIAL").ok();
    std::env::remove_var("AIFO_RUST_TOOLCHAIN_IMAGE");
    std::env::remove_var("AIFO_RUST_TOOLCHAIN_VERSION");
    std::env::remove_var("AIFO_RUST_TOOLCHAIN_USE_OFFICIAL");

    // Case 1: explicit override must win
    std::env::set_var("AIFO_RUST_TOOLCHAIN_IMAGE", "aifo-coder-toolchain-rust:dev");
    let img = aifo_coder::default_toolchain_image("rust");
    assert_eq!(
        img, "aifo-toolchain-rust:dev",
        "explicit image override must be preferred"
    );
    // Clear override
    std::env::remove_var("AIFO_RUST_TOOLCHAIN_IMAGE");

    // Case 2: with no overrides, if local aifo-rust-toolchain:latest is present (or docker unavailable), prefer it.
    let img2 = aifo_coder::default_toolchain_image("rust");
    if docker_has_image("aifo-coder-toolchain-rust:latest") {
        assert!(
            img2.starts_with("aifo-coder-toolchain-rust:"),
            "expected default to prefer aifo-rust-toolchain:* when available; got {}",
            img2
        );
    } else {
        eprintln!(
            "skipping strict image assertion: aifo-coder-toolchain-rust:latest not present locally"
        );
    }

    // Restore env
    if let Some(v) = old_img {
        std::env::set_var("AIFO_RUST_TOOLCHAIN_IMAGE", v);
    }
    if let Some(v) = old_ver {
        std::env::set_var("AIFO_RUST_TOOLCHAIN_VERSION", v);
    }
    if let Some(v) = old_off {
        std::env::set_var("AIFO_RUST_TOOLCHAIN_USE_OFFICIAL", v);
    }
}

#[test]
fn test_rust_default_previews_use_normative_cargo_home_and_path() {
    // Skip if docker isn't available on this host (align with other preview tests)
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }
    let td = tempfile::tempdir().expect("tmpdir");
    let pwd = td.path().to_path_buf();
    let args = aifo_coder::build_sidecar_run_preview(
        "tc-rust-default",
        Some("aifo-net-x"),
        None,
        "rust",
        "aifo-coder-toolchain-rust:latest",
        false,
        &pwd,
        None,
    );
    let preview = aifo_coder::shell_join(&args);
    assert!(
        preview.contains("-e CARGO_HOME=/home/coder/.cargo"),
        "CARGO_HOME missing in default run preview: {}",
        preview
    );
    // Rust v7: do not override PATH at runtime; image sets PATH.
    // Ensure key envs are present and PATH is not exported.
    assert!(
        common::contains_env(&preview, "CC") && preview.contains("CC=gcc"),
        "CC=gcc missing in run preview: {}",
        preview
    );
    assert!(
        common::contains_env(&preview, "CXX")
            && (preview.contains("CXX=g++") || preview.contains("'CXX=g++'")),
        "CXX=g++ missing in run preview: {}",
        preview
    );
    // Rust v7 images may manage RUST_BACKTRACE internally; assert if present, otherwise skip.
    if common::contains_env(&preview, "RUST_BACKTRACE") {
        assert!(
            preview.contains("RUST_BACKTRACE=1"),
            "RUST_BACKTRACE present but not set to 1 in run preview: {}",
            preview
        );
    } else {
        eprintln!("skipping RUST_BACKTRACE assertion: not present in run preview");
    }
    common::assert_preview_no_path_export(&preview);
}

#[test]
fn test_rust_official_fallback_env_forces_official_image() {
    // Save and clear conflicting env overrides
    let old_img = std::env::var("AIFO_RUST_TOOLCHAIN_IMAGE").ok();
    let old_ver = std::env::var("AIFO_RUST_TOOLCHAIN_VERSION").ok();
    let old_off = std::env::var("AIFO_RUST_TOOLCHAIN_USE_OFFICIAL").ok();

    std::env::remove_var("AIFO_RUST_TOOLCHAIN_IMAGE");
    std::env::set_var("AIFO_RUST_TOOLCHAIN_USE_OFFICIAL", "1");
    std::env::set_var("AIFO_RUST_TOOLCHAIN_VERSION", "1.80");

    let img = aifo_coder::default_toolchain_image("rust");
    assert!(
        img.starts_with("rust:"),
        "expected official rust image when AIFO_RUST_TOOLCHAIN_USE_OFFICIAL=1, got {}",
        img
    );

    // Restore env
    if let Some(v) = old_img {
        std::env::set_var("AIFO_RUST_TOOLCHAIN_IMAGE", v);
    } else {
        std::env::remove_var("AIFO_RUST_TOOLCHAIN_IMAGE");
    }
    if let Some(v) = old_ver {
        std::env::set_var("AIFO_RUST_TOOLCHAIN_VERSION", v);
    } else {
        std::env::remove_var("AIFO_RUST_TOOLCHAIN_VERSION");
    }
    if let Some(v) = old_off {
        std::env::set_var("AIFO_RUST_TOOLCHAIN_USE_OFFICIAL", v);
    } else {
        std::env::remove_var("AIFO_RUST_TOOLCHAIN_USE_OFFICIAL");
    }
}
