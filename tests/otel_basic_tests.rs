use aifo_coder::telemetry_init;

/// Idempotence: the second call must always return None (already initialized or disabled).
#[test]
fn otel_idempotent_second_none() {
    // Ensure default behavior (unset); do not enforce enabled to keep test robust
    std::env::remove_var("AIFO_CODER_OTEL");
    let _first = telemetry_init();
    let second = telemetry_init();
    assert!(
        second.is_none(),
        "telemetry_init second call must be None (idempotent)"
    );
}

/// Disable via env must be a no-op and return None for all calls (also robust if already initialized).
#[test]
fn otel_disabled_env_returns_none_both_calls() {
    std::env::set_var("AIFO_CODER_OTEL", "0");
    let first = telemetry_init();
    let second = telemetry_init();
    assert!(
        first.is_none(),
        "when AIFO_CODER_OTEL=0, telemetry_init should return None (disabled)"
    );
    assert!(
        second.is_none(),
        "when AIFO_CODER_OTEL=0, telemetry_init should return None (idempotent/disabled)"
    );
    std::env::remove_var("AIFO_CODER_OTEL");
}

/// Enabling the fmt layer must not panic and must not affect stdout (fmt writes to stderr when enabled).
#[test]
fn otel_fmt_layer_no_panic() {
    std::env::set_var("AIFO_CODER_TRACING_FMT", "1");
    std::env::set_var("RUST_LOG", "warn");
    let _guard = telemetry_init();
    // Cleanup best-effort; the global subscriber may already be set
    std::env::remove_var("AIFO_CODER_TRACING_FMT");
    std::env::remove_var("RUST_LOG");
}
