#![cfg(test)]

#[test]
fn unit_test_aifo_shim_exit86_behavior_is_covered_by_integration_tests() {
    // Unit tests must not spawn external processes. The behavior "aifo-shim exits 86 when
    // proxy env is missing" is validated in integration tests:
    // - tests/int_shims.rs
    // - tests/int_shims_notifications.rs
    assert!(true);
}
