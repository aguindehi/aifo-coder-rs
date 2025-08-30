use std::process::Command;

#[test]
fn test_cli_cache_clear_exits_zero() {
    let bin = env!("CARGO_BIN_EXE_aifo-coder");
    let out = Command::new(bin)
        .arg("cache-clear")
        .output()
        .expect("failed to run aifo-coder cache-clear");
    assert!(
        out.status.success(),
        "aifo-coder cache-clear exited non-zero: {:?}\nstdout:\n{}\nstderr:\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}
