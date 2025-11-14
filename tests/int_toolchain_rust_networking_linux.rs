#[cfg(target_os = "linux")]
use std::env;

#[test]
fn test_rust_sidecar_network_and_add_host_linux() {
    // Skip if docker isn't available on this host (align with other preview tests)
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    let td = tempfile::tempdir().expect("tmpdir");
    let pwd = td.path().to_path_buf();

    // Always expect --network when provided
    let net = "aifo-net-test";
    let args_no_add = aifo_coder::build_sidecar_run_preview(
        "tc-rust-net",
        Some(net),
        None,
        "rust",
        "rust:1.80-slim",
        false,
        &pwd,
        None,
    );
    let preview_no_add = aifo_coder::shell_join(&args_no_add);
    assert!(
        preview_no_add.contains(&format!(" --network {net} ")),
        "expected --network {net} in run preview: {}",
        preview_no_add
    );

    // On Linux, when AIFO_TOOLEEXEC_ADD_HOST=1, expect --add-host host.docker.internal:host-gateway
    #[cfg(target_os = "linux")]
    {
        let old = env::var("AIFO_TOOLEEXEC_ADD_HOST").ok();
        env::set_var("AIFO_TOOLEEXEC_ADD_HOST", "1");
        let args_add = aifo_coder::build_sidecar_run_preview(
            "tc-rust-net",
            Some(net),
            None,
            "rust",
            "rust:1.80-slim",
            false,
            &pwd,
            None,
        );
        let preview_add = aifo_coder::shell_join(&args_add);
        assert!(
            preview_add.contains(" --add-host host.docker.internal:host-gateway "),
            "expected --add-host host.docker.internal:host-gateway on Linux: {}",
            preview_add
        );
        if let Some(v) = old {
            env::set_var("AIFO_TOOLEEXEC_ADD_HOST", v);
        } else {
            env::remove_var("AIFO_TOOLEEXEC_ADD_HOST");
        }
    }
}
