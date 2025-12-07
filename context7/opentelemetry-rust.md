# OpenTelemetry Rust

OpenTelemetry Rust is the official Rust implementation of the OpenTelemetry observability framework, providing APIs and SDKs for instrumenting applications to collect distributed traces, metrics, and logs. It enables developers to instrument their Rust applications once and export telemetry data to various observability backends including Prometheus, Jaeger, Zipkin, and any OTLP-compatible collector. The library follows OpenTelemetry specifications and provides both stable APIs for production use and evolving features for advanced observability scenarios.

The project is organized as a workspace containing multiple crates: the core `opentelemetry` API crate for instrumentation, the `opentelemetry-sdk` implementation crate, exporters for different protocols (OTLP, Prometheus, Jaeger, Zipkin, stdout), and appenders that bridge existing Rust logging libraries (tracing, log) to OpenTelemetry's telemetry data model. This modular architecture allows developers to use only the components they need while maintaining compatibility across the observability ecosystem.

## Initializing Traces with OTLP Exporter (gRPC)

Setting up distributed tracing with the OTLP exporter over gRPC transport to send spans to an OpenTelemetry collector or compatible backend.

```rust
use opentelemetry::{global, trace::Tracer, KeyValue, InstrumentationScope};
use opentelemetry_otlp::SpanExporter;
use opentelemetry_sdk::{trace::SdkTracerProvider, Resource};

fn init_traces() -> SdkTracerProvider {
    let exporter = SpanExporter::builder()
        .with_tonic()  // Use gRPC with tonic
        .build()
        .expect("Failed to create span exporter");

    let provider = SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_resource(
            Resource::builder()
                .with_service_name("my-service")
                .build()
        )
        .build();

    global::set_tracer_provider(provider.clone());
    provider
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let tracer_provider = init_traces();

    // Create instrumentation scope with metadata
    let scope = InstrumentationScope::builder("my-service")
        .with_version("1.0")
        .with_attributes([KeyValue::new("scope-key", "scope-value")])
        .build();

    let tracer = global::tracer_with_scope(scope);

    // Create parent span
    tracer.in_span("Main operation", |cx| {
        let span = cx.span();
        span.set_attribute(KeyValue::new("http.method", "GET"));
        span.add_event(
            "Processing started",
            vec![KeyValue::new("stage", "init")],
        );

        // Create nested child span
        tracer.in_span("Sub operation", |cx| {
            let span = cx.span();
            span.set_attribute(KeyValue::new("db.query", "SELECT * FROM users"));
        });
    });

    tracer_provider.shutdown()?;
    Ok(())
}
```

## Initializing Metrics with OTLP Exporter (HTTP)

Configuring metrics collection with the OTLP exporter using HTTP transport and binary protobuf encoding for efficient metric data export.

```rust
use opentelemetry::{global, KeyValue};
use opentelemetry_otlp::{MetricExporter, Protocol};
use opentelemetry_sdk::{metrics::SdkMeterProvider, Resource};

fn init_metrics() -> SdkMeterProvider {
    let exporter = MetricExporter::builder()
        .with_http()  // Use HTTP transport
        .with_protocol(Protocol::HttpBinary)  // Binary protobuf
        .build()
        .expect("Failed to create metric exporter");

    let provider = SdkMeterProvider::builder()
        .with_periodic_exporter(exporter)  // Automatically exports every 60s
        .with_resource(
            Resource::builder()
                .with_service_name("metrics-service")
                .with_service_namespace("production")
                .build()
        )
        .build();

    global::set_meter_provider(provider.clone());
    provider
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let meter_provider = init_metrics();
    let meter = global::meter("my-library");

    // Create counter for request counting
    let request_counter = meter
        .u64_counter("requests.total")
        .with_description("Total number of requests")
        .with_unit("requests")
        .build();

    request_counter.add(1, &[
        KeyValue::new("method", "GET"),
        KeyValue::new("endpoint", "/api/users"),
    ]);

    // Create histogram for latency tracking
    let latency_histogram = meter
        .f64_histogram("request.duration")
        .with_description("Request duration in seconds")
        .with_unit("s")
        .build();

    latency_histogram.record(0.235, &[
        KeyValue::new("endpoint", "/api/users"),
        KeyValue::new("status", "200"),
    ]);

    meter_provider.shutdown()?;
    Ok(())
}
```

