#[ignore]
#[test]
fn test_embedded_shim_presence_in_agent_image() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // Allow overriding the test image; default to aifo-coder-aider:embedded-shim
    let image = std::env::var("AIFO_CODER_TEST_IMAGE")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "aifo-coder-aider:embedded-shim".to_string());

    // Only run if the image exists locally; avoid pulling
    let present = std::process::Command::new("docker")
        .args(["image", "inspect", &image])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if !present {
        eprintln!("skipping: test image not present locally: {}", image);
        return;
    }

    // Verify that /opt/aifo/bin/aifo-shim exists and PATH resolves cargo/npx
    let cmd = "set -e; command -v cargo >/dev/null 2>&1 && command -v npx >/dev/null 2>&1 && [ -x /opt/aifo/bin/aifo-shim ] && echo ok";
    let out = std::process::Command::new("docker")
        .args(["run", "--rm", &image, "sh", "-lc", cmd])
        .output();

    match out {
        Ok(o) => {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            assert!(
                o.status.success(),
                "docker run failed: status={:?}, stdout={}, stderr={}",
                o.status.code(),
                s,
                String::from_utf8_lossy(&o.stderr)
            );
            assert_eq!(s, "ok", "unexpected output from container check: {}", s);
        }
        Err(e) => panic!("failed to run docker: {}", e),
    }
}

#[test]
fn test_embedded_shim_say_present_notifications_cmd_absent() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // Allow overriding the test image; default to aifo-coder-aider:latest
    let image = std::env::var("AIFO_CODER_TEST_IMAGE")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "aifo-coder-aider:latest".to_string());

    // Only run if the image exists locally; avoid pulling
    let present = std::process::Command::new("docker")
        .args(["image", "inspect", &image])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if !present {
        eprintln!("skipping: test image not present locally: {}", image);
        return;
    }

    // Verify say exists and notifications-cmd is absent
    let cmd = r#"
        set -e
        test -x /opt/aifo/bin/aifo-shim
        test -x /opt/aifo/bin/say
        [ ! -e /opt/aifo/bin/notifications-cmd ]
        echo ok
    "#;

    let out = std::process::Command::new("docker")
        .args(["run", "--rm", &image, "sh", "-lc", cmd])
        .output();

    match out {
        Ok(o) => {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            assert!(
                o.status.success(),
                "docker run failed: status={:?}, stdout={}, stderr={}",
                o.status.code(),
                s,
                String::from_utf8_lossy(&o.stderr)
            );
            assert_eq!(s, "ok", "unexpected output from container check: {}", s);
        }
        Err(e) => panic!("failed to run docker: {}", e),
    }
}
