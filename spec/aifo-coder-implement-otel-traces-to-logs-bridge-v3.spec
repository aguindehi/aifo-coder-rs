
Title: Implement OTEL traces-to-logs bridge (v3) using opentelemetry_sdk::logs

Status: draft
Applies-to:
  - Cargo.toml
  - src/telemetry.rs
  - (comments only) docs/README-opentelemetry.md

Goals:
  - Reintroduce an OpenTelemetry logs bridge that converts `tracing` events to OTEL log records.
  - Use the opentelemetry_sdk `logs` pipeline (LoggerProviderBuilder + LogExporter/LogProcessor).
  - Keep logs export feature-gated (`otel-otlp`) and disabled cleanly when no endpoint or exporter is available.
  - Preserve existing behavior and invariants:
      - CLI stdout and exit codes must not change due to telemetry.
      - Telemetry and logs failures must never panic or abort the CLI.
  - Respect existing env controls (`AIFO_CODER_*`), and match README semantics:
      - `AIFO_CODER_OTEL` (overall telemetry),
      - `AIFO_CODER_OTEL_LOGS` (logs bridge),
      - `AIFO_CODER_OTEL_VERBOSE` (stderr diagnostics),
      - `AIFO_CODER_TRACING_FMT` (fmt layer, unrelated to OTEL logs but must coexist).

Non-goals:
  - No new CLI flags.
  - No new public API surface beyond what already exists (telemetry_init and envs).
  - No complex context propagation for logs (we do not attempt to add trace/span IDs as log attributes in v3).


-------------------------------------------------------------------------------
1. Dependencies and features
-------------------------------------------------------------------------------

1.1 Re-enable OTEL logs features

Current Cargo.toml has logs features removed. Re-enable them as follows:

```toml
[dependencies]
opentelemetry = { version = "0.30.0", optional = true, features = ["logs"] }
opentelemetry_sdk = { version = "0.30.0", features = ["rt-tokio", "logs"], optional = true }
opentelemetry-otlp = { version = "0.30.0", features = ["grpc-tonic", "http-proto", "reqwest-rustls", "logs"], optional = true }
```

Notes:
- `opentelemetry` `logs` feature is needed for `opentelemetry::logs::Severity`.
- `opentelemetry_sdk` `logs` feature is needed for `LoggerProviderBuilder`, `SdkLogger`, `SdkLogRecord`, `LogProcessor`.
- `opentelemetry-otlp` `logs` feature is needed for the OTLP logs exporter builder.

1.2 Features wiring

Keep existing features unchanged; logs bridge is only functional when `otel-otlp` is enabled:

```toml
[features]
otel = [
    "tracing",
    "tracing-subscriber",
    "opentelemetry",
    "opentelemetry_sdk",
    "tracing-opentelemetry",
    "opentelemetry-stdout",
    "hostname",
    "humantime",
]
otel-otlp = [
    "otel",
    "opentelemetry-otlp",
    "tokio",
]
```

Implications:
- In builds without `otel`, telemetry code is not used at all (as today).
- In builds with `otel` but without `otel-otlp`, traces + dev metrics work, but OTLP logs exporter is not available; the logs bridge will not be constructed.
- In builds with `otel-otlp`, traces, metrics, and logs are available if an OTLP endpoint is configured.


-------------------------------------------------------------------------------
2. Environment controls and default behavior
-------------------------------------------------------------------------------

2.1 Overall telemetry enablement (unchanged)

`telemetry_enabled_env()` already implements:

- If `AIFO_CODER_OTEL` is set to `"1"`, `"true"`, `"yes"` (case-insensitive): telemetry enabled.
- If `AIFO_CODER_OTEL` is set to `"0"`, `"false"`, `"no"`, `"off"`: telemetry disabled.
- If `AIFO_CODER_OTEL` is unset or any other value: telemetry enabled (default).

This behavior remains unchanged.

2.2 Logs bridge enablement

`telemetry_logs_enabled_env()` currently:

```rust
fn telemetry_logs_enabled_env() -> bool {
    match env::var("AIFO_CODER_OTEL_LOGS") {
        Ok(v) => {
            let v = v.trim().to_ascii_lowercase();
            !(v == "0" || v == "false" || v == "no" || v == "off")
        }
        Err(_) => true,
    }
}
```

