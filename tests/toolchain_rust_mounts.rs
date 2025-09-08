use std::env;
use std::path::PathBuf;

#[test]
fn test_rust_mounts_host_present_preferred() {
    // Skip if docker isn't available on this host (align with other preview tests)
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    let td = tempfile::tempdir().expect("tmpdir");
    let home = td.path().join("home");
    let cargo = home.join(".cargo");
    let reg = cargo.join("registry");
    let git = cargo.join("git");
    std::fs::create_dir_all(&reg).expect("mkdir registry");
    std::fs::create_dir_all(&git).expect("mkdir git");

    // Preserve and set HOME to temp to simulate host caches
    let old_home = env::var("HOME").ok();
    env::set_var("HOME", &home);

    let name = "tc-rust-cache";
    let net = "aifo-net-x";
    let pwd = td.path().join("ws");
    std::fs::create_dir_all(&pwd).unwrap();
    let args = aifo_coder::build_sidecar_run_preview(
        name,
        Some(net),
        None,
        "rust",
        "rust:1.80-slim",
        false,
        &pwd,
        None,
    );
    let preview = aifo_coder::shell_join(&args);

    // Normative mounts must exist
    assert!(
        preview.contains(&format!("{}:/home/coder/.cargo/registry", reg.display())),
        "missing host registry mount: {}",
        preview
    );
    assert!(
        preview.contains(&format!("{}:/home/coder/.cargo/git", git.display())),
        "missing host git mount: {}",
        preview
    );

    // Restore HOME
    if let Some(v) = old_home {
        env::set_var("HOME", v);
    } else {
        env::remove_var("HOME");
    }
}

#[test]
fn test_rust_mounts_fallback_to_named_volumes_when_missing() {
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }
    let td = tempfile::tempdir().expect("tmpdir");
    let home = td.path().join("home");
    std::fs::create_dir_all(&home).unwrap();

    let old_home = env::var("HOME").ok();
    env::set_var("HOME", &home);

    let name = "tc-rust-cache";
    let net = "aifo-net-x";
    let pwd = td.path().join("ws");
    std::fs::create_dir_all(&pwd).unwrap();
    let args = aifo_coder::build_sidecar_run_preview(
        name,
        Some(net),
        None,
        "rust",
        "rust:1.80-slim",
        false,
        &pwd,
        None,
    );
    let preview = aifo_coder::shell_join(&args);

    assert!(
        preview.contains("aifo-cargo-registry:/home/coder/.cargo/registry"),
        "missing fallback named registry volume: {}",
        preview
    );
    assert!(
        preview.contains("aifo-cargo-git:/home/coder/.cargo/git"),
        "missing fallback named git volume: {}",
        preview
    );

    if let Some(v) = old_home {
        env::set_var("HOME", v);
    } else {
        env::remove_var("HOME");
    }
}

#[test]
fn test_rust_no_cache_removes_mounts() {
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }
    let td = tempfile::tempdir().expect("tmpdir");
    let pwd = td.path().join("ws");
    std::fs::create_dir_all(&pwd).unwrap();

    let args = aifo_coder::build_sidecar_run_preview(
        "tc-rust-nocache",
        Some("aifo-net-x"),
        None,
        "rust",
        "rust:1.80-slim",
        true, // no_cache
        &pwd,
        None,
    );
    let preview = aifo_coder::shell_join(&args);
    assert!(
        !preview.contains("/home/coder/.cargo/registry"),
        "registry mount should be absent when no_cache=1: {}",
        preview
    );
    assert!(
        !preview.contains("/home/coder/.cargo/git"),
        "git mount should be absent when no_cache=1: {}",
        preview
    );
}

#[test]
fn test_rust_force_named_volumes_env() {
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }
    let td = tempfile::tempdir().expect("tmpdir");
    let home = td.path().join("home");
    let cargo = home.join(".cargo");
    let reg = cargo.join("registry");
    let git = cargo.join("git");
    std::fs::create_dir_all(&reg).unwrap();
    std::fs::create_dir_all(&git).unwrap();

    let old_home = env::var("HOME").ok();
    let old_force = env::var("AIFO_TOOLCHAIN_RUST_USE_DOCKER_VOLUMES").ok();
    env::set_var("HOME", &home);
    env::set_var("AIFO_TOOLCHAIN_RUST_USE_DOCKER_VOLUMES", "1");

    let args = aifo_coder::build_sidecar_run_preview(
        "tc-rust-forcevol",
        Some("aifo-net-x"),
        None,
        "rust",
        "rust:1.80-slim",
        false,
        &td.path().to_path_buf(),
        None,
    );
    let preview = aifo_coder::shell_join(&args);
    assert!(
        preview.contains("aifo-cargo-registry:/home/coder/.cargo/registry"),
        "missing forced named registry volume: {}",
        preview
    );
    assert!(
        preview.contains("aifo-cargo-git:/home/coder/.cargo/git"),
        "missing forced named git volume: {}",
        preview
    );

    if let Some(v) = old_home {
        env::set_var("HOME", v);
    } else {
        env::remove_var("HOME");
    }
    if let Some(v) = old_force {
        env::set_var("AIFO_TOOLCHAIN_RUST_USE_DOCKER_VOLUMES", v);
    } else {
        env::remove_var("AIFO_TOOLCHAIN_RUST_USE_DOCKER_VOLUMES");
    }
}

#[test]
#[cfg(windows)]
fn test_rust_windows_defaults_to_named_volumes() {
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }
    let td = tempfile::tempdir().expect("tmpdir");
    let args = aifo_coder::build_sidecar_run_preview(
        "tc-rust-win",
        Some("aifo-net-x"),
        None,
        "rust",
        "rust:1.80-slim",
        false,
        &td.path().to_path_buf(),
        None,
    );
    let preview = aifo_coder::shell_join(&args);
    assert!(
        preview.contains("aifo-cargo-registry:/home/coder/.cargo/registry"),
        "Windows should default to named volumes (registry): {}",
        preview
    );
    assert!(
        preview.contains("aifo-cargo-git:/home/coder/.cargo/git"),
        "Windows should default to named volumes (git): {}",
        preview
    );
}
