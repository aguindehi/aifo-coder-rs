use std::env;

fn contains_rustflags(preview: &str, needle: &str) -> bool {
    preview.contains(&format!("-e RUSTFLAGS={}", needle))
        || preview.contains(&format!("-e 'RUSTFLAGS={}'", needle))
        || preview.contains(&format!("-e \"RUSTFLAGS={}\"", needle))
}

#[test]
fn test_rust_linker_rustflags_lld_and_mold() {
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }
    let save_linker = env::var("AIFO_RUST_LINKER").ok();
    let save_rf = env::var("RUSTFLAGS").ok();

    env::remove_var("RUSTFLAGS");

    let td = tempfile::tempdir().expect("tmpdir");
    let pwd = td.path().to_path_buf();

    // lld
    env::set_var("AIFO_RUST_LINKER", "lld");
    let run_lld = aifo_coder::shell_join(&aifo_coder::build_sidecar_run_preview(
        "tc-rust-lld",
        Some("aifo-net-x"),
        None,
        "rust",
        "rust:1.80-slim",
        false,
        &pwd,
        None,
    ));
    let exec_lld = aifo_coder::shell_join(&aifo_coder::build_sidecar_exec_preview(
        "tc-rust-lld",
        None,
        &pwd,
        "rust",
        &["cargo".to_string(), "build".to_string()],
    ));
    let lld_flags = "-Clinker=clang -Clink-arg=-fuse-ld=lld";
    assert!(
        contains_rustflags(&run_lld, lld_flags),
        "missing lld RUSTFLAGS in run preview: {}",
        run_lld
    );
    assert!(
        contains_rustflags(&exec_lld, lld_flags),
        "missing lld RUSTFLAGS in exec preview: {}",
        exec_lld
    );

    // mold
    env::set_var("AIFO_RUST_LINKER", "mold");
    let run_mold = aifo_coder::shell_join(&aifo_coder::build_sidecar_run_preview(
        "tc-rust-mold",
        Some("aifo-net-x"),
        None,
        "rust",
        "rust:1.80-slim",
        false,
        &pwd,
        None,
    ));
    let exec_mold = aifo_coder::shell_join(&aifo_coder::build_sidecar_exec_preview(
        "tc-rust-mold",
        None,
        &pwd,
        "rust",
        &["cargo".to_string(), "build".to_string()],
    ));
    let mold_flags = "-Clinker=clang -Clink-arg=-fuse-ld=mold";
    assert!(
        contains_rustflags(&run_mold, mold_flags),
        "missing mold RUSTFLAGS in run preview: {}",
        run_mold
    );
    assert!(
        contains_rustflags(&exec_mold, mold_flags),
        "missing mold RUSTFLAGS in exec preview: {}",
        exec_mold
    );

    // restore env
    if let Some(v) = save_linker {
        env::set_var("AIFO_RUST_LINKER", v);
    } else {
        env::remove_var("AIFO_RUST_LINKER");
    }
    if let Some(v) = save_rf {
        env::set_var("RUSTFLAGS", v);
    } else {
        env::remove_var("RUSTFLAGS");
    }
}
