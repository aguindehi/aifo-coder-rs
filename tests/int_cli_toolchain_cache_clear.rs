use std::process::Command;

#[test]
fn int_test_cli_toolchain_cache_clear_skips_or_succeeds() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }
    let bin = env!("CARGO_BIN_EXE_aifo-coder");
    let out = Command::new(bin)
        .arg("toolchain-cache-clear")
        .output()
        .expect("failed to run aifo-coder toolchain-cache-clear");
    assert!(
        out.status.success(),
        "aifo-coder toolchain-cache-clear exited non-zero: {:?}\nstdout:\n{}\nstderr:\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}
