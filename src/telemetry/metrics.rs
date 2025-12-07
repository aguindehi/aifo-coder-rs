use once_cell::sync::OnceCell;
use opentelemetry::metrics::{Counter, Histogram, Meter};
use opentelemetry::KeyValue;

// Instrument accessors (lazily created via global Meter)
static RUNS_TOTAL: OnceCell<Counter<u64>> = OnceCell::new();
static DOCKER_INVOCATIONS_TOTAL: OnceCell<Counter<u64>> = OnceCell::new();
static PROXY_REQUESTS_TOTAL: OnceCell<Counter<u64>> = OnceCell::new();
static SIDECARS_STARTED_TOTAL: OnceCell<Counter<u64>> = OnceCell::new();
static SIDECARS_STOPPED_TOTAL: OnceCell<Counter<u64>> = OnceCell::new();
static RUN_DURATION: OnceCell<Histogram<f64>> = OnceCell::new();
static DOCKER_RUN_DURATION: OnceCell<Histogram<f64>> = OnceCell::new();
static PROXY_EXEC_DURATION: OnceCell<Histogram<f64>> = OnceCell::new();
static REGISTRY_PROBE_DURATION: OnceCell<Histogram<f64>> = OnceCell::new();

fn meter() -> Meter {
    opentelemetry::global::meter("aifo_coder")
}

fn runs_total() -> Counter<u64> {
    RUNS_TOTAL
        .get_or_init(|| {
            meter()
                .u64_counter("aifo_coder_runs_total")
                .with_description("Total aifo-coder CLI runs")
                .with_unit("1")
                .build()
        })
        .clone()
}

fn docker_invocations_total() -> Counter<u64> {
    DOCKER_INVOCATIONS_TOTAL
        .get_or_init(|| {
            meter()
                .u64_counter("aifo_coder_docker_invocations_total")
                .with_description("Total Docker CLI invocations by kind")
                .with_unit("1")
                .build()
        })
        .clone()
}

fn proxy_requests_total() -> Counter<u64> {
    PROXY_REQUESTS_TOTAL
        .get_or_init(|| {
            meter()
                .u64_counter("aifo_coder_proxy_requests_total")
                .with_description("Total proxy tool requests by result")
                .with_unit("1")
                .build()
        })
        .clone()
}

fn sidecars_started_total() -> Counter<u64> {
    SIDECARS_STARTED_TOTAL
        .get_or_init(|| {
            meter()
                .u64_counter("aifo_coder_toolchain_sidecars_started_total")
                .with_description("Total toolchain sidecars started by kind")
                .with_unit("1")
                .build()
        })
        .clone()
}

fn sidecars_stopped_total() -> Counter<u64> {
    SIDECARS_STOPPED_TOTAL
        .get_or_init(|| {
            meter()
                .u64_counter("aifo_coder_toolchain_sidecars_stopped_total")
                .with_description("Total toolchain sidecars stopped by kind")
                .with_unit("1")
                .build()
        })
        .clone()
}

fn run_duration_hist() -> Histogram<f64> {
    RUN_DURATION
        .get_or_init(|| {
            meter()
                .f64_histogram("aifo_coder_run_duration_seconds")
                .with_description("Total duration of aifo-coder CLI runs")
                .with_unit("s")
                .build()
        })
        .clone()
}

fn docker_run_duration_hist() -> Histogram<f64> {
    DOCKER_RUN_DURATION
        .get_or_init(|| {
            meter()
                .f64_histogram("aifo_coder_docker_run_duration_seconds")
                .with_description("Duration of docker run invocations by agent (s)")
                .with_unit("s")
                .build()
        })
        .clone()
}

fn proxy_exec_duration_hist() -> Histogram<f64> {
    PROXY_EXEC_DURATION
        .get_or_init(|| {
            meter()
                .f64_histogram("aifo_coder_proxy_exec_duration_seconds")
                .with_description("Duration of proxy exec per tool (s)")
                .with_unit("s")
                .build()
        })
        .clone()
}

fn registry_probe_duration_hist() -> Histogram<f64> {
    REGISTRY_PROBE_DURATION
        .get_or_init(|| {
            meter()
                .f64_histogram("aifo_coder_registry_probe_duration_seconds")
                .with_description("Duration of registry probe by source (s)")
                .with_unit("s")
                .build()
        })
        .clone()
}

// Public helpers used from other modules (all no-ops when metrics provider not installed)

pub fn record_run(agent: &str) {
    let c = runs_total();
    c.add(1, &[KeyValue::new("aifo_coder_agent", agent.to_string())]);
}

pub fn record_docker_invocation(kind: &str) {
    let c = docker_invocations_total();
    c.add(1, &[KeyValue::new("aifo_coder_kind", kind.to_string())]);
}

pub fn record_proxy_request(tool: &str, result: &str) {
    let c = proxy_requests_total();
    c.add(
        1,
        &[
            KeyValue::new("aifo_coder_tool", tool.to_string()),
            KeyValue::new("aifo_coder_result", result.to_string()),
        ],
    );
}

pub fn record_sidecar_started(kind: &str) {
    let c = sidecars_started_total();
    c.add(1, &[KeyValue::new("aifo_coder_kind", kind.to_string())]);
}

pub fn record_sidecar_stopped(kind: &str) {
    let c = sidecars_stopped_total();
    c.add(1, &[KeyValue::new("aifo_coder_kind", kind.to_string())]);
}

pub fn record_docker_run_duration(agent: &str, secs: f64) {
    let h = docker_run_duration_hist();
    h.record(
        secs,
        &[KeyValue::new("aifo_coder_agent", agent.to_string())],
    );
}

pub fn record_run_duration(agent: &str, secs: f64) {
    let h = run_duration_hist();
    h.record(
        secs,
        &[KeyValue::new("aifo_coder_agent", agent.to_string())],
    );
}

pub fn record_proxy_exec_duration(tool: &str, secs: f64) {
    let h = proxy_exec_duration_hist();
    h.record(secs, &[KeyValue::new("aifo_coder_tool", tool.to_string())]);
}

pub fn record_registry_probe_duration(source: &str, secs: f64) {
    let h = registry_probe_duration_hist();
    h.record(
        secs,
        &[KeyValue::new("aifo_coder_source", source.to_string())],
    );
}