## Recording All Metric Instrument Types

Comprehensive example demonstrating all available metric instruments including counters, gauges, histograms, and their observable variants with callback-based reporting.

```rust
use opentelemetry::{global, KeyValue};
use opentelemetry_sdk::{metrics::SdkMeterProvider, Resource};
use std::error::Error;

fn init_meter_provider() -> SdkMeterProvider {
    let exporter = opentelemetry_stdout::MetricExporterBuilder::default().build();
    let provider = SdkMeterProvider::builder()
        .with_periodic_exporter(exporter)
        .with_resource(
            Resource::builder()
                .with_service_name("metrics-demo")
                .build(),
        )
        .build();
    global::set_meter_provider(provider.clone());
    provider
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let meter_provider = init_meter_provider();
    let meter = global::meter("mylibraryname");

    // Counter: monotonically increasing value
    let counter = meter.u64_counter("my_counter").build();
    counter.add(10, &[
        KeyValue::new("key1", "value1"),
        KeyValue::new("key2", "value2"),
    ]);

    // ObservableCounter: callback-based counter
    let _observable_counter = meter
        .u64_observable_counter("my_observable_counter")
        .with_description("Callback-based counter")
        .with_unit("items")
        .with_callback(|observer| {
            observer.observe(100, &[KeyValue::new("source", "callback")])
        })
        .build();

    // UpDownCounter: can increase or decrease
    let updown_counter = meter.i64_up_down_counter("active_connections").build();
    updown_counter.add(5, &[]);   // Connection opened
    updown_counter.add(-2, &[]);  // Connections closed

    // ObservableUpDownCounter: callback-based up/down counter
    let _observable_updown = meter
        .i64_observable_up_down_counter("memory_usage")
        .with_callback(|observer| {
            let memory_bytes = 1024 * 1024 * 512; // 512 MB
            observer.observe(memory_bytes, &[])
        })
        .build();

    // Histogram: distribution of values
    let histogram = meter
        .f64_histogram("response_time")
        .with_description("HTTP response time distribution")
        .with_boundaries(vec![0.0, 5.0, 10.0, 15.0, 20.0, 25.0])
        .build();

    histogram.record(10.5, &[
        KeyValue::new("endpoint", "/api"),
        KeyValue::new("method", "POST"),
    ]);

    // Gauge: instantaneous measurement
    let gauge = meter
        .f64_gauge("cpu_temperature")
        .with_description("Current CPU temperature")
        .with_unit("celsius")
        .build();

    gauge.record(72.5, &[KeyValue::new("core", "0")]);

    // ObservableGauge: callback-based gauge
    let _observable_gauge = meter
        .f64_observable_gauge("cpu_usage")
        .with_description("Current CPU usage percentage")
        .with_unit("percent")
        .with_callback(|observer| {
            observer.observe(45.3, &[KeyValue::new("core", "0")])
        })
        .build();

    meter_provider.shutdown()?;
    Ok(())
}
```

## Using Metric Views for Transformation

Applying views to metrics for renaming instruments, changing units, adjusting cardinality limits, and controlling metric aggregation behavior.

