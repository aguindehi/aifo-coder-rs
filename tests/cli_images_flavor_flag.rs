use std::process::Command;

#[test]
fn test_cli_images_flavor_flag_slim() {
    let bin = env!("CARGO_BIN_EXE_aifo-coder");

    let out = Command::new(bin)
        .args(["--flavor", "slim", "images"])
        .output()
        .expect("failed to run aifo-coder --flavor slim images");

    assert!(
        out.status.success(),
        "aifo-coder images exited non-zero: {:?}\nstdout:\n{}\nstderr:\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let all = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        all.contains("-slim:"),
        "expected slim flavor image references in output, got:\n{}",
        all
    );
}