We keep this semantics and use it in **two** places:

- When deciding whether to build the logger provider (pipeline) at init time.
- As a cheap runtime guard inside the logging layer’s `on_event`.

Effective behavior:

- `AIFO_CODER_OTEL_LOGS` unset:
  - logs bridge is *eligible* to be enabled.
- `AIFO_CODER_OTEL_LOGS` set to `"0"`, `"false"`, `"no"`, `"off"`:
  - logs bridge is disabled even if OTLP endpoint exists.
- Any other value: treated as enabled (same as unset).

Additional constraints:
- Logs bridge only runs if:
  - `telemetry_enabled_env()` is true.
  - `cfg!(feature = "otel-otlp")` is true at build time.
  - `effective_otlp_endpoint()` returns Some endpoint.
  - OTLP log exporter builds successfully.

2.3 Telemetry verbosity

`verbose_otel_enabled()` already checks `AIFO_CODER_OTEL_VERBOSE`. We reuse it:

- When building logs exporter/provider, any error is logged via `log_warn_stderr` only if `verbose_otel_enabled()` is true.
- No additional stderr output is produced otherwise.

This keeps logs bridge consistent with the metrics exporter’s logging style.

2.4 Flood protection

To avoid collector flooding:

- The logs bridge only exports events with `tracing::Level` >= `INFO`:
  - `ERROR`, `WARN`, `INFO` exported.
  - `DEBUG` and `TRACE` ignored, regardless of `RUST_LOG` or fmt settings.
- This filter is enforced in the `OtelLogLayer::on_event` method, independent from any `EnvFilter` used for the fmt layer.

We do **not** support env overrides for this behavior in v3; this matches README wording and keeps complexity down.

2.5 Transport

The existing telemetry code currently fixes:

```rust
fn otel_transport() -> OtelTransport {
    // Force HTTP/HTTPS transport; ignore any grpc requests.
    OtelTransport::Http
}
```

For logs bridge in v3:

- We always use HTTP/HTTPS log exporter (`LogExporter::builder().with_http()`).
- We do **not** implement a gRPC logs transport in this version, even if `AIFO_OTEL_DEFAULT_TRANSPORT` is `"grpc"`.
- This matches metrics behavior (they also use HttpExporterBuilder + HTTP).

The plan must acknowledge this: README references to "transport" remain correct conceptually, but logs/metrics/traces are all using HTTP in v3 implementation.


-------------------------------------------------------------------------------
3. Logger provider and OTLP logs exporter
-------------------------------------------------------------------------------

3.1 Build logger provider

Add to `src/telemetry.rs`, behind `#[cfg(feature = "otel-otlp")]`:

Imports to ensure:

```rust
#[cfg(feature = "otel-otlp")]
use opentelemetry_sdk::logs::{
    LoggerProvider as SdkLoggerProvider,
    LoggerProviderBuilder,
};
#[cfg(feature = "otel-otlp")]
use opentelemetry::logs::Severity;
```

Function:

```rust
#[cfg(feature = "otel-otlp")]
fn build_logger_provider(use_otlp: bool) -> Option<SdkLoggerProvider> {
    if !use_otlp {
        return None;
    }

    let endpoint = effective_otlp_endpoint()?;

    let exporter = match opentelemetry_otlp::LogExporter::builder()
        .with_http()
        .with_endpoint(endpoint)
        .build()
    {
        Ok(exp) => exp,
        Err(e) => {
            if verbose_otel_enabled() {
                let use_err = crate::color_enabled_stderr();
                crate::log_warn_stderr(
                    use_err,
                    &format!(
                        "aifo-coder: telemetry: failed to build OTLP log exporter: {}",
                        e
                    ),
                );
            }
            return None;
        }
    };

    let provider = LoggerProviderBuilder::default()
        .with_resource(build_resource())
        .with_batch_exporter(exporter)
        .build();

    Some(provider)
}
```

Notes:
- Uses `with_batch_exporter` to avoid per-event network calls.
- Re-uses `build_resource()` so logs share `service.name`, `service.version`, `service.instance.id` with traces/metrics.
- On exporter creation failure, logs bridge is disabled; no panic or hard failure.

