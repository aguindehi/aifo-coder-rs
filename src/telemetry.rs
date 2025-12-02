#![allow(dead_code)]

use std::env;
use std::time::Duration;
use std::time::SystemTime;

use once_cell::sync::OnceCell;
use opentelemetry::global;
use opentelemetry::KeyValue;
use opentelemetry_sdk::export::metrics::{
    AggregationSelector, ExportKindFor, ExportResult as MetricsExportResult, MetricsExporter,
};
use opentelemetry_sdk::export::trace::{ExportResult as TraceExportResult, SpanData, SpanExporter};
use opentelemetry_sdk::metrics as sdkmetrics;
use opentelemetry_sdk::metrics::data::ScopeMetrics;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use opentelemetry_sdk::resource::Resource;
use opentelemetry_sdk::trace as sdktrace;
use tracing_subscriber::prelude::*;

pub struct TelemetryGuard {
    meter_provider: Option<SdkMeterProvider>,
    #[cfg(feature = "otel-otlp")]
    runtime: Option<tokio::runtime::Runtime>,
}

static INIT: OnceCell<()> = OnceCell::new();
static INSTANCE_ID: OnceCell<String> = OnceCell::new();

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

    let instance_id = INSTANCE_ID.get_or_init(|| {
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        format!("{pid}-{nanos}")
    });
    attrs.push(KeyValue::new("service.instance.id", instance_id.clone()));

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

#[cfg(feature = "otel-otlp")]
fn build_tracer(
    resource: &Resource,
    use_otlp: bool,
) -> (sdktrace::TracerProvider, Option<tokio::runtime::Runtime>) {
    if use_otlp {
        use opentelemetry_otlp::WithExportConfig;

        let endpoint = env::var("OTEL_EXPORTER_OTLP_ENDPOINT").unwrap_or_default();
        let endpoint = endpoint.trim();
        if endpoint.is_empty() {
            let provider = build_stderr_tracer(resource);
            return (provider, None);
        }

        let timeout = env::var("OTEL_EXPORTER_OTLP_TIMEOUT")
            .ok()
            .and_then(|s| humantime::parse_duration(&s).ok())
            .unwrap_or_else(|| Duration::from_secs(5));

        let rt_result = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("aifo-otel-worker")
            .build();

        let rt = match rt_result {
            Ok(rt) => rt,
            Err(e) => {
                eprintln!(
                    "aifo-coder: telemetry: failed to create OTLP runtime: {e}; falling back to stderr exporter"
                );
                let provider = build_stderr_tracer(resource);
                return (provider, None);
            }
        };

        let provider_result = rt.block_on(async move {
            let exporter = opentelemetry_otlp::new_exporter()
                .tonic()
                .with_endpoint(endpoint)
                .with_timeout(timeout);

            let mut builder = opentelemetry_otlp::new_pipeline()
                .tracing()
                .with_exporter(exporter);

            builder = builder
                .with_trace_config(sdktrace::Config::default().with_resource(resource.clone()));

            builder.install_batch(opentelemetry_sdk::runtime::Tokio)
        });

        match provider_result {
            Ok(provider) => (provider, Some(rt)),
            Err(e) => {
                eprintln!(
                    "aifo-coder: telemetry: failed to install OTLP tracer: {e}; falling back to stderr exporter"
                );
                let provider = build_stderr_tracer(resource);
                (provider, None)
            }
        }
    } else {
        let provider = build_stderr_tracer(resource);
        (provider, None)
    }
}

#[cfg(not(feature = "otel-otlp"))]
fn build_tracer(resource: &Resource, _use_otlp: bool) -> sdktrace::TracerProvider {
    build_stderr_tracer(resource)
}

fn build_stderr_tracer(resource: &Resource) -> sdktrace::TracerProvider {
    #[derive(Debug)]
    struct StderrSpanExporter;

    impl SpanExporter for StderrSpanExporter {
        fn export(
            &mut self,
            batch: Vec<SpanData>,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = TraceExportResult> + Send + 'static>>
        {
            Box::pin(async move {
                let mut stderr = std::io::stderr();
                for span in batch {
                    let name = span.name;
                    let span_id = span.span_context.span_id().to_string();
                    let trace_id = span.span_context.trace_id().to_string();
                    let _ = writeln!(
                        stderr,
                        "otel-span name={name} trace_id={trace_id} span_id={span_id}"
                    );
                }
                TraceExportResult::Success
            })
        }

        fn shutdown(&mut self) {
            // nothing to do for stderr exporter
        }
    }

    let exporter = StderrSpanExporter;
    sdktrace::TracerProvider::builder()
        .with_simple_exporter(exporter)
        .with_config(sdktrace::Config::default().with_resource(resource.clone()))
        .build()
}

fn build_metrics_provider(resource: &Resource, use_otlp: bool) -> Option<SdkMeterProvider> {
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

            let exporter = opentelemetry_otlp::new_exporter()
                .tonic()
                .with_endpoint(endpoint);
            let reader =
                sdkmetrics::PeriodicReader::builder(exporter, Duration::from_secs(2)).build();

            let provider = SdkMeterProvider::builder()
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
        struct StderrMetricsExporter;

        impl MetricsExporter for StderrMetricsExporter {
            fn export(
                &self,
                _resource: &opentelemetry_sdk::Resource,
                _scope_metrics: &[ScopeMetrics<'_>],
            ) -> MetricsExportResult {
                // For Phase 1, keep metrics off stdout; real dev sink comes in Phase 4.
                MetricsExportResult::Success
            }

            fn force_flush(&self, _timeout: Option<std::time::Duration>) -> MetricsExportResult {
                MetricsExportResult::Success
            }

            fn shutdown(&self) -> MetricsExportResult {
                MetricsExportResult::Success
            }

            fn export_kind(&self) -> ExportKindFor {
                ExportKindFor::Cumulative
            }
        }

        let exporter = StderrMetricsExporter;
        let reader = sdkmetrics::PeriodicReader::builder(exporter, Duration::from_secs(2)).build();

        let provider = SdkMeterProvider::builder()
            .with_resource(resource.clone())
            .with_reader(reader)
            .with_aggregation_selector(AggregationSelector::new())
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

    #[cfg(feature = "otel-otlp")]
    let (tracer_provider, runtime) = build_tracer(&resource, use_otlp);

    #[cfg(not(feature = "otel-otlp"))]
    let tracer_provider = build_tracer(&resource, use_otlp);
    let tracer = tracer_provider.tracer("aifo-coder");

    let meter_provider = build_metrics_provider(&resource, use_otlp);
    if let Some(ref mp) = meter_provider {
        global::set_meter_provider(mp.clone());
    }

    global::set_tracer_provider(tracer_provider);
    let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

    let mut registry = tracing_subscriber::registry().with(otel_layer);

    if env::var("AIFO_CODER_TRACING_FMT").ok().as_deref() == Some("1") {
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

    #[cfg(feature = "otel-otlp")]
    {
        Some(TelemetryGuard {
            meter_provider,
            runtime,
        })
    }

    #[cfg(not(feature = "otel-otlp"))]
    {
        Some(TelemetryGuard { meter_provider })
    }
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
