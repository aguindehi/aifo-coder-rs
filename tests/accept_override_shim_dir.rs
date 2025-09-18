#[test]
#[ignore]
fn accept_phase4_host_override_shim_dir_script_active() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // Prepare temp dir with host-generated shims
    let td = tempfile::tempdir().expect("tmpdir");
    let shim_dir = td.path();
    aifo_coder::toolchain_write_shims(shim_dir).expect("write shims");
    // On macOS, Docker Desktop may not share /private/var/folders/... with containers.
    // Copy shims to a mount path under $HOME to ensure visibility inside the container.
    let mount_dir = if cfg!(target_os = "macos") {
        let home = home::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/tmp"));
        let md = home.join(format!(".aifo-shim-accept-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&md);
        std::fs::create_dir_all(&md).expect("create mount_dir");
        for ent in std::fs::read_dir(shim_dir).expect("read shim_dir") {
            let ent = ent.expect("dirent");
            let src = ent.path();
            let dst = md.join(ent.file_name());
            // best-effort copy; ignore perms errors
            let _ = std::fs::copy(&src, &dst);
        }
        md
    } else {
        shim_dir.to_path_buf()
    };

    // Choose agent image (prefer env override; fallback to aider)
    let image = std::env::var("AIFO_CODER_TEST_IMAGE")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| {
            std::env::var("IMAGE_PREFIX")
                .map(|p| format!("{}-aider:latest", p))
                .unwrap_or_else(|_| "aifo-coder-aider:latest".to_string())
        });

    // Skip if image not present locally
    let rt = aifo_coder::container_runtime_path().expect("docker path");
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

    // Run container with host override mounted read-only and inspect aifo-shim
    let out = std::process::Command::new(&rt)
        .args([
            "run",
            "--rm",
            "-v",
            &format!("{}:/opt/aifo/bin:ro", mount_dir.display()),
            "--entrypoint",
            "sh",
            &image,
            "-lc",
            "head -n 1 /opt/aifo/bin/aifo-shim",
        ])
        .output()
        .expect("docker run");
    let line = String::from_utf8_lossy(&out.stdout).trim().to_string();
    assert!(
        line.starts_with("#!/bin/sh"),
        "expected script shim to be active (shebang '#!/bin/sh'); got: {}",
        line
    );
}