```rust
use opentelemetry::{global, KeyValue};
use opentelemetry_sdk::metrics::{Instrument, SdkMeterProvider, Stream, Temporality};
use opentelemetry_sdk::Resource;

fn init_meter_provider() -> SdkMeterProvider {
    // View 1: Rename and change unit
    let rename_view = |i: &Instrument| {
        if i.name() == "my_histogram" {
            Some(
                Stream::builder()
                    .with_name("my_histogram_renamed")
                    .with_unit("milliseconds")
                    .build()
                    .unwrap(),
            )
        } else {
            None
        }
    };

    // View 2: Limit cardinality to prevent metric explosion
    let cardinality_view = |i: &Instrument| {
        if i.name() == "my_second_histogram" {
            Stream::builder()
                .with_cardinality_limit(2)  // Only 2 unique attribute sets
                .build()
                .ok()
        } else {
            None
        }
    };

    // Use Delta temporality (good for rate-based systems)
    let exporter = opentelemetry_stdout::MetricExporterBuilder::default()
        .with_temporality(Temporality::Delta)
        .build();

    let provider = SdkMeterProvider::builder()
        .with_periodic_exporter(exporter)
        .with_resource(Resource::builder().with_service_name("metrics-advanced").build())
        .with_view(rename_view)
        .with_view(cardinality_view)
        .build();

    global::set_meter_provider(provider.clone());
    provider
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let meter_provider = init_meter_provider();
    let meter = global::meter("mylibraryname");

    // This will be renamed to "my_histogram_renamed"
    let histogram = meter
        .f64_histogram("my_histogram")
        .with_unit("ms")
        .build();

    histogram.record(10.5, &[
        KeyValue::new("key1", "value1"),
        KeyValue::new("key2", "value2"),
    ]);

    // This will have cardinality limit of 2
    let histogram2 = meter.f64_histogram("my_second_histogram").build();

    histogram2.record(1.5, &[KeyValue::new("mykey", "v1")]);  // Recorded
    histogram2.record(1.2, &[KeyValue::new("mykey", "v2")]);  // Recorded
    histogram2.record(1.7, &[KeyValue::new("mykey", "v1")]);  // OK (already seen)
    histogram2.record(1.8, &[KeyValue::new("mykey", "v3")]);  // Overflow!
    histogram2.record(1.9, &[KeyValue::new("mykey", "v4")]);  // Overflow!

    meter_provider.shutdown()?;
    Ok(())
}
```

## Initializing Logs with Tracing Appender

Bridging the tracing logging library to OpenTelemetry's log data model with proper filtering to prevent telemetry-induced-telemetry loops.

```rust
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_otlp::LogExporter;
use opentelemetry_sdk::{logs::SdkLoggerProvider, Resource};
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

fn init_logs() -> SdkLoggerProvider {
    let exporter = LogExporter::builder()
        .with_tonic()
        .build()
        .expect("Failed to create log exporter");

    let provider = SdkLoggerProvider::builder()
        .with_batch_exporter(exporter)
        .with_resource(
            Resource::builder()
                .with_service_name("my-service")
                .build()
        )
        .build();

    let otel_layer = OpenTelemetryTracingBridge::new(&provider);

    // Critical: Filter to prevent telemetry-induced-telemetry loops
    let filter_otel = EnvFilter::new("info")
        .add_directive("hyper=off".parse().unwrap())
        .add_directive("tonic=off".parse().unwrap())
        .add_directive("h2=off".parse().unwrap())
        .add_directive("reqwest=off".parse().unwrap());

    let otel_layer = otel_layer.with_filter(filter_otel);

    // Add console logging layer
    let filter_fmt = EnvFilter::new("info")
        .add_directive("opentelemetry=debug".parse().unwrap());
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_thread_names(true)
        .with_filter(filter_fmt);

    tracing_subscriber::registry()
        .with(otel_layer)
        .with(fmt_layer)
        .init();

    provider
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let logger_provider = init_logs();

    // Use tracing macros with structured fields
    info!(
        name: "user_login",
        target: "auth-service",
        user_id = 12345,
        ip_address = "192.168.1.1",
        "User logged in successfully"
    );

    error!(
        name: "database_error",
        target: "db-service",
        error_code = "ERR_CONN_TIMEOUT",
        "Failed to connect to database"
    );

    logger_provider.shutdown()?;
    Ok(())
}
```

## Propagating Context over HTTP

