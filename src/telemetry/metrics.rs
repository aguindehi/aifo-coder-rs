use std::path::PathBuf;

use once_cell::sync::OnceCell;
use opentelemetry::metrics::{Counter, Histogram, Meter};
use opentelemetry::KeyValue;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::resource::Resource;

/// Compute the dev metrics file path:
/// - ${AIFO_CODER_OTEL_METRICS_FILE} if set and non-empty
/// - else ${XDG_RUNTIME_DIR:-/tmp}/aifo-coder.otel.metrics.jsonl
pub fn dev_metrics_path() -> PathBuf {
    if let Ok(p) = std::env::var("AIFO_CODER_OTEL_METRICS_FILE") {
        let t = p.trim();
        if !t.is_empty() {
            return PathBuf::from(t);
        }
    }
    let base = std::env::var("XDG_RUNTIME_DIR")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    base.join("aifo-coder.otel.metrics.jsonl")
}

/// Build a simple development metrics provider using a stdout exporter configured
/// by the opentelemetry-stdout crate. The exporter itself must be configured by
/// environment to avoid writing to stdout in production; when in doubt, metrics
/// can be disabled via AIFO_CODER_OTEL_METRICS=0.
pub fn build_file_metrics_provider(resource: Resource, _path: PathBuf) -> SdkMeterProvider {
    use opentelemetry_stdout::MetricsExporterBuilder;

    // Best-effort: build a stdout metrics exporter. The crate handles the actual
    // writer target; our main constraint is to never touch stdout in default runs,
    // so this dev exporter is intended for explicitly opt-in scenarios only.
    let exporter = MetricsExporterBuilder::default().build();

    let reader = opentelemetry_sdk::metrics::PeriodicReader::builder(
        exporter,
        opentelemetry_sdk::runtime::Tokio,
    )
    .build();

    opentelemetry_sdk::metrics::SdkMeterProvider::builder()
        .with_resource(resource)
        .with_reader(reader)
        .build()
}

// Instrument accessors (lazily created via global Meter)
static RUNS_TOTAL: OnceCell<Counter<u64>> = OnceCell::new();
static DOCKER_INVOCATIONS_TOTAL: OnceCell<Counter<u64>> = OnceCell::new();
static PROXY_REQUESTS_TOTAL: OnceCell<Counter<u64>> = OnceCell::new();
static SIDEcars_STARTED_TOTAL: OnceCell<Counter<u64>> = OnceCell::new();
static SIDEcars_STOPPED_TOTAL: OnceCell<Counter<u64>> = OnceCell::new();
static DOCKER_RUN_DURATION: OnceCell<Histogram<f64>> = OnceCell::new();
static PROXY_EXEC_DURATION: OnceCell<Histogram<f64>> = OnceCell::new();
static REGISTRY_PROBE_DURATION: OnceCell<Histogram<f64>> = OnceCell::new();

fn meter() -> Meter {
    opentelemetry::global::meter("aifo-coder")
}

fn runs_total() -> Counter<u64> {
    RUNS_TOTAL
        .get_or_init(|| {
            meter()
                .u64_counter("aifo_runs_total")
                .with_description("Total aifo-coder CLI runs")
                .init()
        })
        .clone()
}

fn docker_invocations_total() -> Counter<u64> {
    DOCKER_INVOCATIONS_TOTAL
        .get_or_init(|| {
            meter()
                .u64_counter("docker_invocations_total")
                .with_description("Total Docker CLI invocations by kind")
                .init()
        })
        .clone()
}

fn proxy_requests_total() -> Counter<u64> {
    PROXY_REQUESTS_TOTAL
        .get_or_init(|| {
            meter()
                .u64_counter("proxy_requests_total")
                .with_description("Total proxy tool requests by result")
                .init()
        })
        .clone()
}

fn sidecars_started_total() -> Counter<u64> {
    SIDEcars_STARTED_TOTAL
        .get_or_init(|| {
            meter()
                .u64_counter("toolchain_sidecars_started_total")
                .with_description("Total toolchain sidecars started by kind")
                .init()
        })
        .clone()
}

fn sidecars_stopped_total() -> Counter<u64> {
    SIDEcars_STOPPED_TOTAL
        .get_or_init(|| {
            meter()
                .u64_counter("toolchain_sidecars_stopped_total")
                .with_description("Total toolchain sidecars stopped by kind")
                .init()
        })
        .clone()
}

fn docker_run_duration_hist() -> Histogram<f64> {
    DOCKER_RUN_DURATION
        .get_or_init(|| {
            meter()
                .f64_histogram("docker_run_duration")
                .with_description("Duration of docker run invocations by agent (s)")
                .init()
        })
        .clone()
}

fn proxy_exec_duration_hist() -> Histogram<f64> {
    PROXY_EXEC_DURATION
        .get_or_init(|| {
            meter()
                .f64_histogram("proxy_exec_duration")
                .with_description("Duration of proxy exec per tool (s)")
                .init()
        })
        .clone()
}

fn registry_probe_duration_hist() -> Histogram<f64> {
    REGISTRY_PROBE_DURATION
        .get_or_init(|| {
            meter()
                .f64_histogram("registry_probe_duration")
                .with_description("Duration of registry probe by source (s)")
                .init()
        })
        .clone()
}

// Public helpers used from other modules (all no-ops when metrics provider not installed)

pub fn record_run(agent: &str) {
    let c = runs_total();
    c.add(1, &[KeyValue::new("agent", agent.to_string())]);
}

pub fn record_docker_invocation(kind: &str) {
    let c = docker_invocations_total();
    c.add(1, &[KeyValue::new("kind", kind.to_string())]);
}

pub fn record_proxy_request(tool: &str, result: &str) {
    let c = proxy_requests_total();
    c.add(
        1,
        &[
            KeyValue::new("tool", tool.to_string()),
            KeyValue::new("result", result.to_string()),
        ],
    );
}

pub fn record_sidecar_started(kind: &str) {
    let c = sidecars_started_total();
    c.add(1, &[KeyValue::new("kind", kind.to_string())]);
}

pub fn record_sidecar_stopped(kind: &str) {
    let c = sidecars_stopped_total();
    c.add(1, &[KeyValue::new("kind", kind.to_string())]);
}

pub fn record_docker_run_duration(agent: &str, secs: f64) {
    let h = docker_run_duration_hist();
    h.record(secs, &[KeyValue::new("agent", agent.to_string())]);
}

pub fn record_proxy_exec_duration(tool: &str, secs: f64) {
    let h = proxy_exec_duration_hist();
    h.record(secs, &[KeyValue::new("tool", tool.to_string())]);
}

pub fn record_registry_probe_duration(source: &str, secs: f64) {
    let h = registry_probe_duration_hist();
    h.record(secs, &[KeyValue::new("source", source.to_string())]);
}
