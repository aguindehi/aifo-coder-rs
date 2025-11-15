mod support;
use std::fs;
use std::path::PathBuf;

#[test]
fn int_proxy_tsc_prefers_local_compiler() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // Prepare a local ./node_modules/.bin/tsc that prints a sentinel
    let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let nm_dir = root.join("node_modules");
    let bin_dir = nm_dir.join(".bin");
    let tsc_path = bin_dir.join("tsc");

    let existed_nm = nm_dir.exists();
    let existed_bin = bin_dir.exists();
    let existed_tsc = tsc_path.exists();

    if existed_tsc {
        eprintln!("skipping: node_modules/.bin/tsc already exists; not overriding");
        return;
    }

    fs::create_dir_all(&bin_dir).expect("create node_modules/.bin failed");
    fs::write(&tsc_path, "#!/bin/sh\necho local-tsc\n").expect("write tsc shim failed");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tsc_path, fs::Permissions::from_mode(0o755)).expect("chmod tsc");
    }

    // Start node sidecar and proxy (skip if image not present locally to avoid pulling)
    let kinds = vec!["node".to_string()];
    let image = support::default_node_test_image();
    let rt = aifo_coder::container_runtime_path().expect("runtime");
    let present = support::docker_image_present(&rt.as_path(), &image);
    if !present {
        eprintln!("skipping: test image not present locally: {}", image);
        return;
    }
    let overrides: Vec<(String, String)> = vec![("node".to_string(), image.clone())];
    let sid = aifo_coder::toolchain_start_session(&kinds, &overrides, true, true)
        .expect("failed to start sidecar session");
    let (url, token, flag, handle) =
        aifo_coder::toolexec_start_proxy(&sid, true).expect("failed to start proxy");

    fn extract_port(u: &str) -> u16 {
        support::port_from_http_url(u)
    }
    let port = extract_port(&url);

    // POST tool=tsc
    let (status, _headers, body) = support::http_post_tcp(
        port,
        &[
            ("Authorization", &format!("Bearer {}", token)),
            ("X-Aifo-Proto", "1"),
        ],
        &[("tool", "tsc"), ("cwd", ".")],
    );
    assert_eq!(status, 200, "expected 200, got status={}", status);
    let text = String::from_utf8_lossy(&body).to_string();
    assert!(
        text.contains("local-tsc"),
        "tsc did not come from local node_modules"
    );

    // Cleanup session first
    flag.store(false, std::sync::atomic::Ordering::SeqCst);
    let _ = handle.join();
    aifo_coder::toolchain_cleanup_session(&sid, true);

    // Cleanup files we created
    let _ = fs::remove_file(&tsc_path);
    if !existed_bin {
        let _ = fs::remove_dir(&bin_dir);
    }
    if !existed_nm {
        let _ = fs::remove_dir(&nm_dir);
    }
}
