#![allow(dead_code)]

use std::env;
use std::time::Duration;
use std::time::SystemTime;

use once_cell::sync::OnceCell;
use opentelemetry::global;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry::KeyValue;
use opentelemetry_sdk::error::OTelSdkResult;
use opentelemetry_sdk::logs::{
    LoggerProvider as SdkLoggerProvider, LoggerProviderBuilder, SdkLogRecord, SimpleLogProcessor,
};
use opentelemetry_sdk::metrics::exporter::PushMetricExporter;
use opentelemetry_sdk::metrics::{data::ResourceMetrics, SdkMeterProvider, Temporality};
use opentelemetry_sdk::propagation::TraceContextPropagator;
use opentelemetry_sdk::trace as sdktrace;
use opentelemetry_sdk::Resource;
use tracing_subscriber::prelude::*;

#[cfg(feature = "otel-otlp")]
use opentelemetry_otlp::WithExportConfig;

#[cfg(feature = "otel")]
pub mod metrics;

pub struct TelemetryGuard {
    meter_provider: Option<SdkMeterProvider>,
    #[cfg(feature = "otel-otlp")]
    runtime: Option<tokio::runtime::Runtime>,
    #[cfg(feature = "otel-otlp")]
    log_provider: Option<SdkLoggerProvider>,
}

static INIT: OnceCell<()> = OnceCell::new();
static INSTANCE_ID: OnceCell<String> = OnceCell::new();
static HASH_SALT: OnceCell<u64> = OnceCell::new();

// Default OTLP endpoint selection:
// - First, use AIFO_OTEL_DEFAULT_ENDPOINT baked in at compile time (via build.rs) when present.
// - Otherwise, fall back to a safe example endpoint for local collectors.
const DEFAULT_OTLP_ENDPOINT: Option<&str> = option_env!("AIFO_OTEL_DEFAULT_ENDPOINT");

// Default OTLP transport selection:
// - First, use AIFO_OTEL_DEFAULT_TRANSPORT baked in at compile time (via build.rs) when present.
// - Otherwise, fall back to "grpc".
const DEFAULT_OTLP_TRANSPORT: Option<&str> = option_env!("AIFO_OTEL_DEFAULT_TRANSPORT");

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum OtelTransport {
    Grpc,
    Http,
}

fn telemetry_enabled_env() -> bool {
    match env::var("AIFO_CODER_OTEL") {
        Ok(v) => {
            let v = v.trim().to_ascii_lowercase();
            matches!(v.as_str(), "1" | "true" | "yes")
        }
        Err(_) => true,
    }
}

fn telemetry_logs_enabled_env() -> bool {
    match env::var("AIFO_CODER_OTEL_LOGS") {
        Ok(v) => {
            let v = v.trim().to_ascii_lowercase();
            !(v == "0" || v == "false" || v == "no" || v == "off")
        }
        Err(_) => true,
    }
}

fn otel_transport() -> OtelTransport {
    // Force HTTP/HTTPS transport; ignore any grpc requests.
    OtelTransport::Http
}

fn effective_otlp_endpoint() -> Option<String> {
    // 1) Runtime override via OTEL_EXPORTER_OTLP_ENDPOINT
    if let Ok(v) = env::var("OTEL_EXPORTER_OTLP_ENDPOINT") {
        let t = v.trim();
        if !t.is_empty() {
            return Some(t.to_string());
        }
    }

    // 2) Baked-in default (if any), else 3) code default; trim at runtime.
    // Prefer HTTP/HTTPS form here so HTTP OTLP (including TLS) works out of the box.
    let baked = DEFAULT_OTLP_ENDPOINT.unwrap_or("https://localhost:4318");
    let t = baked.trim();
    if t.is_empty() {
        return Some("https://localhost:4318".to_string());
    }

    Some(t.to_string())
}

/// Return true if PII-rich telemetry is allowed (unsafe; for debugging only).
pub fn telemetry_pii_enabled() -> bool {
    env::var("AIFO_CODER_OTEL_PII").ok().as_deref() == Some("1")
}

/// Return true if debug mode should use stderr/file exporter for metrics.
fn telemetry_debug_otlp() -> bool {
    env::var("AIFO_CODER_OTEL_DEBUG_OTLP").ok().as_deref() == Some("1")
}

