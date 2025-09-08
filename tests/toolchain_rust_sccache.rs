use std::env;

#[test]
fn test_rust_sccache_default_volume_and_envs() {
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }
    let old = env::var("AIFO_RUST_SCCACHE").ok();
    env::set_var("AIFO_RUST_SCCACHE", "1");
    env::remove_var("AIFO_RUST_SCCACHE_DIR");

    let td = tempfile::tempdir().expect("tmpdir");
    let args = aifo_coder::build_sidecar_run_preview(
        "tc-rust-sccache",
        Some("aifo-net-x"),
        None,
        "rust",
        "rust:1.80-slim",
        false,
        &td.path().to_path_buf(),
        None,
    );
    let preview = aifo_coder::shell_join(&args);
    assert!(
        preview.contains("aifo-sccache:/home/coder/.cache/sccache"),
        "missing default sccache volume mount: {}",
        preview
    );
    assert!(
        preview.contains("-e RUSTC_WRAPPER=sccache"),
        "missing RUSTC_WRAPPER env: {}",
        preview
    );
    assert!(
        preview.contains("-e SCCACHE_DIR=/home/coder/.cache/sccache"),
        "missing SCCACHE_DIR env: {}",
        preview
    );

    if let Some(v) = old {
        env::set_var("AIFO_RUST_SCCACHE", v);
    } else {
        env::remove_var("AIFO_RUST_SCCACHE");
    }
}

#[test]
fn test_rust_sccache_dir_override() {
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }
    let td = tempfile::tempdir().expect("tmpdir");
    let dir = td.path().join("sc");
    std::fs::create_dir_all(&dir).unwrap();

    let old = env::var("AIFO_RUST_SCCACHE").ok();
    let old_dir = env::var("AIFO_RUST_SCCACHE_DIR").ok();
    env::set_var("AIFO_RUST_SCCACHE", "1");
    env::set_var("AIFO_RUST_SCCACHE_DIR", &dir);

    let args = aifo_coder::build_sidecar_run_preview(
        "tc-rust-sccache-dir",
        Some("aifo-net-x"),
        None,
        "rust",
        "rust:1.80-slim",
        false,
        &td.path().to_path_buf(),
        None,
    );
    let preview = aifo_coder::shell_join(&args);
    assert!(
        preview.contains(&format!("{}:/home/coder/.cache/sccache", dir.display())),
        "missing sccache dir mount override: {}",
        preview
    );

    if let Some(v) = old {
        env::set_var("AIFO_RUST_SCCACHE", v);
    } else {
        env::remove_var("AIFO_RUST_SCCACHE");
    }
    if let Some(v) = old_dir {
        env::set_var("AIFO_RUST_SCCACHE_DIR", v);
    } else {
        env::remove_var("AIFO_RUST_SCCACHE_DIR");
    }
}
