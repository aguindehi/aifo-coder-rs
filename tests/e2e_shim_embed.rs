#[ignore]
#[test]
fn e2e_embedded_shim_presence_in_agent_image() {
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

    // Verify that /opt/aifo/bin/aifo-shim exists and PATH resolves key wrappers
    let cmd = "set -e; command -v cargo >/dev/null 2>&1 && command -v npx >/dev/null 2>&1 && command -v python3 >/dev/null 2>&1 && command -v bun >/dev/null 2>&1 && [ -x /opt/aifo/bin/aifo-shim ] && echo ok";
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
#[ignore]
#[test]
fn e2e_embedded_shims_present_across_agent_images() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // Candidate images (fat and slim). We will only test those present locally.
    let candidates = vec![
        "aifo-coder-aider:latest".to_string(),
        "aifo-coder-codex:latest".to_string(),
        "aifo-coder-crush:latest".to_string(),
        "aifo-coder-aider-slim:latest".to_string(),
        "aifo-coder-codex-slim:latest".to_string(),
        "aifo-coder-crush-slim:latest".to_string(),
    ];

    let present: Vec<String> = candidates
        .into_iter()
        .filter(|image| {
            std::process::Command::new("docker")
                .args(["image", "inspect", image])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        })
        .collect();

    if present.is_empty() {
        eprintln!("skipping: no agent images present locally");
        return;
    }

    // Build the shell snippet to verify aifo-shim and all tool shims exist and are executable
    let tools = aifo_coder::shim_tool_names();

    for image in present {
        let mut script = String::from("set -e; test -x /opt/aifo/bin/aifo-shim; ");
        for t in tools {
            script.push_str(&format!("test -x \"/opt/aifo/bin/{}\"; ", t));
        }
        // Extra guard: python3 and bun wrappers are part of the embedded shim set.
        script.push_str("test -x /opt/aifo/bin/python3; test -x /opt/aifo/bin/bun; ");
        script.push_str("echo ok");

        let out = std::process::Command::new("docker")
            .args(["run", "--rm", &image, "sh", "-lc", &script])
            .output();

        match out {
            Ok(o) => {
                let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
                assert!(
                    o.status.success(),
                    "docker run failed for image {}: status={:?}, stdout={}, stderr={}",
                    image,
                    o.status.code(),
                    s,
                    String::from_utf8_lossy(&o.stderr)
                );
                assert_eq!(
                    s, "ok",
                    "unexpected output from container check for image {}: {}",
                    image, s
                );
            }
            Err(e) => panic!("failed to run docker for image {}: {}", image, e),
        }
    }
}