/// Return true when verbose OTEL logging is enabled (wired from CLI --verbose).
fn verbose_otel_enabled() -> bool {
    env::var("AIFO_CODER_OTEL_VERBOSE").ok().as_deref() == Some("1")
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

/* No explicit Resource configuration; use SDK defaults. */

fn instance_id() -> String {
    INSTANCE_ID
        .get_or_init(|| format!("{:016x}", hash_salt()))
        .clone()
}

fn build_resource() -> Resource {
    let name = env::var("OTEL_SERVICE_NAME")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "aifo-coder".to_string());

    Resource::builder()
        .with_attribute(KeyValue::new("service.name", name))
        .with_attribute(KeyValue::new("service.version", env!("CARGO_PKG_VERSION")))
        .with_attribute(KeyValue::new("service.instance.id", instance_id()))
        .build()
}

#[cfg(feature = "otel-otlp")]
fn build_tracer(
    _use_otlp: bool,
    _transport: OtelTransport,
) -> (sdktrace::SdkTracerProvider, Option<tokio::runtime::Runtime>) {
    // Silent tracer provider (no exporter).
    let provider = build_stderr_tracer();
    (provider, None)
}

#[cfg(not(feature = "otel-otlp"))]
fn build_tracer(_use_otlp: bool, _transport: OtelTransport) -> sdktrace::SdkTracerProvider {
    // Silent tracer provider (no exporter).
    build_stderr_tracer()
}

