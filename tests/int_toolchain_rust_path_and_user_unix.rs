mod common;

#[test]
fn int_rust_run_and_exec_include_user_flags_and_path_env() {
    // For consistency with other tests, skip if docker isn't available
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // On non-Unix platforms, user flags (-u/--user) are not used; skip assertions there.
    #[cfg(not(unix))]
    {
        eprintln!("skipping: user flag assertions are Unix-only");
        return;
    }

    #[cfg(unix)]
    {
        let td = tempfile::tempdir().expect("tmpdir");
        let pwd = td.path().to_path_buf();

        // Choose arbitrary uid:gid for preview
        let (uid, gid) = (123u32, 456u32);

        // Run preview: expect --user and PATH/CARGO_HOME envs
        let run_args = aifo_coder::build_sidecar_run_preview(
            "tc-rust-user",
            Some("aifo-net-x"),
            Some((uid, gid)),
            "rust",
            "rust:1.80-slim",
            false,
            &pwd,
            None,
        );
        let run_preview = aifo_coder::shell_join(&run_args);
        assert!(
            run_preview.contains(&format!(" --user {uid}:{gid} ")),
            "expected --user {uid}:{gid} in run preview: {}",
            run_preview
        );
        assert!(
            run_preview.contains("-e CARGO_HOME=/home/coder/.cargo"),
            "CARGO_HOME missing in run preview: {}",
            run_preview
        );
        // Rust v7: image sets PATH; do not override at runtime
        common::assert_preview_no_path_export(&run_preview);

        // Exec preview: expect -u and PATH/CARGO_HOME envs
        let exec_args = aifo_coder::build_sidecar_exec_preview(
            "tc-rust-user",
            Some((uid, gid)),
            &pwd,
            "rust",
            &["cargo".to_string(), "--version".to_string()],
        );
        let exec_preview = aifo_coder::shell_join(&exec_args);
        assert!(
            exec_preview.contains(&format!(" -u {uid}:{gid} ")),
            "expected -u {uid}:{gid} in exec preview: {}",
            exec_preview
        );
        assert!(
            exec_preview.contains("-e CARGO_HOME=/home/coder/.cargo"),
            "CARGO_HOME missing in exec preview: {}",
            exec_preview
        );
        // Rust v7: image sets PATH; do not override at runtime
        common::assert_preview_no_path_export(&exec_preview);
    }
}
