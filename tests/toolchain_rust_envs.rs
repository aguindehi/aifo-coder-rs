use std::env;

mod common;

#[test]
fn test_rust_envs_in_run_and_exec_previews() {
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // Save and set proxy/cargo networking envs
    let saved: Vec<(&'static str, Option<String>)> = vec![
        ("HTTP_PROXY", env::var("HTTP_PROXY").ok()),
        ("HTTPS_PROXY", env::var("HTTPS_PROXY").ok()),
        ("NO_PROXY", env::var("NO_PROXY").ok()),
        ("http_proxy", env::var("http_proxy").ok()),
        ("https_proxy", env::var("https_proxy").ok()),
        ("no_proxy", env::var("no_proxy").ok()),
        (
            "CARGO_NET_GIT_FETCH_WITH_CLI",
            env::var("CARGO_NET_GIT_FETCH_WITH_CLI").ok(),
        ),
        (
            "CARGO_REGISTRIES_CRATES_IO_PROTOCOL",
            env::var("CARGO_REGISTRIES_CRATES_IO_PROTOCOL").ok(),
        ),
        ("RUSTFLAGS", env::var("RUSTFLAGS").ok()),
        ("RUST_BACKTRACE", env::var("RUST_BACKTRACE").ok()),
    ];
    env::set_var("HTTP_PROXY", "http://proxy.example:8080");
    env::set_var("HTTPS_PROXY", "http://proxy.example:8443");
    env::set_var("NO_PROXY", "localhost,127.0.0.1");
    env::set_var("http_proxy", "http://proxy.example:8080");
    env::set_var("https_proxy", "http://proxy.example:8443");
    env::set_var("no_proxy", "localhost,127.0.0.1");
    env::set_var("CARGO_NET_GIT_FETCH_WITH_CLI", "true");
    env::set_var("CARGO_REGISTRIES_CRATES_IO_PROTOCOL", "sparse");
    env::remove_var("RUSTFLAGS");
    env::remove_var("RUST_BACKTRACE");

    let td = tempfile::tempdir().expect("tmpdir");
    let pwd = td.path().to_path_buf();

    // Run preview
    let run_args = aifo_coder::build_sidecar_run_preview(
        "tc-rust-envs",
        Some("aifo-net-x"),
        None,
        "rust",
        "rust:1.80-slim",
        false,
        &pwd,
        None,
    );
    let run_preview = aifo_coder::shell_join(&run_args);

    assert!(
        run_preview.contains("-e CARGO_HOME=/home/coder/.cargo"),
        "CARGO_HOME missing in run preview: {}",
        run_preview
    );
    common::assert_preview_path_includes(
        &run_preview,
        &["/home/coder/.cargo/bin", "/usr/local/cargo/bin"]
    );
    assert!(
        run_preview.contains("-e RUST_BACKTRACE=1"),
        "RUST_BACKTRACE default missing in run preview: {}",
        run_preview
    );
    // Proxies and cargo networking envs
    for key in &[
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "NO_PROXY",
        "http_proxy",
        "https_proxy",
        "no_proxy",
        "CARGO_NET_GIT_FETCH_WITH_CLI",
        "CARGO_REGISTRIES_CRATES_IO_PROTOCOL",
    ] {
        assert!(
            common::contains_env(&run_preview, key),
            "missing {} passthrough in run preview: {}",
            key,
            run_preview
        );
    }

    // Exec preview
    let exec_args = aifo_coder::build_sidecar_exec_preview(
        "tc-rust-envs",
        None,
        &pwd,
        "rust",
        &["cargo".to_string(), "--version".to_string()],
    );
    let exec_preview = aifo_coder::shell_join(&exec_args);

    assert!(
        exec_preview.contains("-e CARGO_HOME=/home/coder/.cargo"),
        "CARGO_HOME missing in exec preview: {}",
        exec_preview
    );
    common::assert_preview_path_includes(
        &exec_preview,
        &["/home/coder/.cargo/bin", "/usr/local/cargo/bin"]
    );
    assert!(
        exec_preview.contains("-e RUST_BACKTRACE=1"),
        "RUST_BACKTRACE default missing in exec preview: {}",
        exec_preview
    );
    for key in &[
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "NO_PROXY",
        "http_proxy",
        "https_proxy",
        "no_proxy",
        "CARGO_NET_GIT_FETCH_WITH_CLI",
        "CARGO_REGISTRIES_CRATES_IO_PROTOCOL",
    ] {
        assert!(
            common::contains_env(&exec_preview, key),
            "missing {} passthrough in exec preview: {}",
            key,
            exec_preview
        );
    }

    // Restore env
    for (k, v) in saved {
        if let Some(val) = v {
            env::set_var(k, val);
        } else {
            env::remove_var(k);
        }
    }
}
