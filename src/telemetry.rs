#![allow(dead_code)]

use std::env;
use std::time::Duration;
use std::time::SystemTime;

use once_cell::sync::OnceCell;
use opentelemetry::global;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::propagation::TraceContextPropagator;
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
    sdktrace::SdkTracerProvider::builder().build()
}

fn build_metrics_provider(use_otlp: bool, _transport: OtelTransport) -> Option<SdkMeterProvider> {
    // Default: metrics enabled unless explicitly disabled via AIFO_CODER_OTEL_METRICS=0|false|no|off
    let metrics_enabled = env::var("AIFO_CODER_OTEL_METRICS")
        .ok()
        .map(|v| v.trim().to_ascii_lowercase())
        .map(|v| !(v == "0" || v == "false" || v == "no" || v == "off"))
        .unwrap_or(true);
    if !metrics_enabled {
        return None;
    }

    // Export interval (best-effort; default ~2s)
    let interval = env::var("OTEL_METRICS_EXPORT_INTERVAL")
        .ok()
        .and_then(|s| humantime::parse_duration(&s).ok())
        .unwrap_or_else(|| Duration::from_secs(2));

    let mut provider_builder = opentelemetry_sdk::metrics::SdkMeterProvider::builder();

    // Debug mode: send metrics to stderr/file to inspect locally
    if telemetry_debug_otlp() {
        let exporter = opentelemetry_stdout::MetricExporterBuilder::default().build();
        let reader = opentelemetry_sdk::metrics::PeriodicReader::builder(exporter)
            .with_interval(interval)
            .build();
        provider_builder = provider_builder.with_reader(reader);
        return Some(provider_builder.build());
    }

    // Prefer OTLP HTTP/HTTPS exporter when available; avoid stderr flooding otherwise.
    if use_otlp {
        #[cfg(feature = "otel-otlp")]
        {
            let ep =
                effective_otlp_endpoint().unwrap_or_else(|| "https://localhost:4318".to_string());
            let exporter = match opentelemetry_otlp::new_exporter()
                .http()
                .with_endpoint(ep)
                .build_metrics_exporter()
            {
                Ok(exp) => exp,
                Err(_e) => {
                    // Best-effort: disable metrics locally if exporter creation fails
                    return None;
                }
            };
            let reader = opentelemetry_sdk::metrics::PeriodicReader::builder(exporter)
                .with_interval(interval)
                .build();
            provider_builder = provider_builder.with_reader(reader);
            return Some(provider_builder.build());
        }
    }

    // No endpoint available and not in debug mode: disable local metrics to avoid flooding.
    None
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
        if use_otlp {
            // Try to show the *effective* OTLP endpoint (runtime or baked-in), not just the env var.
            if let Some(ep) = effective_otlp_endpoint() {
                eprintln!(
                    "aifo-coder: telemetry: using OTLP endpoint {} (best-effort; export errors ignored)",
                    ep
                );
            } else {
                eprintln!(
                    "aifo-coder: telemetry: OTLP enabled but no effective endpoint could be resolved"
                );
            }
            eprintln!(
                "aifo-coder: telemetry: OTLP transport={}",
                match transport {
                    OtelTransport::Grpc => "grpc",
                    OtelTransport::Http => "http",
                }
            );
        } else {
            eprintln!(
                "aifo-coder: telemetry: using stderr/file development exporters (no OTLP endpoint)"
            );
        }

        if metrics_enabled {
            if use_otlp {
                eprintln!("aifo-coder: telemetry: metrics: enabled (OTLP http)");
            } else if telemetry_debug_otlp() {
                eprintln!("aifo-coder: telemetry: metrics: enabled (dev exporter to stderr/file)");
            } else {
                eprintln!(
                    "aifo-coder: telemetry: metrics: enabled (no OTLP endpoint; disabled locally to avoid flooding)"
                );
            }
        } else {
            eprintln!(
                "aifo-coder: telemetry: metrics: disabled (env override; enabled by default)"
            );
        }
    }

    #[cfg(feature = "otel-otlp")]
    let (tracer_provider, runtime) = build_tracer(use_otlp, transport);

    #[cfg(not(feature = "otel-otlp"))]
    let tracer_provider = build_tracer(use_otlp, transport);
    let tracer = tracer_provider.tracer("aifo-coder");

    let meter_provider = build_metrics_provider(use_otlp, transport);
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
        #[cfg(feature = "otel-otlp")]
        {
            if let Some(rt) = self.runtime.take() {
                drop(rt);
            }
        }
    }
}
