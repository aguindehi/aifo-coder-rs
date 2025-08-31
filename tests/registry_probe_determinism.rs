use once_cell::sync::Lazy;
use std::sync::Mutex;

static REG_GUARD: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

fn clear_env_overrides() {
    std::env::remove_var("AIFO_CODER_REGISTRY_PREFIX");
    std::env::remove_var("AIFO_CODER_TEST_REGISTRY_PROBE");
}

#[test]
fn test_registry_probe_curl_success_forced() {
    let _g = REG_GUARD.lock().unwrap();
    clear_env_overrides();
    std::env::set_var("AIFO_CODER_TEST_REGISTRY_PROBE", "curl-ok");
    let rp = aifo_coder::preferred_registry_prefix_quiet();
    let src = aifo_coder::preferred_registry_source();
    assert_eq!(rp, "repository.migros.net/", "expected curl-ok to force internal registry prefix");
    assert_eq!(src, "curl", "expected source=curl");
    std::env::remove_var("AIFO_CODER_TEST_REGISTRY_PROBE");
}

#[test]
fn test_registry_probe_curl_failure_forced() {
    let _g = REG_GUARD.lock().unwrap();
    clear_env_overrides();
    std::env::set_var("AIFO_CODER_TEST_REGISTRY_PROBE", "curl-fail");
    let rp = aifo_coder::preferred_registry_prefix_quiet();
    let src = aifo_coder::preferred_registry_source();
    assert_eq!(rp, "", "expected curl-fail to force Docker Hub (no prefix)");
    assert_eq!(src, "curl", "expected source=curl");
    std::env::remove_var("AIFO_CODER_TEST_REGISTRY_PROBE");
}

#[test]
fn test_registry_probe_tcp_success_forced() {
    let _g = REG_GUARD.lock().unwrap();
    clear_env_overrides();
    std::env::set_var("AIFO_CODER_TEST_REGISTRY_PROBE", "tcp-ok");
    let rp = aifo_coder::preferred_registry_prefix_quiet();
    let src = aifo_coder::preferred_registry_source();
    assert_eq!(rp, "repository.migros.net/", "expected tcp-ok to force internal registry prefix");
    assert_eq!(src, "tcp", "expected source=tcp");
    std::env::remove_var("AIFO_CODER_TEST_REGISTRY_PROBE");
}

#[test]
fn test_registry_probe_tcp_failure_forced() {
    let _g = REG_GUARD.lock().unwrap();
    clear_env_overrides();
    std::env::set_var("AIFO_CODER_TEST_REGISTRY_PROBE", "tcp-fail");
    let rp = aifo_coder::preferred_registry_prefix_quiet();
    let src = aifo_coder::preferred_registry_source();
    assert_eq!(rp, "", "expected tcp-fail to force Docker Hub (no prefix)");
    assert_eq!(src, "tcp", "expected source=tcp");
    std::env::remove_var("AIFO_CODER_TEST_REGISTRY_PROBE");
}