Extracting and injecting distributed trace context across HTTP requests using W3C Trace Context and Baggage propagators for end-to-end correlation.

```rust
use hyper::{body::Incoming, Request, Response};
use opentelemetry::{
    baggage::BaggageExt,
    global,
    propagation::TextMapCompositePropagator,
    trace::{FutureExt, SpanKind, TraceContextExt, Tracer},
    Context, KeyValue,
};
use opentelemetry_http::{Bytes, HeaderExtractor};
use opentelemetry_sdk::{
    propagation::{BaggagePropagator, TraceContextPropagator},
    trace::SdkTracerProvider,
};
use opentelemetry_stdout::SpanExporter;

fn extract_context_from_request(req: &Request<Incoming>) -> Context {
    global::get_text_map_propagator(|propagator| {
        propagator.extract(&HeaderExtractor(req.headers()))
    })
}

fn init_tracer() -> SdkTracerProvider {
    // Setup composite propagator
    let composite_propagator = TextMapCompositePropagator::new(vec![
        Box::new(BaggagePropagator::new()),
        Box::new(TraceContextPropagator::new()),
    ]);

    global::set_text_map_propagator(composite_propagator);

    let provider = SdkTracerProvider::builder()
        .with_simple_exporter(SpanExporter::default())
        .build();

    global::set_tracer_provider(provider.clone());
    provider
}

async fn router(req: Request<Incoming>) -> Result<Response<String>, std::io::Error> {
    // Extract parent context from incoming request headers
    let parent_cx = extract_context_from_request(&req);

    let tracer = global::tracer("http-server");

    // Create server span with extracted parent context
    let span = tracer
        .span_builder("handle_request")
        .with_kind(SpanKind::Server)
        .start_with_context(&tracer, &parent_cx);

    let cx = parent_cx.with_span(span);

    // Access baggage from context
    let user_id = cx.baggage().get("user.id").map(|v| v.0.as_str());
    cx.span().set_attribute(KeyValue::new("user.id", user_id.unwrap_or("unknown")));

    // Execute handler with context
    async move {
        cx.span().add_event("Processing request", vec![]);
        Ok(Response::new("Success".to_string()))
    }
    .with_context(cx)  // Propagate context to async block
    .await
}

#[tokio::main]
async fn main() {
    let provider = init_tracer();

    // HTTP server setup would go here

    provider.shutdown().expect("Shutdown failed");
}
```

## Creating Custom Span and Log Processors

Implementing custom processors to enrich telemetry data with baggage attributes for cross-cutting concerns like user context and request metadata.

```rust
use opentelemetry::{baggage::BaggageExt, logs::LogRecord, Context, KeyValue};
use opentelemetry_sdk::{
    error::OTelSdkResult,
    logs::{LogProcessor, SdkLogRecord, SdkLoggerProvider},
    trace::{SpanData, SpanProcessor, SdkTracerProvider},
    InstrumentationScope,
};
use opentelemetry_stdout::{LogExporter, SpanExporter};
use std::time::Duration;

#[derive(Debug)]
struct EnrichWithBaggageSpanProcessor;

impl SpanProcessor for EnrichWithBaggageSpanProcessor {
    fn on_start(&self, span: &mut opentelemetry_sdk::trace::Span, cx: &Context) {
        // Add all baggage items as span attributes
        for (key, value) in cx.baggage().iter() {
            span.set_attribute(KeyValue::new(key.clone(), value.0.clone()));
        }
    }

    fn on_end(&self, _span: SpanData) {}

    fn force_flush(&self) -> OTelSdkResult {
        Ok(())
    }

    fn shutdown_with_timeout(&self, _timeout: Duration) -> OTelSdkResult {
        Ok(())
    }
}

#[derive(Debug)]
struct EnrichWithBaggageLogProcessor;

impl LogProcessor for EnrichWithBaggageLogProcessor {
    fn emit(&self, data: &mut SdkLogRecord, _instrumentation: &InstrumentationScope) {
        Context::map_current(|cx| {
            // Add all baggage items as log attributes
            for (key, value) in cx.baggage().iter() {
                data.add_attribute(key.clone(), value.0.clone());
            }
        });
    }

    fn force_flush(&self) -> OTelSdkResult {
        Ok(())
    }
}

fn init_with_processors() -> (SdkTracerProvider, SdkLoggerProvider) {
    let tracer_provider = SdkTracerProvider::builder()
        .with_span_processor(EnrichWithBaggageSpanProcessor)
        .with_simple_exporter(SpanExporter::default())
        .build();

    let logger_provider = SdkLoggerProvider::builder()
        .with_log_processor(EnrichWithBaggageLogProcessor)
        .with_simple_exporter(LogExporter::default())
        .build();

    (tracer_provider, logger_provider)
}

fn main() {
    let (tracer_provider, logger_provider) = init_with_processors();

    // Use providers...

    tracer_provider.shutdown().unwrap();
    logger_provider.shutdown().unwrap();
}
```

