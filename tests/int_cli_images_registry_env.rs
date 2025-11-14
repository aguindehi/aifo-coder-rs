use once_cell::sync::Lazy;
use std::process::Command;
use std::sync::Mutex;

static REG_ENV_GUARD: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[test]
fn int_test_cli_images_respects_registry_env_value() {
    let _g = REG_ENV_GUARD.lock().unwrap();
    let bin = env!("CARGO_BIN_EXE_aifo-coder");

    // Force mirror probe to succeed via curl-ok
    std::env::set_var("AIFO_CODER_TEST_REGISTRY_PROBE", "curl-ok");

    let out = Command::new(bin)
        .arg("images")
        .output()
        .expect("run images");
    assert!(
        out.status.success(),
        "images exited non-zero: {:?}\nstdout:\n{}\nstderr:\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let all = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    // The CLI prints the mirror registry without trailing slash
    assert!(
        all.contains("mirror registry: repository.migros.net"),
        "expected mirror registry line to reflect mirror probe, got:\n{}",
        all
    );
    // Internal registry should be (none) when not set
    assert!(
        all.contains("internal registry: (none)"),
        "expected internal registry to be (none), got:\n{}",
        all
    );

    std::env::remove_var("AIFO_CODER_TEST_REGISTRY_PROBE");
}

#[test]
fn int_test_cli_images_respects_registry_env_empty() {
    let _g = REG_ENV_GUARD.lock().unwrap();
    let bin = env!("CARGO_BIN_EXE_aifo-coder");

    // Force mirror probe to fail → Docker Hub
    std::env::set_var("AIFO_CODER_TEST_REGISTRY_PROBE", "tcp-fail");

    let out = Command::new(bin)
        .arg("images")
        .output()
        .expect("run images");
    assert!(
        out.status.success(),
        "images exited non-zero: {:?}\nstdout:\n{}\nstderr:\n{}",
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
        all.contains("mirror registry: (none)"),
        "expected '(none)' when mirror probe fails, got:\n{}",
        all
    );
    assert!(
        all.contains("internal registry: (none)"),
        "expected internal registry to be (none), got:\n{}",
        all
    );

    std::env::remove_var("AIFO_CODER_TEST_REGISTRY_PROBE");
}
