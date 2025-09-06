use once_cell::sync::Lazy;
use std::process::Command;
use std::sync::Mutex;

static REG_ENV_GUARD: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[test]
fn test_cli_images_respects_registry_env_value() {
    let _g = REG_ENV_GUARD.lock().unwrap();
    let bin = env!("CARGO_BIN_EXE_aifo-coder");

    // Save and set env
    let old = std::env::var("AIFO_CODER_REGISTRY_PREFIX").ok();
    std::env::set_var("AIFO_CODER_REGISTRY_PREFIX", "example.com/");

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
    // The CLI prints the registry without trailing slash
    assert!(
        all.contains("registry: example.com"),
        "expected registry line to reflect env override, got:\n{}",
        all
    );

    // Restore
    if let Some(v) = old {
        std::env::set_var("AIFO_CODER_REGISTRY_PREFIX", v);
    } else {
        std::env::remove_var("AIFO_CODER_REGISTRY_PREFIX");
    }
}

#[test]
fn test_cli_images_respects_registry_env_empty() {
    let _g = REG_ENV_GUARD.lock().unwrap();
    let bin = env!("CARGO_BIN_EXE_aifo-coder");

    // Save and set env to empty → Docker Hub
    let old = std::env::var("AIFO_CODER_REGISTRY_PREFIX").ok();
    std::env::set_var("AIFO_CODER_REGISTRY_PREFIX", "");

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
        all.contains("registry: Docker Hub"),
        "expected 'Docker Hub' when registry env is empty, got:\n{}",
        all
    );

    // Restore
    if let Some(v) = old {
        std::env::set_var("AIFO_CODER_REGISTRY_PREFIX", v);
    } else {
        std::env::remove_var("AIFO_CODER_REGISTRY_PREFIX");
    }
}