## Complete Multi-Signal Initialization

Production-ready initialization pattern combining traces, metrics, and logs with proper resource attribution and graceful shutdown handling.

```rust
use opentelemetry::{global, trace::Tracer, InstrumentationScope, KeyValue};
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_otlp::{LogExporter, MetricExporter, SpanExporter};
use opentelemetry_sdk::{
    logs::SdkLoggerProvider,
    metrics::SdkMeterProvider,
    trace::SdkTracerProvider,
    Resource,
};
use std::error::Error;
use std::sync::OnceLock;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

fn get_resource() -> Resource {
    static RESOURCE: OnceLock<Resource> = OnceLock::new();
    RESOURCE
        .get_or_init(|| {
            Resource::builder()
                .with_service_name("my-production-service")
                .with_service_namespace("production")
                .with_service_instance_id("instance-1")
                .with_attributes([
                    KeyValue::new("deployment.environment", "production"),
                    KeyValue::new("service.version", "1.2.3"),
                ])
                .build()
        })
        .clone()
}

fn init_traces() -> SdkTracerProvider {
    let exporter = SpanExporter::builder()
        .with_tonic()
        .build()
        .expect("Failed to create span exporter");

    SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_resource(get_resource())
        .build()
}

fn init_metrics() -> SdkMeterProvider {
    let exporter = MetricExporter::builder()
        .with_tonic()
        .build()
        .expect("Failed to create metric exporter");

    SdkMeterProvider::builder()
        .with_periodic_exporter(exporter)
        .with_resource(get_resource())
        .build()
}

fn init_logs() -> SdkLoggerProvider {
    let exporter = LogExporter::builder()
        .with_tonic()
        .build()
        .expect("Failed to create log exporter");

    SdkLoggerProvider::builder()
        .with_batch_exporter(exporter)
        .with_resource(get_resource())
        .build()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
    // Initialize logs first to capture initialization messages
    let logger_provider = init_logs();

    let otel_layer = OpenTelemetryTracingBridge::new(&logger_provider);
    let filter = EnvFilter::new("info")
        .add_directive("hyper=off".parse().unwrap())
        .add_directive("tonic=off".parse().unwrap());

    tracing_subscriber::registry()
        .with(otel_layer.with_filter(filter))
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Initialize traces and metrics
    let tracer_provider = init_traces();
    let meter_provider = init_metrics();

    global::set_tracer_provider(tracer_provider.clone());
    global::set_meter_provider(meter_provider.clone());

    // Create instrumentation scope
    let scope = InstrumentationScope::builder("my-service")
        .with_version(env!("CARGO_PKG_VERSION"))
        .with_attributes([KeyValue::new("component", "main")])
        .build();

    let tracer = global::tracer_with_scope(scope.clone());
    let meter = global::meter_with_scope(scope);

    // Use telemetry
    let counter = meter.u64_counter("operations.count").build();

    tracer.in_span("main_operation", |cx| {
        let span = cx.span();
        span.set_attribute(KeyValue::new("operation.type", "startup"));

        counter.add(1, &[KeyValue::new("status", "success")]);

        info!(
            name: "operation_complete",
            target: "my-service",
            "Application started successfully"
        );
    });

    // Graceful shutdown with error collection
    let mut shutdown_errors = Vec::new();

    if let Err(e) = tracer_provider.shutdown() {
        shutdown_errors.push(format!("tracer provider: {e}"));
    }

    if let Err(e) = meter_provider.shutdown() {
        shutdown_errors.push(format!("meter provider: {e}"));
    }

    if let Err(e) = logger_provider.shutdown() {
        shutdown_errors.push(format!("logger provider: {e}"));
    }

    if !shutdown_errors.is_empty() {
        return Err(format!(
            "Failed to shutdown providers:\n{}",
            shutdown_errors.join("\n")
        )
        .into());
    }

    Ok(())
}
```

