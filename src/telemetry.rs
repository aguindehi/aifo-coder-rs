#![allow(dead_code)]

use std::env;
use std::io::Write;
use std::time::Duration;
use std::time::SystemTime;

use once_cell::sync::OnceCell;
use opentelemetry::global;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry::KeyValue;
use opentelemetry_sdk::export::trace::{ExportResult as TraceExportResult, SpanData, SpanExporter};
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use opentelemetry_sdk::resource::Resource;
use opentelemetry_sdk::trace as sdktrace;
use tracing_subscriber::prelude::*;

#[cfg(feature = "otel")]
pub mod metrics;

pub struct TelemetryGuard {
    meter_provider: Option<SdkMeterProvider>,
    #[cfg(feature = "otel-otlp")]
    runtime: Option<tokio::runtime::Runtime>,
}

static INIT: OnceCell<()> = OnceCell::new();
static INSTANCE_ID: OnceCell<String> = OnceCell::new();
static HASH_SALT: OnceCell<u64> = OnceCell::new();

 // Default OTLP endpoint selection:
 // - First, use AIFO_OTEL_DEFAULT_ENDPOINT baked in at compile time (via build.rs) when present.
 // - Otherwise, fall back to a safe example endpoint for local collectors.
const DEFAULT_OTLP_ENDPOINT: &str =
    option_env!("AIFO_OTEL_DEFAULT_ENDPOINT").unwrap_or("http://localhost:4317");

fn telemetry_enabled_env() -> bool {
    match env::var("AIFO_CODER_OTEL") {
        Ok(v) => {
            let v = v.trim().to_ascii_lowercase();
            matches!(v.as_str(), "1" | "true" | "yes")
        }
        Err(_) => true,
    }
}

fn effective_otlp_endpoint() -> Option<String> {
    if let Ok(v) = env::var("OTEL_EXPORTER_OTLP_ENDPOINT") {
        let t = v.trim();
        if !t.is_empty() {
            return Some(t.to_string());
        }
    }

    // Fallback to baked-in or code default; trim at runtime to avoid const fn limitations.
    let t = DEFAULT_OTLP_ENDPOINT.trim();
    if t.is_empty() {
        return Some("http://localhost:4317".to_string());
    }

    Some(t.to_string())
}

/// Return true if PII-rich telemetry is allowed (unsafe; for debugging only).
pub fn telemetry_pii_enabled() -> bool {
    env::var("AIFO_CODER_OTEL_PII").ok().as_deref() == Some("1")
}

/// Compute or retrieve a per-process FNV-1a salt derived from pid and start time.
fn hash_salt() -> u64 {
    *HASH_SALT.get_or_init(|| {
        let pid = std::process::id() as u64;
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        pid ^ nanos.wrapping_mul(0x9e3779b97f4a7c15)
    })
}

/// Simple 64-bit FNV-1a hash of a string with a per-process salt; returns 16-hex lowercase id.
pub fn hash_string_hex(s: &str) -> String {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 1099511628211;

    let mut h: u64 = FNV_OFFSET ^ hash_salt();
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(FNV_PRIME);
    }
    format!("{:016x}", h)
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

        let endpoint = match effective_otlp_endpoint() {
            Some(ep) => ep,
            None => {
                if env::var("AIFO_CODER_OTEL_VERBOSE").ok().as_deref() == Some("1") {
                    eprintln!(
                        "aifo-coder: telemetry: no OTLP endpoint available; falling back to stderr exporter"
                    );
                }
                let provider = build_stderr_tracer(resource);
                return (provider, None);
            }
        };

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

            // Use default Config which respects OTEL_TRACES_SAMPLER / OTEL_TRACES_SAMPLER_ARG
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
                if env::var("AIFO_CODER_OTEL_VERBOSE").ok().as_deref() == Some("1") {
                    eprintln!(
                        "aifo-coder: telemetry: OTLP export will be disabled; CLI output and exit codes remain unchanged"
                    );
                }
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
fn build_tracer(resource: &Resource, use_otlp: bool) -> sdktrace::TracerProvider {
    if use_otlp {
        eprintln!(
            "aifo-coder: telemetry: OTLP endpoint configured but otel-otlp feature is disabled; falling back to stderr exporter"
        );
    }
    build_stderr_tracer(resource)
}

