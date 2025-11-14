use std::env;

#[test]
fn int_test_rust_host_cargo_config_mount() {
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }
    let td = tempfile::tempdir().expect("tmpdir");
    let home = td.path().join("home");
    let cargo = home.join(".cargo");
    std::fs::create_dir_all(&cargo).unwrap();
    let cfg = cargo.join("config.toml");
    std::fs::write(&cfg, b"[registry]\n").unwrap();

    let old_home = env::var("HOME").ok();
    let old_flag = env::var("AIFO_TOOLCHAIN_RUST_USE_HOST_CONFIG").ok();
    env::set_var("HOME", &home);
    env::set_var("AIFO_TOOLCHAIN_RUST_USE_HOST_CONFIG", "1");

    let args = aifo_coder::build_sidecar_run_preview(
        "tc-rust-hostcfg",
        Some("aifo-net-x"),
        None,
        "rust",
        "rust:1.80-slim",
        false,
        td.path(),
        None,
    );
    let preview = aifo_coder::shell_join(&args);
    assert!(
        preview.contains(&format!(
            "{}:/home/coder/.cargo/config.toml:ro",
            cfg.display()
        )),
        "missing host cargo config mount: {}",
        preview
    );

    if let Some(v) = old_home {
        env::set_var("HOME", v);
    } else {
        env::remove_var("HOME");
    }
    if let Some(v) = old_flag {
        env::set_var("AIFO_TOOLCHAIN_RUST_USE_HOST_CONFIG", v);
    } else {
        env::remove_var("AIFO_TOOLCHAIN_RUST_USE_HOST_CONFIG");
    }
}

#[test]
fn int_test_rust_ssh_agent_forwarding_mount_and_env() {
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }
    let old_flag = env::var("AIFO_TOOLCHAIN_SSH_FORWARD").ok();
    let old_sock = env::var("SSH_AUTH_SOCK").ok();
    env::set_var("AIFO_TOOLCHAIN_SSH_FORWARD", "1");
    env::set_var("SSH_AUTH_SOCK", "/tmp/test-ssh.sock");

    let td = tempfile::tempdir().expect("tmpdir");
    let args = aifo_coder::build_sidecar_run_preview(
        "tc-rust-ssh",
        Some("aifo-net-x"),
        None,
        "rust",
        "rust:1.80-slim",
        false,
        td.path(),
        None,
    );
    let preview = aifo_coder::shell_join(&args);
    assert!(
        preview.contains("-e SSH_AUTH_SOCK=/tmp/test-ssh.sock"),
        "missing SSH_AUTH_SOCK env: {}",
        preview
    );
    assert!(
        preview.contains("/tmp/test-ssh.sock:/tmp/test-ssh.sock"),
        "missing SSH_AUTH_SOCK bind mount: {}",
        preview
    );

    if let Some(v) = old_flag {
        env::set_var("AIFO_TOOLCHAIN_SSH_FORWARD", v);
    } else {
        env::remove_var("AIFO_TOOLCHAIN_SSH_FORWARD");
    }
    if let Some(v) = old_sock {
        env::set_var("SSH_AUTH_SOCK", v);
    } else {
        env::remove_var("SSH_AUTH_SOCK");
    }
}
