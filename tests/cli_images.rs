use std::process::Command;

#[test]
fn test_cli_images_prints_and_exits_zero() {
    let bin = env!("CARGO_BIN_EXE_aifo-coder");
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
    assert!(all.contains("codex:"), "output should contain 'codex:'");
    assert!(all.contains("crush:"), "output should contain 'crush:'");
    assert!(all.contains("aider:"), "output should contain 'aider:'");
}
