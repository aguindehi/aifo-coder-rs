#[ignore]
#[test]
fn e2e_smart_node_outside_workspace_runs_local_and_does_not_proxy() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // Default to a locally present codex image; allow override.
    let image = std::env::var("AIFO_CODER_TEST_IMAGE")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "aifo-coder-codex:latest".to_string());

    // Only run if the image exists locally; avoid pulling.
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

    // Run `codex --version` which is outside /workspace and should therefore be local.
    //
    // Codex is a shebang script (`#!/usr/bin/env node`), so this exercises the env trampoline path
    // (env -> node -> codex) and must still bypass the proxy.
    //
    // We enable verbose mode so the shim emits the smart bypass line, and we set the smart toggles.
    let out = std::process::Command::new("docker")
        .args([
            "run",
            "--rm",
            "-e",
            "AIFO_SHIM_SMART=1",
            "-e",
            "AIFO_SHIM_SMART_NODE=1",
            "-e",
            "AIFO_TOOLCHAIN_VERBOSE=1",
            &image,
            "sh",
            "-lc",
            "set -e; codex --version >/dev/null 2>&1; echo OK",
        ])
        .output()
        .expect("docker run");

    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();

    assert!(
        out.status.success(),
        "docker run failed: status={:?}\nstdout:\n{}\nstderr:\n{}",
        out.status.code(),
        stdout,
        stderr
    );

    // Must indicate local mode due to outside-workspace program path.
    assert!(
        stderr.contains("aifo-shim: smart: tool=node mode=local reason=outside-workspace"),
        "expected smart bypass log line in stderr; got:\n{}",
        stderr
    );

    // Must not show proxy preparation logs (best-effort: avoid false positives).
    assert!(
        !stderr.contains("preparing request to /exec"),
        "unexpected proxy path log line; got:\n{}",
        stderr
    );
}
