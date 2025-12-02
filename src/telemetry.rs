#![allow(dead_code)]

use std::env;
use std::time::Duration;

use once_cell::sync::OnceCell;
use opentelemetry::global;
use opentelemetry::propagation::TraceContextPropagator;
use opentelemetry::sdk::resource::Resource;
use opentelemetry::sdk::{metrics as sdkmetrics, trace as sdktrace};
use opentelemetry::KeyValue;
use tracing_subscriber::prelude::*;

pub struct TelemetryGuard {
    tracer_provider: Option<sdktrace::TracerProvider>,
    meter_provider: Option<sdkmetrics::MeterProvider>,
    #[cfg(feature = "otel-otlp")]
    runtime: Option<tokio::runtime::Runtime>,
}

static INIT: OnceCell<()> = OnceCell::new();

fn telemetry_enabled_env() -> bool {
    let aifo = env::var("AIFO_CODER_OTEL").ok().as_deref() == Some("1");
    let endpoint = env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .is_some();
    aifo || endpoint
}

fn build_resource() -> Resource {
    let service_name = env::var("OTEL_SERVICE_NAME")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "aifo-coder".to_string());

    let mut attrs = Vec::new();
    attrs.push(KeyValue::new("service.name", service_name));
    attrs.push(KeyValue::new("service.version", env!("CARGO_PKG_VERSION")));
    attrs.push(KeyValue::new("service.namespace", "aifo"));

    let pid = std::process::id() as i64;
    attrs.push(KeyValue::new("process.pid", pid));

    if let Ok(host) = hostname::get() {
        if let Ok(s) = host.into_string() {
            attrs.push(KeyValue::new("host.name", s));
        }
    }

    if let Ok(os_type) = env::var("OSTYPE") {
        if !os_type.is_empty() {
            attrs.push(KeyValue::new("os.type", os_type));
        }
    }

    if let Ok(exec) = env::current_exe() {
        if let Some(name) = exec.file_name().and_then(|s| s.to_str()) {
            attrs.push(KeyValue::new("process.executable.name", name.to_string()));
        }
    }

    if let Ok(env_name) = env::var("DEPLOYMENT_ENVIRONMENT") {
        if !env_name.is_empty() {
            attrs.push(KeyValue::new("deployment.environment", env_name));
        }
    }

    Resource::new(attrs)
}

fn build_tracer(
    resource: &Resource,
    use_otlp: bool,
) -> (
    sdktrace::TracerProvider,
    #[cfg(feature = "otel-otlp")] Option<tokio::runtime::Runtime>,
) {
    if use_otlp {
        #[cfg(feature = "otel-otlp")]
        {
            use opentelemetry_otlp::WithExportConfig;

            let endpoint = env::var("OTEL_EXPORTER_OTLP_ENDPOINT").unwrap_or_default();
            let endpoint = endpoint.trim();
            if endpoint.is_empty() {
                return build_stdout_tracer(resource);
            }

            let timeout = env::var("OTEL_EXPORTER_OTLP_TIMEOUT")
                .ok()
                .and_then(|s| humantime::parse_duration(&s).ok())
                .unwrap_or_else(|| Duration::from_secs(5));

            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .thread_name("aifo-otel-worker")
                .build()
                .ok();

            if let Some(rt) = rt {
                let provider = rt.block_on(async move {
                    let exporter = opentelemetry_otlp::new_exporter()
                        .tonic()
                        .with_endpoint(endpoint)
                        .with_timeout(timeout);

                    let mut builder =
                        opentelemetry_otlp::new_pipeline().tracing().with_exporter(exporter);

                    builder = builder.with_trace_config(
                        sdktrace::Config::default().with_resource(resource.clone()),
                    );

                    builder
                        .install_batch(opentelemetry_sdk::runtime::Tokio)
                        .expect("failed to install OTLP tracer")
                });

                return (provider, Some(rt));
            }

            build_stdout_tracer(resource)
        }

        #[cfg(not(feature = "otel-otlp"))]
        {
            eprintln!("aifo-coder: telemetry: OTLP endpoint set but otel-otlp feature not enabled; falling back to stdout exporter");
            build_stdout_tracer(resource)
        }
    } else {
        build_stdout_tracer(resource)
    }
}