## Creating Spans with Advanced Features

Comprehensive span creation patterns including span builders, attributes, events, error recording, status setting, and linking between traces.

```rust
use opentelemetry::{
    global,
    trace::{Link, Span, SpanContext, SpanKind, Status, Tracer, TraceFlags, TraceId},
    Context, KeyValue,
};
use std::borrow::Cow;

#[tokio::main]
async fn main() {
    let tracer = global::tracer("advanced-tracing");

    // Pattern 1: Span builder with full configuration
    let span = tracer
        .span_builder("database_query")
        .with_kind(SpanKind::Client)
        .with_attributes([
            KeyValue::new("db.system", "postgresql"),
            KeyValue::new("db.name", "users"),
            KeyValue::new("db.operation", "SELECT"),
            KeyValue::new("db.statement", "SELECT * FROM users WHERE id = $1"),
        ])
        .start(&tracer);

    // Pattern 2: Adding events to spans
    let mut span = tracer.start("processing_request");
    span.add_event("validation_started", vec![]);
    span.add_event(
        "validation_complete",
        vec![KeyValue::new("fields_validated", 5)],
    );

    // Pattern 3: Recording errors
    let result: Result<(), Box<dyn std::error::Error>> =
        Err("Connection timeout".into());

    if let Err(err) = result {
        span.record_error(&*err);
        span.set_status(Status::Error {
            description: Cow::from("Database connection failed"),
        });
    } else {
        span.set_status(Status::Ok);
    }
    span.end();

    // Pattern 4: Creating links between spans
    let remote_trace_id = TraceId::from_hex("4bf92f3577b34da6a3ce929d0e0e4736").unwrap();
    let remote_span_id = opentelemetry::trace::SpanId::from_hex("00f067aa0ba902b7").unwrap();
    let remote_context = SpanContext::new(
        remote_trace_id,
        remote_span_id,
        TraceFlags::SAMPLED,
        true,
        Default::default(),
    );

    let link = Link::new(remote_context, vec![
        KeyValue::new("link.type", "follows_from"),
        KeyValue::new("link.source", "external_system"),
    ]);

    let span = tracer
        .span_builder("linked_operation")
        .with_links(vec![link])
        .start(&tracer);

    // Pattern 5: Nested spans with explicit context
    tracer.in_span("parent_span", |parent_cx| {
        let parent_span = parent_cx.span();
        parent_span.set_attribute(KeyValue::new("level", "parent"));

        let child_span = tracer.start_with_context("child_span", parent_cx);
        let child_cx = Context::current_with_span(child_span);

        child_cx.span().set_attribute(KeyValue::new("level", "child"));
        child_cx.span().end();
    });
}
```

## Using Stdout Exporter for Development

Debug-friendly configuration using stdout exporters for traces, metrics, and logs to inspect telemetry data during development without external infrastructure.

