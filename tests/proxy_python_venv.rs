mod support;
use std::fs;
use std::path::PathBuf;

#[test]
fn test_proxy_python_venv_precedence() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // Prepare a local .venv/bin/python that prints a sentinel
    let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let venv_dir = root.join(".venv");
    let bin_dir = venv_dir.join("bin");
    let py_path = bin_dir.join("python");

    let existed_venv = venv_dir.exists();
    let existed_bin = bin_dir.exists();
    let existed_py = py_path.exists();

    if existed_venv || existed_bin || existed_py {
        eprintln!("skipping: existing .venv detected; not modifying");
        return;
    }

    fs::create_dir_all(&bin_dir).expect("create .venv/bin failed");
    fs::write(&py_path, "#!/bin/sh\necho venv-python\n").expect("write python shim failed");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&py_path, fs::Permissions::from_mode(0o755)).expect("chmod python");
    }

    // Start python sidecar and proxy (skip if image not present locally to avoid pulling)
    let kinds = vec!["python".to_string()];
    let image = std::env::var("AIFO_CODER_TEST_PY_IMAGE")
        .unwrap_or_else(|_| "python:3.12-slim".to_string());
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
    let overrides: Vec<(String, String)> = vec![("python".to_string(), image.clone())];
    let sid = aifo_coder::toolchain_start_session(&kinds, &overrides, true, true)
        .expect("failed to start sidecar session");
    let (url, token, flag, handle) =
        aifo_coder::toolexec_start_proxy(&sid, true).expect("failed to start proxy");

    fn extract_port(u: &str) -> u16 {
        support::port_from_http_url(u)
    }
    let port = extract_port(&url);

    // POST tool=python
    let (status, _headers, body) = support::http_post_tcp(
        port,
        &[("Authorization", &format!("Bearer {}", token)), ("X-Aifo-Proto", "1")],
        &[("tool", "python"), ("cwd", ".")],
    );
    assert_eq!(status, 200, "expected 200, got status={}", status);
    let text = String::from_utf8_lossy(&body).to_string();
    assert!(text.contains("venv-python"), "venv PATH was not preferred");

    // Cleanup session
    flag.store(false, std::sync::atomic::Ordering::SeqCst);
    let _ = handle.join();
    aifo_coder::toolchain_cleanup_session(&sid, true);

    // Cleanup files we created
    let _ = fs::remove_file(&py_path);
    let _ = fs::remove_dir(&bin_dir);
    let _ = fs::remove_dir(&venv_dir);
}
