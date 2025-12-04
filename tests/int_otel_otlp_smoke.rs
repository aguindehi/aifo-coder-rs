use aifo_coder::telemetry_init;

/// Integration smoke: init telemetry with an OTLP HTTP endpoint must not panic.
/// Runs under feature otel-otlp; does not assert export success, only init/no-panic.
#[test]
fn int_otel_otlp_init_no_panic() {
    // Provide a benign local endpoint; init should be best-effort.
    std::env::set_var("OTEL_EXPORTER_OTLP_ENDPOINT", "https://localhost:4318");
    // Keep fmt disabled to avoid stderr chatter in CI.
    std::env::set_var("AIFO_CODER_TRACING_FMT", "0");
    // Metrics enabled is fine; provider installs a periodic reader when possible.
    std::env::set_var("AIFO_CODER_OTEL_METRICS", "1");

    let _guard = telemetry_init();

    // Cleanup env
    std::env::remove_var("OTEL_EXPORTER_OTLP_ENDPOINT");
    std::env::remove_var("AIFO_CODER_TRACING_FMT");
    std::env::remove_var("AIFO_CODER_OTEL_METRICS");
}
