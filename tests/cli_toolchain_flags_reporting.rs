use std::process::Command;

#[test]
fn test_cli_toolchain_flags_verbose_dry_run_reporting() {
    let bin = env!("CARGO_BIN_EXE_aifo-coder");
    let out = Command::new(bin)
        .args([
            "--verbose",
            "--dry-run",
            "--toolchain",
            "rust",
            "--toolchain-spec",
            "node@20",
            "--toolchain-image",
            "python=python:3.12-slim",
            "aider",
            "--",
            "--version",
        ])
        .output()
        .expect("failed to run aifo-coder with toolchain flags in dry-run");
    assert!(
        out.status.success(),
        "aifo-coder exited non-zero: {:?}\nstdout:\n{}\nstderr:\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("would attach toolchains:"),
        "expected verbose toolchain attach reporting; stderr:\n{}",
        err
    );
    assert!(
        err.contains("\"rust\"") && err.contains("\"node\""),
        "expected rust and node listed in toolchains; stderr:\n{}",
        err
    );
    assert!(
        err.contains("would use image overrides:"),
        "expected image overrides reporting; stderr:\n{}",
        err
    );
    assert!(
        err.contains("python:3.12-slim"),
        "expected explicit python override present; stderr:\n{}",
        err
    );
    assert!(
        err.contains("aifo-node-toolchain:20"),
        "expected node@20 default mapping present; stderr:\n{}",
        err
    );
}