fn build_stderr_tracer(resource: &Resource) -> sdktrace::TracerProvider {
    // Development-only stderr exporter: emits compact span summaries to stderr.
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
                Ok(())
            })
        }

        fn shutdown(&mut self) {
            // nothing to do for stderr exporter
        }
    }

    let exporter = StderrSpanExporter;
    // Use default Config so standard env vars (e.g., OTEL_TRACES_SAMPLER) are honored.
    sdktrace::TracerProvider::builder()
        .with_simple_exporter(exporter)
        .with_config(sdktrace::Config::default().with_resource(resource.clone()))
        .build()
}

fn build_metrics_provider(resource: &Resource, use_otlp: bool) -> Option<SdkMeterProvider> {
    if env::var("AIFO_CODER_OTEL_METRICS").ok().as_deref() != Some("1") {
        return None;
    }

    #[cfg(feature = "otel-otlp")]
    {
        if use_otlp {
            // OTLP metrics exporter with PeriodicReader (1–2s interval).
            use opentelemetry_otlp::WithExportConfig;

            let endpoint = match effective_otlp_endpoint() {
                Some(ep) => ep,
                None => {
                    if env::var("AIFO_CODER_OTEL_VERBOSE").ok().as_deref() == Some("1") {
                        eprintln!(
                            "aifo-coder: telemetry: no OTLP endpoint available for metrics; disabling metrics exporter"
                        );
                    }
                    return None;
                }
            };

            let timeout = env::var("OTEL_EXPORTER_OTLP_TIMEOUT")
                .ok()
                .and_then(|s| humantime::parse_duration(&s).ok())
                .unwrap_or_else(|| Duration::from_secs(5));

            // Default to a 2s export interval when OTEL_METRICS_EXPORT_INTERVAL is unset.
            let interval = env::var("OTEL_METRICS_EXPORT_INTERVAL")
                .ok()
                .and_then(|s| humantime::parse_duration(&s).ok())
                .unwrap_or_else(|| Duration::from_secs(2));

            let exporter = match opentelemetry_otlp::new_exporter()
                .tonic()
                .with_endpoint(endpoint)
                .with_timeout(timeout)
                .build_metrics_exporter(
                    Box::<opentelemetry_sdk::metrics::reader::DefaultAggregationSelector>::default(
                    ),
                    Box::<opentelemetry_sdk::metrics::reader::DefaultTemporalitySelector>::default(
                    ),
                ) {
                Ok(exp) => exp,
                Err(e) => {
                    eprintln!(
                        "aifo-coder: telemetry: failed to create OTLP metrics exporter: {e}; disabling metrics exporter"
                    );
                    if env::var("AIFO_CODER_OTEL_VERBOSE").ok().as_deref() == Some("1") {
                        eprintln!(
                            "aifo-coder: telemetry: metrics export disabled; CLI behavior remains unchanged"
                        );
                    }
                    return None;
                }
            };

            let mut provider_builder = opentelemetry_sdk::metrics::SdkMeterProvider::builder()
                .with_resource(resource.clone());

            let reader = opentelemetry_sdk::metrics::PeriodicReader::builder(
                exporter,
                opentelemetry_sdk::runtime::Tokio,
            )
            .with_interval(interval)
            .build();

            provider_builder = provider_builder.with_reader(reader);

            return Some(provider_builder.build());
        }
    }

    #[cfg(not(feature = "otel-otlp"))]
    {
        if use_otlp {
            eprintln!(
                "aifo-coder: telemetry: OTLP endpoint configured for metrics but otel-otlp feature is disabled; metrics exporter will not be installed"
            );
        }
    }

    // Dev fallback: non-OTLP metrics exporter using stderr or a JSONL file sink.
    // Prefer stderr; if AIFO_CODER_OTEL_METRICS_FILE is set (or XDG_RUNTIME_DIR is available),
    // write JSONL to that path. Never write to stdout.
    let file_sink_path_opt = env::var("AIFO_CODER_OTEL_METRICS_FILE")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            // Default sink: ${XDG_RUNTIME_DIR:-/tmp}/aifo-coder.otel.metrics.jsonl
            let base = env::var("XDG_RUNTIME_DIR")
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "/tmp".to_string());
            Some(format!(
                "{}/aifo-coder.otel.metrics.jsonl",
                base.trim_end_matches('/')
            ))
        });

    // Build exporter with a writer to stderr or to a file.
    let exporter = if let Some(path) = file_sink_path_opt {
        match std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            Ok(f) => opentelemetry_stdout::MetricsExporterBuilder::default()
                .with_writer(f)
                .build(),
            Err(_) => {
                // Fallback: stderr
                opentelemetry_stdout::MetricsExporterBuilder::default()
                    .with_writer(std::io::stderr())
                    .build()
            }
        }
    } else {
        opentelemetry_stdout::MetricsExporterBuilder::default()
            .with_writer(std::io::stderr())
            .build()
    };

    // Use a PeriodicReader with ~2s export interval.
    let interval = env::var("OTEL_METRICS_EXPORT_INTERVAL")
        .ok()
        .and_then(|s| humantime::parse_duration(&s).ok())
        .unwrap_or_else(|| Duration::from_secs(2));

    let mut provider_builder =
        opentelemetry_sdk::metrics::SdkMeterProvider::builder().with_resource(resource.clone());

    let reader = opentelemetry_sdk::metrics::PeriodicReader::builder(
        exporter,
        opentelemetry_sdk::runtime::Tokio,
    )
    .with_interval(interval)
    .build();

    provider_builder = provider_builder.with_reader(reader);
    Some(provider_builder.build())
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

    let use_otlp = cfg!(feature = "otel-otlp") && effective_otlp_endpoint().is_some();

    // Verbose OTEL mode: driven by env, set by main when CLI --verbose is active.
    let verbose_otel = env::var("AIFO_CODER_OTEL_VERBOSE").ok().as_deref() == Some("1");

    if verbose_otel {
        if use_otlp {
            match env::var("OTEL_EXPORTER_OTLP_ENDPOINT") {
                Ok(ep) => {
                    let ep_trimmed = ep.trim();
                    if ep_trimmed.is_empty() {
                        eprintln!(
                            "aifo-coder: telemetry: OTLP enabled but OTEL_EXPORTER_OTLP_ENDPOINT is empty"
                        );
                    } else {
                        eprintln!(
                            "aifo-coder: telemetry: using OTLP endpoint {} (best-effort; export errors ignored)",
                            ep_trimmed
                        );
                    }
                }
                Err(_) => {
                    eprintln!(
                        "aifo-coder: telemetry: OTLP enabled but OTEL_EXPORTER_OTLP_ENDPOINT is unset"
                    );
                }
            }
        } else {
            eprintln!(
                "aifo-coder: telemetry: using stderr/file development exporters (no OTLP endpoint)"
            );
        }

        if env::var("AIFO_CODER_OTEL_METRICS").ok().as_deref() == Some("1") {
            if use_otlp {
                eprintln!("aifo-coder: telemetry: metrics: OTLP exporter requested (best-effort; failures ignored)");
            } else {
                eprintln!(
                    "aifo-coder: telemetry: metrics: dev exporter to stderr/file enabled (no OTLP)"
                );
            }
        } else {
            eprintln!("aifo-coder: telemetry: metrics: disabled (AIFO_CODER_OTEL_METRICS != 1)");
        }
    }

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

    // Base subscriber: registry + OpenTelemetry layer only.
    let base_subscriber = tracing_subscriber::registry().with(otel_layer);

    let fmt_enabled = env::var("AIFO_CODER_TRACING_FMT").ok().as_deref() == Some("1");

    if fmt_enabled {
        let filter = env::var("RUST_LOG").unwrap_or_else(|_| "warn".to_string());
        let env_filter = tracing_subscriber::EnvFilter::new(filter);
        let fmt_layer = tracing_subscriber::fmt::layer();
        let fmt_subscriber = base_subscriber.with(env_filter).with(fmt_layer);

        if fmt_subscriber.try_init().is_err() {
            eprintln!("aifo-coder: telemetry init skipped (global subscriber already set)");
            return None;
        }
    } else if base_subscriber.try_init().is_err() {
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
