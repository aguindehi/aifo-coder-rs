use once_cell::sync::Lazy;
use std::sync::Mutex;

static GUARD: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[test]
fn unit_test_registry_probe_override_modes() {
    let _g = GUARD.lock().unwrap();
    // Clear env to avoid interference
    std::env::remove_var("AIFO_CODER_REGISTRY_PREFIX");
    std::env::remove_var("AIFO_CODER_TEST_REGISTRY_PROBE");

    // curl-ok
    aifo_coder::registry_probe_set_override_for_tests(Some(
        aifo_coder::RegistryProbeTestMode::CurlOk,
    ));
    let rp = aifo_coder::preferred_mirror_registry_prefix_quiet();
    let src = aifo_coder::preferred_mirror_registry_source();
    assert_eq!(
        rp, "repository.migros.net/",
        "override curl-ok should force internal registry"
    );
    assert_eq!(
        src, "unknown",
        "override should not populate source (unknown)"
    );
    // tcp-fail
    aifo_coder::registry_probe_set_override_for_tests(Some(
        aifo_coder::RegistryProbeTestMode::TcpFail,
    ));
    let rp2 = aifo_coder::preferred_registry_prefix_quiet();
    assert_eq!(rp2, "", "override tcp-fail should force Docker Hub (empty)");

    // Clear override
    aifo_coder::registry_probe_set_override_for_tests(None);
}
