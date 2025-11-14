use once_cell::sync::Lazy;
use std::sync::Mutex;

static GUARD: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[test]
fn unit_test_registry_probe_without_curl_does_not_panic() {
    let _g = GUARD.lock().unwrap();
    // Clear overrides and force PATH to empty so curl is not found
    std::env::remove_var("AIFO_CODER_REGISTRY_PREFIX");
    std::env::remove_var("AIFO_CODER_TEST_REGISTRY_PROBE");
    let old_path = std::env::var("PATH").ok();
    std::env::set_var("PATH", "");
    let rp = aifo_coder::preferred_mirror_registry_prefix_quiet();
    assert!(
        rp.is_empty() || rp == "repository.migros.net/",
        "expected either empty or internal registry, got: {}",
        rp
    );
    // restore PATH
    if let Some(v) = old_path {
        std::env::set_var("PATH", v);
    } else {
        std::env::remove_var("PATH");
    }
}

#[test]
fn unit_test_registry_probe_unknown_mode_reports_unknown_source() {
    let _g = GUARD.lock().unwrap();
    std::env::remove_var("AIFO_CODER_REGISTRY_PREFIX");
    std::env::set_var("AIFO_CODER_TEST_REGISTRY_PROBE", "unknown");
    let _ = aifo_coder::preferred_registry_prefix_quiet();
    let src = aifo_coder::preferred_registry_source();
    assert_eq!(
        src, "unknown",
        "expected source=unknown for unknown probe mode"
    );
    std::env::remove_var("AIFO_CODER_TEST_REGISTRY_PROBE");
}
