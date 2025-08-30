use std::process::Command;

#[test]
fn test_cli_images_respects_flavor_env_slim() {
    let bin = env!("CARGO_BIN_EXE_aifo-coder");

    // Ensure slim flavor is selected
    std::env::set_var("AIFO_CODER_IMAGE_FLAVOR", "slim");

    let out = Command::new(bin)
        .arg("images")
        .output()
        .expect("failed to run aifo-coder images");
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

    // Expect at least one of the image lines to include '-slim:' tag
    assert!(
        all.contains("-slim:"),
        "expected slim flavor image references, got:\n{}",
        all
    );
}
