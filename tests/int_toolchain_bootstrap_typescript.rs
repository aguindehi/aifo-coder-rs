#[test]
fn int_toolchain_bootstrap_typescript_global_best_effort() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // The function is best-effort and should not error even if the sidecar is missing.
    let sid = "unit-test-session";
    let res = aifo_coder::toolchain_bootstrap_typescript_global(sid, true);
    assert!(
        res.is_ok(),
        "bootstrap should not error even if sidecar is missing: {:?}",
        res
    );
}
