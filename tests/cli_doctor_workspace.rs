mod support;
use std::process::Command;

fn docker_present() -> bool {
    aifo_coder::container_runtime_path().is_ok()
}

fn image_present(img: &str) -> bool {
    if let Ok(rt) = aifo_coder::container_runtime_path() {
        return support::docker_image_present(&rt.as_path(), img);
    }
    false
}

fn default_images() -> Vec<String> {
    let prefix =
        std::env::var("AIFO_CODER_IMAGE_PREFIX").unwrap_or_else(|_| "aifo-coder".to_string());
    let tag = std::env::var("AIFO_CODER_IMAGE_TAG").unwrap_or_else(|_| "latest".to_string());
    vec![
        format!("{}-crush:{}", prefix, tag),
        format!("{}-crush-slim:{}", prefix, tag),
    ]
}

#[test]
fn test_cli_doctor_reports_workspace_writable_when_image_present() {
    if !docker_present() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }
    // Skip if neither full nor slim crush image is present locally to avoid pull
    let imgs = default_images();
    if !imgs.iter().any(|i| image_present(i)) {
        eprintln!("skipping: no local crush/crush-slim images found");
        return;
    }

    let bin = env!("CARGO_BIN_EXE_aifo-coder");
    let out = Command::new(bin)
        .arg("doctor")
        .output()
        .expect("run aifo-coder doctor");
    assert!(out.status.success(), "doctor should succeed");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("workspace writable:"),
        "doctor output should include workspace writable line; stderr:\n{}",
        err
    );
}