fn build_stdout_tracer(
    resource: &Resource,
) -> (
    sdktrace::TracerProvider,
    #[cfg(feature = "otel-otlp")] Option<tokio::runtime::Runtime>,
) {
    use opentelemetry_stdout::SpanExporter;

    let exporter = SpanExporter::default();
    let provider = sdktrace::TracerProvider::builder()
        .with_simple_exporter(exporter)
        .with_config(sdktrace::Config::default().with_resource(resource.clone()))
        .build();

    #[cfg(feature = "otel-otlp")]
    {
        (provider, None)
    }
    #[cfg(not(feature = "otel-otlp"))]
    {
        (provider, )
    }
}

fn build_metrics_provider(
    resource: &Resource,
    use_otlp: bool,
) -> Option<sdkmetrics::MeterProvider> {
    if env::var("AIFO_CODER_OTEL_METRICS").ok().as_deref() != Some("1") {
        return None;
    }

    if use_otlp {
        #[cfg(feature = "otel-otlp")]
        {
            use opentelemetry_otlp::WithExportConfig;

            let endpoint = env::var("OTEL_EXPORTER_OTLP_ENDPOINT").unwrap_or_default();
            let endpoint = endpoint.trim();
            if endpoint.is_empty() {
                return None;
            }

            let exporter = opentelemetry_otlp::new_exporter().tonic().with_endpoint(endpoint);
            let reader = sdkmetrics::PeriodicReader::builder(exporter, Duration::from_secs(2))
                .build();

            let provider = sdkmetrics::MeterProvider::builder()
                .with_resource(resource.clone())
                .with_reader(reader)
                .build();

            Some(provider)
        }

        #[cfg(not(feature = "otel-otlp"))]
        {
            None
        }
    } else {
        use opentelemetry_stdout::MetricsExporter;

        let exporter = MetricsExporter::default();
        let reader = sdkmetrics::PeriodicReader::builder(exporter, Duration::from_secs(2)).build();

        let provider = sdkmetrics::MeterProvider::builder()
            .with_resource(resource.clone())
            .with_reader(reader)
            .build();

        Some(provider)
    }
}

pub fn telemetry_init() -> Option<TelemetryGuard> {
    if INIT.get().is_some() {
        return None;
    }

    if !telemetry_enabled_env() {
        return None;
    }

    global::set_text_map_propagator(TraceContextPropagator::new());

    let resource = build_resource();

    let use_otlp = cfg!(feature = "otel-otlp")
        && env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
            .ok()
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false);

    let (tracer_provider, runtime) = build_tracer(&resource, use_otlp);
    let tracer = tracer_provider.tracer("aifo-coder");

    let meter_provider = build_metrics_provider(&resource, use_otlp);
    if let Some(ref mp) = meter_provider {
        global::set_meter_provider(mp.clone());
    }

    global::set_tracer_provider(tracer_provider.clone());
    let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

    let mut registry = tracing_subscriber::registry().with(otel_layer);

    if env::var("AIFO_CODER_TRACING_FMT")
        .ok()
        .as_deref()
        == Some("1")
    {
        let filter = env::var("RUST_LOG").unwrap_or_else(|_| "warn".to_string());
        let env_filter = tracing_subscriber::EnvFilter::new(filter);
        let fmt_layer = tracing_subscriber::fmt::layer();
        registry = registry.with(env_filter).with(fmt_layer);
    }

    if registry.try_init().is_err() {
        eprintln!("aifo-coder: telemetry init skipped (global subscriber already set)");
        return None;
    }

    let _ = INIT.set(());

    Some(TelemetryGuard {
        tracer_provider: Some(tracer_provider),
        meter_provider,
        #[cfg(feature = "otel-otlp")]
        runtime,
    })
}

impl Drop for TelemetryGuard {
    fn drop(&mut self) {
        if let Some(ref mp) = self.meter_provider {
            let _ = mp.force_flush();
        }
        global::shutdown_tracer_provider();
        #[cfg(feature = "otel-otlp")]
        {
            if let Some(rt) = self.runtime.take() {
                drop(rt);
            }
        }
    }
}