```rust
use once_cell::sync::Lazy;
use opentelemetry::{global, trace::Tracer, KeyValue, InstrumentationScope};
use opentelemetry_appender_tracing::layer;
use opentelemetry_sdk::{
    logs::SdkLoggerProvider,
    metrics::SdkMeterProvider,
    trace::SdkTracerProvider,
    Resource,
};
use tracing::error;
use tracing_subscriber::prelude::*;

static RESOURCE: Lazy<Resource> = Lazy::new(|| {
    Resource::builder()
        .with_service_name("development-app")
        .build()
});

fn init_trace() -> SdkTracerProvider {
    let exporter = opentelemetry_stdout::SpanExporter::default();
    let provider = SdkTracerProvider::builder()
        .with_simple_exporter(exporter)
        .with_resource(RESOURCE.clone())
        .build();
    global::set_tracer_provider(provider.clone());
    provider
}

fn init_metrics() -> SdkMeterProvider {
    let exporter = opentelemetry_stdout::MetricExporter::default();
    let provider = SdkMeterProvider::builder()
        .with_periodic_exporter(exporter)
        .with_resource(RESOURCE.clone())
        .build();
    global::set_meter_provider(provider.clone());
    provider
}

fn init_logs() -> SdkLoggerProvider {
    let exporter = opentelemetry_stdout::LogExporter::default();
    let provider = SdkLoggerProvider::builder()
        .with_simple_exporter(exporter)
        .with_resource(RESOURCE.clone())
        .build();
    let layer = layer::OpenTelemetryTracingBridge::new(&provider);
    tracing_subscriber::registry().with(layer).init();
    provider
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let tracer_provider = init_trace();
    let meter_provider = init_metrics();
    let logger_provider = init_logs();

    // Create scope with metadata
    let scope = InstrumentationScope::builder("dev-example")
        .with_version("v1")
        .with_attributes([KeyValue::new("environment", "development")])
        .build();

    let tracer = global::tracer_with_scope(scope);
    let meter = global::meter("dev-example");

    // Generate telemetry
    tracer.in_span("example-span", |cx| {
        let span = cx.span();
        span.set_attribute(KeyValue::new("debug", "true"));
        span.add_event(
            "debug-event",
            vec![KeyValue::new("timestamp", "2025-01-01T00:00:00Z")],
        );

        error!(
            name: "error-event",
            target: "dev-system",
            error_code = 500,
            "Example error message"
        );
    });

    let counter = meter.u64_counter("debug_counter").build();
    counter.add(42, &[KeyValue::new("color", "blue")]);

    let histogram = meter.f64_histogram("debug_histogram").build();
    histogram.record(123.45, &[KeyValue::new("size", "large")]);

    // Shutdown flushes all telemetry to stdout
    tracer_provider.shutdown()?;
    meter_provider.shutdown()?;
    logger_provider.shutdown()?;

    Ok(())
}
```

## Summary

OpenTelemetry Rust provides a comprehensive, production-ready observability solution for Rust applications with support for distributed tracing, metrics collection, and structured logging. The main use cases include instrumenting microservices for distributed tracing across service boundaries, collecting application and business metrics for monitoring and alerting, bridging existing logging frameworks to export logs alongside traces and metrics, and exporting telemetry data to various backends through standardized protocols like OTLP. The library is designed for both greenfield applications starting fresh with OpenTelemetry and brownfield applications gradually adopting observability through its bridge adapters for popular logging libraries.

Integration patterns follow a consistent builder-based initialization approach where applications create provider instances (TracerProvider, MeterProvider, LoggerProvider) configured with exporters and resource attributes, register them globally for convenient access throughout the codebase, instrument code using API methods for creating spans and recording metrics, and perform graceful shutdown to flush remaining telemetry before exit. Advanced patterns include using Views to transform metrics for controlling cardinality and aggregation, implementing custom processors to enrich telemetry with cross-cutting concerns like user context, composing multiple propagators for comprehensive context propagation across service boundaries, and configuring sampling strategies to balance observability coverage with performance overhead. The modular architecture allows teams to adopt signals incrementally and swap exporters without changing instrumentation code.