3.2 TelemetryGuard structure

Extend `TelemetryGuard` in `src/telemetry.rs`:

```rust
pub struct TelemetryGuard {
    meter_provider: Option<SdkMeterProvider>,
    #[cfg(feature = "otel-otlp")]
    runtime: Option<tokio::runtime::Runtime>,
    #[cfg(feature = "otel-otlp")]
    log_provider: Option<SdkLoggerProvider>,
}
```

Ensure imports are present:

```rust
use opentelemetry_sdk::metrics::SdkMeterProvider;
#[cfg(feature = "otel-otlp")]
use opentelemetry_sdk::logs::LoggerProvider as SdkLoggerProvider;
```

(Adjust imports minimally to avoid unused warnings.)

3.3 TelemetryGuard Drop (logs flush)

Update `Drop` impl:

```rust
impl Drop for TelemetryGuard {
    fn drop(&mut self) {
        if let Some(ref mp) = self.meter_provider {
            let _ = mp.force_flush();
        }
        #[cfg(feature = "otel-otlp")]
        {
            if let Some(ref lp) = self.log_provider {
                let _ = lp.force_flush();
            }
            if let Some(rt) = self.runtime.take() {
                drop(rt);
            }
        }
    }
}
```

- We use the provider-level `force_flush()`; any errors are ignored (consistent with metrics).
- No attempt to call `shutdown()` (telemetry is best-effort; process exit is more important).

Potential subtlety:
- If `SdkLoggerProvider` doesn’t expose `force_flush()` in the version used, we must adapt; but `0.30.0` does expose it in `opentelemetry_sdk::logs::SdkLoggerProvider`. Implementation must verify this and adjust accordingly if signature differs (e.g., `fn force_flush(&self) -> OTelSdkResult`).

If compiler complains, adjust Drop to:

```rust
if let Some(ref lp) = self.log_provider {
    let _ = lp.shutdown();
}
```

but `force_flush` is preferred; this spec assumes `force_flush` is available.


-------------------------------------------------------------------------------
4. Tracing layer: bridging events to OTEL logs
-------------------------------------------------------------------------------

4.1 OtelLogLayer struct and constructor

Add to `src/telemetry.rs`:

```rust
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
```

Imports needed:

```rust
#[cfg(feature = "otel-otlp")]
use opentelemetry_sdk::logs::SdkLogger;
```

(Or reference the fully qualified type directly where needed to avoid extra imports.)

4.2 Layer implementation

Implement `Layer` for `OtelLogLayer`:

```rust
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
        // Respect logs env toggle quickly.
        if !telemetry_logs_enabled_env() {
            return;
        }

        let meta = event.metadata();
        let level = *meta.level();

        // Flood control: only INFO/WARN/ERROR events are exported to OTEL logs.
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

        // Simple body rendering: debug representation of the event.
        let mut buf = String::new();
        use std::fmt::Write as _;
        let _ = write!(&mut buf, "{:?}", event);

        let mut record = opentelemetry_sdk::logs::SdkLogRecord::new(severity);
        record.set_body(buf.into());
        record.add_attribute(KeyValue::new("logger.name", meta.target().to_string()));
        record.add_attribute(KeyValue::new(
            "logger.level",
            meta.level().as_str().to_string(),
        ));

        let _ = self.logger.emit(record);
    }
}
```

Notes and constraints:

- No PII-specific env gating is implemented here; we treat log bodies similarly to typical log messages. If future requirements demand PII-safe logs, we can add env-based redaction later.
- The layer does not depend on context (`_ctx`) or spans; we intentionally avoid complex lookups for now.
- It is safe if the layer is constructed but no events occur; `logger.emit()` is not called.
- If the provider had no exporters (e.g., exporter failed to build and `build_logger_provider` returned `None`), then this layer is simply not attached; we do not attach a layer with a dummy logger.


-------------------------------------------------------------------------------
5. Wiring logs into telemetry_init
-------------------------------------------------------------------------------

5.1 Construct log_provider

Within `telemetry_init()` (near where metrics provider is built), add:

