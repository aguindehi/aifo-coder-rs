use aifo_coder::telemetry_init;

/// Telemetry with metrics flag on must not panic (dev exporters when debug flag is set).
#[test]
fn unit_otel_metrics_debug_exporter_no_panic() {
    // Enable metrics and dev-exporter; do not enable fmt to keep stderr quiet.
    std::env::set_var("AIFO_CODER_OTEL_METRICS", "1");
    std::env::set_var("AIFO_CODER_OTEL_DEBUG_OTLP", "1");
    let _g = telemetry_init();
    // Best-effort cleanup of env toggles
    std::env::remove_var("AIFO_CODER_OTEL_METRICS");
    std::env::remove_var("AIFO_CODER_OTEL_DEBUG_OTLP");
}

/// Telemetry with metrics enabled (no dev exporter) must also not panic.
#[test]
fn unit_otel_metrics_enabled_no_panic() {
    std::env::set_var("AIFO_CODER_OTEL_METRICS", "1");
    let _g = telemetry_init();
    std::env::remove_var("AIFO_CODER_OTEL_METRICS");
}
