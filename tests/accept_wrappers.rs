#[test]
#[ignore]
fn accept_phase4_wrappers_auto_exit_present() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // Allow overriding the test image; default to aider slim/full based on env
    let image = std::env::var("AIFO_CODER_TEST_IMAGE")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| {
            // Fallback to aider image name used by the Makefile
            std::env::var("IMAGE_PREFIX")
                .map(|p| format!("{}-aider:latest", p))
                .unwrap_or_else(|_| "aifo-coder-aider:latest".to_string())
        });

    // Inspect the first 50 lines of /opt/aifo/bin/sh and assert '; exit' injection present
    let rt = aifo_coder::container_runtime_path().expect("docker path");
    // Skip if the image is not present locally to avoid pulling during acceptance tests
    let present = std::process::Command::new(&rt)
        .args(["image", "inspect", &image])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !present {
        eprintln!("skipping: image not present locally: {}", image);
        return;
    }
    let mut cmd = std::process::Command::new(&rt);
    cmd.args([
        "run",
        "--rm",
        "--entrypoint",
        "sh",
        &image,
        "-lc",
        "head -n 50 /opt/aifo/bin/sh",
    ]);
    let out = match cmd.output() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("skipping: failed to run {}: {}", image, e);
            return;
        }
    };
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        s.contains("exec /bin/sh \"$flag\" \"$cmd; exit\""),
        "wrapper should append '; exit' for -c/-lc. Wrapper content:\n{}",
        s
    );
}
