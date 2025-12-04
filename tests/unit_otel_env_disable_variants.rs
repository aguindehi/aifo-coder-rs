use aifo_coder::telemetry_init;

/// Ensure that disabling via common falsy values yields None (idempotent/no-op).
#[test]
fn otel_disabled_env_false() {
    std::env::set_var("AIFO_CODER_OTEL", "false");
    let first = telemetry_init();
    let second = telemetry_init();
    assert!(first.is_none(), "expected None when AIFO_CODER_OTEL=false");
    assert!(
        second.is_none(),
        "expected None (idempotent) when AIFO_CODER_OTEL=false"
    );
    std::env::remove_var("AIFO_CODER_OTEL");
}

#[test]
fn otel_disabled_env_no() {
    std::env::set_var("AIFO_CODER_OTEL", "no");
    let first = telemetry_init();
    let second = telemetry_init();
    assert!(first.is_none(), "expected None when AIFO_CODER_OTEL=no");
    assert!(
        second.is_none(),
        "expected None (idempotent) when AIFO_CODER_OTEL=no"
    );
    std::env::remove_var("AIFO_CODER_OTEL");
}

#[test]
fn otel_disabled_env_off() {
    std::env::set_var("AIFO_CODER_OTEL", "off");
    let first = telemetry_init();
    let second = telemetry_init();
    assert!(first.is_none(), "expected None when AIFO_CODER_OTEL=off");
    assert!(
        second.is_none(),
        "expected None (idempotent) when AIFO_CODER_OTEL=off"
    );
    std::env::remove_var("AIFO_CODER_OTEL");
}