fn build_stderr_tracer() -> sdktrace::SdkTracerProvider {
    // Silent tracer provider (no exporter). For debugging, prefer enabling the fmt layer via
    // AIFO_CODER_TRACING_FMT=1 which prints spans/logs without custom exporters.
    sdktrace::SdkTracerProvider::builder()
        .with_resource(build_resource())
        .build()
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum MetricsStatus {
    InstalledOtlpHttp,
    InstalledDev,
    DisabledEnv,
    DisabledNoEndpoint,
    DisabledCreateFailed,
    DisabledLocalFlood,
}

fn build_metrics_provider_with_status(
    use_otlp: bool,
    _transport: OtelTransport,
) -> (Option<SdkMeterProvider>, MetricsStatus) {
    // Default: metrics enabled unless explicitly disabled via AIFO_CODER_OTEL_METRICS=0|false|no|off
    let metrics_enabled = env::var("AIFO_CODER_OTEL_METRICS")
        .ok()
        .map(|v| v.trim().to_ascii_lowercase())
        .map(|v| !(v == "0" || v == "false" || v == "no" || v == "off"))
        .unwrap_or(true);
    if !metrics_enabled {
        return (None, MetricsStatus::DisabledEnv);
    }

    // Export interval (best-effort; default ~2s)
    let interval = env::var("OTEL_METRICS_EXPORT_INTERVAL")
        .ok()
        .and_then(|s| humantime::parse_duration(&s).ok())
        .unwrap_or_else(|| Duration::from_secs(2));

    let mut provider_builder = opentelemetry_sdk::metrics::SdkMeterProvider::builder();
    provider_builder = provider_builder.with_resource(build_resource());

    // Debug mode: send metrics to stderr/file to inspect locally
    if telemetry_debug_otlp() {
        let exporter = opentelemetry_stdout::MetricExporterBuilder::default().build();
        let reader = opentelemetry_sdk::metrics::PeriodicReader::builder(exporter)
            .with_interval(interval)
            .build();
        provider_builder = provider_builder.with_reader(reader);
        return (Some(provider_builder.build()), MetricsStatus::InstalledDev);
    }

    // Wrap a PushMetricExporter to log export failures in verbose OTEL mode.
    struct LoggingMetricsExporter<E> {
        inner: E,
    }

    impl<E> LoggingMetricsExporter<E> {
        fn new(inner: E) -> Self {
            Self { inner }
        }
    }

    impl<E> PushMetricExporter for LoggingMetricsExporter<E>
    where
        E: PushMetricExporter + Send + Sync + 'static,
    {
        fn export(
            &self,
            rm: &ResourceMetrics,
        ) -> impl std::future::Future<Output = OTelSdkResult> + Send {
            let fut = self.inner.export(rm);
            async move {
                let res = fut.await;
                if let Err(ref err) = res {
                    if verbose_otel_enabled() {
                        let use_err = crate::color_enabled_stderr();
                        crate::log_warn_stderr(
                            use_err,
                            &format!("aifo-coder: telemetry: metrics export failed: {}", err),
                        );
                    }
                }
                res
            }
        }

        fn force_flush(&self) -> OTelSdkResult {
            let res = self.inner.force_flush();
            if let Err(ref err) = res {
                if verbose_otel_enabled() {
                    let use_err = crate::color_enabled_stderr();
                    crate::log_warn_stderr(
                        use_err,
                        &format!(
                            "aifo-coder: telemetry: metrics exporter force_flush failed: {}",
                            err
                        ),
                    );
                }
            }
            res
        }

        fn shutdown(&self) -> OTelSdkResult {
            self.inner.shutdown()
        }

        fn shutdown_with_timeout(&self, timeout: std::time::Duration) -> OTelSdkResult {
            self.inner.shutdown_with_timeout(timeout)
        }

        fn temporality(&self) -> Temporality {
            self.inner.temporality()
        }
    }

    // Prefer OTLP HTTP/HTTPS exporter when available; avoid stderr flooding otherwise.
    if use_otlp {
        #[cfg(feature = "otel-otlp")]
        {
            let ep =
                effective_otlp_endpoint().unwrap_or_else(|| "https://localhost:4318".to_string());
            let exporter = match opentelemetry_otlp::HttpExporterBuilder::default()
                .with_endpoint(ep)
                .build_metrics_exporter(Temporality::Cumulative)
            {
                Ok(exp) => exp,
                Err(e) => {
                    // Best-effort: disable metrics locally if exporter creation fails
                    if verbose_otel_enabled() {
                        let use_err = crate::color_enabled_stderr();
                        crate::log_warn_stderr(
                            use_err,
                            &format!(
                                "aifo-coder: telemetry: failed to build OTLP metrics exporter: {}",
                                e
                            ),
                        );
                    }
                    return (None, MetricsStatus::DisabledCreateFailed);
                }
            };
            // Wrap exporter so that runtime export/flush failures are logged in verbose OTEL mode.
            let logging_exporter = LoggingMetricsExporter::new(exporter);
            let reader = opentelemetry_sdk::metrics::PeriodicReader::builder(logging_exporter)
                .with_interval(interval)
                .build();
            provider_builder = provider_builder.with_reader(reader);
            return (
                Some(provider_builder.build()),
                MetricsStatus::InstalledOtlpHttp,
            );
        }
        #[cfg(not(feature = "otel-otlp"))]
        {
            return (None, MetricsStatus::DisabledNoEndpoint);
        }
    }

    // No endpoint available and not in debug mode: disable local metrics to avoid flooding.
    (None, MetricsStatus::DisabledLocalFlood)
}

#[cfg(feature = "otel-otlp")]
fn build_logger_provider(use_otlp: bool) -> Option<SdkLoggerProvider> {
    if !use_otlp {
        return None;
    }

    let endpoint = effective_otlp_endpoint()?;
    let exporter = opentelemetry_otlp::new_exporter()
        .http()
        .with_endpoint(endpoint)
        .build_log_exporter()
        .ok()?;

    let provider = LoggerProviderBuilder::default()
        .with_resource(build_resource())
        .with_log_processor(SimpleLogProcessor::new(exporter))
        .build();

    Some(provider)
}

#[cfg(feature = "otel-otlp")]
struct OtelLogLayer {
    logger: opentelemetry_sdk::logs::SdkLogger,
}

#[cfg(feature = "otel-otlp")]
impl OtelLogLayer {
    fn new(provider: &SdkLoggerProvider) -> Self {
        let logger = provider.logger("aifo-coder-logs");
        OtelLogLayer { logger }
    }
}

#[cfg(feature = "otel-otlp")]
impl<S> tracing_subscriber::Layer<S> for OtelLogLayer
where
    S: tracing::Subscriber + for<'span> tracing_subscriber::registry::LookupSpan<'span>,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        use opentelemetry::logs::Severity;
        use opentelemetry::KeyValue;

        let level = *event.metadata().level();

        // Flood control v1: send only INFO/WARN/ERROR to OTEL logs.
        if level < tracing::Level::INFO {
            return;
        }

        let severity = match level {
            tracing::Level::ERROR => Severity::Error,
            tracing::Level::WARN => Severity::Warn,
            tracing::Level::INFO => Severity::Info,
            tracing::Level::DEBUG => Severity::Debug,
            tracing::Level::TRACE => Severity::Trace,
        };

        let mut buf = String::new();
        use std::fmt::Write as _;
        let _ = write!(&mut buf, "{:?}", event);

        let meta = event.metadata();

        let mut record = SdkLogRecord::default();
        record.set_severity(severity);
        record.set_body(buf.into());
        record.add_attribute(KeyValue::new("logger.name", meta.target().to_string()));
        record.add_attribute(KeyValue::new(
            "logger.level",
            meta.level().as_str().to_string(),
        ));

        let _ = self.logger.emit(record);
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

    let use_otlp = cfg!(feature = "otel-otlp") && effective_otlp_endpoint().is_some();
    let transport = otel_transport();

    // Verbose OTEL mode: driven by env, set by main when CLI --verbose is active.
    let verbose_otel = env::var("AIFO_CODER_OTEL_VERBOSE").ok().as_deref() == Some("1");

    // Compute metrics_enabled here so logging and behavior stay in sync (default: disabled).
    let metrics_enabled = env::var("AIFO_CODER_OTEL_METRICS")
        .ok()
        .map(|v| v.trim().to_ascii_lowercase())
        .map(|v| !(v == "0" || v == "false" || v == "no" || v == "off"))
        .unwrap_or(true);

    if verbose_otel {
        let use_err = crate::color_enabled_stderr();
        if use_otlp {
            // Try to show the *effective* OTLP endpoint (runtime or baked-in), not just the env var.
            if let Some(ep) = effective_otlp_endpoint() {
                crate::log_info_stderr(
                    use_err,
                    &format!(
                        "aifo-coder: telemetry: using OTLP endpoint {} (best-effort; export errors ignored)",
                        ep
                    ),
                );
            } else {
                crate::log_info_stderr(
                    use_err,
                    "aifo-coder: telemetry: OTLP enabled but no effective endpoint could be resolved",
                );
            }
            crate::log_info_stderr(
                use_err,
                &format!(
                    "aifo-coder: telemetry: OTLP transport={}",
                    match transport {
                        OtelTransport::Grpc => "grpc",
                        OtelTransport::Http => "http",
                    }
                ),
            );
        } else {
            crate::log_info_stderr(
                use_err,
                "aifo-coder: telemetry: using stderr/file development exporters (no OTLP endpoint)",
            );
        }

        if metrics_enabled {
            if use_otlp {
                crate::log_info_stderr(
                    use_err,
                    "aifo-coder: telemetry: metrics: enabled (OTLP http)",
                );
            } else if telemetry_debug_otlp() {
                crate::log_info_stderr(
                    use_err,
                    "aifo-coder: telemetry: metrics: enabled (dev exporter to stderr/file)",
                );
            } else {
                crate::log_info_stderr(
                    use_err,
                    "aifo-coder: telemetry: metrics: enabled (no OTLP endpoint; disabled locally to avoid flooding)",
                );
            }
        } else {
            crate::log_info_stderr(
                use_err,
                "aifo-coder: telemetry: metrics: disabled (env override; enabled by default)",
            );
        }
    }

    #[cfg(feature = "otel-otlp")]
    let (tracer_provider, runtime) = build_tracer(use_otlp, transport);

    #[cfg(not(feature = "otel-otlp"))]
    let tracer_provider = build_tracer(use_otlp, transport);
    let tracer = tracer_provider.tracer("aifo-coder");

    let (meter_provider, metrics_status) = build_metrics_provider_with_status(use_otlp, transport);

    #[cfg(feature = "otel-otlp")]
    let log_provider = if use_otlp && telemetry_logs_enabled_env() {
        build_logger_provider(use_otlp)
    } else {
        None
    };

    if let Some(ref mp) = meter_provider {
        global::set_meter_provider(mp.clone());
    }
    if verbose_otel {
        let use_err = crate::color_enabled_stderr();
        let msg = match metrics_status {
            MetricsStatus::InstalledOtlpHttp => {
                "aifo-coder: telemetry: metrics exporter: installed (otlp http)"
            }
            MetricsStatus::InstalledDev => {
                "aifo-coder: telemetry: metrics exporter: installed (dev exporter to stderr/file)"
            }
            MetricsStatus::DisabledEnv => {
                "aifo-coder: telemetry: metrics exporter: disabled (env)"
            }
            MetricsStatus::DisabledNoEndpoint => {
                "aifo-coder: telemetry: metrics exporter: disabled (no endpoint)"
            }
            MetricsStatus::DisabledCreateFailed => {
                "aifo-coder: telemetry: metrics exporter: disabled (exporter creation failed)"
            }
            MetricsStatus::DisabledLocalFlood => {
                "aifo-coder: telemetry: metrics exporter: disabled (local; no endpoint; avoiding flooding)"
            }
        };
        crate::log_info_stderr(use_err, msg);
    }

    global::set_tracer_provider(tracer_provider);
    let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

    let registry = tracing_subscriber::registry().with(otel_layer);

    #[cfg(feature = "otel-otlp")]
    let registry = if let Some(ref lp) = log_provider {
        let log_layer = OtelLogLayer::new(lp);
        registry.with(log_layer)
    } else {
        registry
    };

    #[cfg(not(feature = "otel-otlp"))]
    let registry = registry;

    // Base subscriber: registry + OpenTelemetry (and optional logs) layers.
    let base_subscriber = registry;

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
            log_provider,
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
        #[cfg(feature = "otel-otlp")]
        {
            if let Some(rt) = self.runtime.take() {
                drop(rt);
            }
        }
    }
}