```rust
#[cfg(feature = "otel-otlp")]
let log_provider = if use_otlp && telemetry_logs_enabled_env() {
    build_logger_provider(use_otlp)
} else {
    None
};
```

Important:

- `use_otlp` is already computed as `cfg!(feature = "otel-otlp") && effective_otlp_endpoint().is_some()`.
  - If `effective_otlp_endpoint()` is None, no logs provider is built.
- Even if `telemetry_logs_enabled_env() == true`, `build_logger_provider` can still return `None` (e.g., exporter build error); in that case, logs bridge is disabled.

5.2 Attach OtelLogLayer to subscriber only when provider exists

Currently we have:

```rust
global::set_tracer_provider(tracer_provider);
let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

let base_subscriber = tracing_subscriber::registry().with(otel_layer);
let base_subscriber = base_subscriber;
```

Change this to:

```rust
global::set_tracer_provider(tracer_provider);
let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

let mut base_subscriber = tracing_subscriber::registry().with(otel_layer);

#[cfg(feature = "otel-otlp")]
if let Some(ref lp) = log_provider {
    let log_layer = OtelLogLayer::new(lp);
    base_subscriber = base_subscriber.with(log_layer);
}

// Base subscriber: registry + OTEL trace + optional OTEL logs layers.
let base_subscriber = base_subscriber;
```

The rest of `telemetry_init()` (fmt layer logic, try_init, INIT OnceCell) remains unchanged and operates on `base_subscriber`.

5.3 TelemetryGuard construction

At the end of `telemetry_init()`:

```rust
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
```

This ensures `TelemetryGuard` always consistently reflects what was created during init.

5.4 No change to metrics logic

Ensure the metrics path is untouched except for possible import adjustments:

- `build_metrics_provider_with_status` remains as-is.
- Metrics env `AIFO_CODER_OTEL_METRICS` semantics remain unchanged.


-------------------------------------------------------------------------------
6. Documentation and comments
-------------------------------------------------------------------------------

6.1 Code comments

Replace or update any comments in `src/telemetry.rs` referring to:

> logs bridge is not yet implemented in v1

with a short note such as:

> // Base subscriber: registry + OTEL trace + logs layers (when enabled) and optional fmt layer.

6.2 README-opentelemetry.md

The README already describes:

- Logs behavior in “2.3 OTEL logs”.
- Flood control (INFO/WARN/ERROR only).
- Env `AIFO_CODER_OTEL_LOGS`.

There is no required behavioral change to README, but ensure the new behavior matches:

- Logs bridge is present only in `otel-otlp` builds.
- Uses HTTP OTLP endpoint (HTTP/HTTPS), consistent with how metrics are described.

If any README lines still suggest that logs bridge is “not yet implemented”, they must be updated in a follow-up docs-only change (out of scope for this spec, which focuses on code behavior).


-------------------------------------------------------------------------------
7. Validation notes (non-normative)
-------------------------------------------------------------------------------

After implementation, validate:

- Build:
  - `cargo build --features otel-otlp`
- Telemetry disabled:
  - `AIFO_CODER_OTEL=0 cargo run --features otel-otlp -- --help`
    - Should behave as without telemetry; no panics, stdout unchanged.
- Logs disabled:
  - `AIFO_CODER_OTEL=1 AIFO_CODER_OTEL_LOGS=0 cargo run --features otel-otlp -- --help`
    - Logs bridge not attached; no exporter warnings; stdout unchanged.
- Logs enabled, no endpoint:
  - `AIFO_CODER_OTEL=1 AIFO_CODER_OTEL_LOGS=1 cargo run --features otel-otlp -- --help`
    - Because `effective_otlp_endpoint()` still returns a fallback (`https://localhost:4318`), exporter should attempt to build. If it fails (e.g., network issues), it must log only in verbose mode, but the CLI must still succeed.
- Logs + verbose:
  - `AIFO_CODER_OTEL=1 AIFO_CODER_OTEL_LOGS=1 AIFO_CODER_OTEL_VERBOSE=1 cargo run --features otel-otlp -- --help`
    - On exporter failure, a concise warning appears on stderr but stdout remains identical to the non-otel run.

The existing golden stdout test (`ci/otel-golden-stdout.sh`) must continue to pass after this change.

End of spec.
