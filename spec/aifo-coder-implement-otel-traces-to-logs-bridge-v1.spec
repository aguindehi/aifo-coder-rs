Title: Implement an OpenTelemetry logs bridge design (v1) for aifo-coder

Version: 1
Status: Draft (design only; no-op implementation in current branch)

Goal
====

Define a safe-by-default design for an OpenTelemetry log pipeline that will eventually export
selected `tracing` events as OTLP HTTP log records alongside existing traces and metrics in
builds with `otel-otlp`.

For the **current branch**, logs export remains a no-op: we do not emit OTEL logs yet, but we
prepare configuration (env toggle, docs) and avoid any code that does not compile or that relies
on unstable / non-public SDK APIs.

Constraints
===========

- Must compile with the current dependency versions:

  - opentelemetry = 0.30.0
  - opentelemetry_sdk = 0.30.0
  - opentelemetry-otlp = 0.30.0

- Must NOT rely on non-existent APIs such as:
  - `opentelemetry::global::logger_provider()`
  - public constructors or builders that do not exist in 0.30.0 for logs (e.g., non-public `SdkLogRecord::new`).
  - crates not yet wired into this project (e.g., `opentelemetry-appender-tracing`).

- Must not break or materially change:
  - Existing trace initialization and behavior.
  - Existing metrics initialization and behavior.
  - CLI stdout or exit codes.

- Flood control (for future implementation):
  - OTEL logs must be low-volume by default.
  - There must be an explicit opt-out environment variable for logs.
  - Logs must respect the existing `RUST_LOG` and `AIFO_CODER_TRACING_FMT` behavior.


Out of Scope (v1 branch)
========================

- Actual emission of OTEL log records from `tracing` events.
- Changes to public CLI options.
- Changes to existing span names, attributes, or metrics (beyond already completed work).
- Collector configuration; we only send traces/metrics to the existing OTLP endpoint.
- Advanced log features (e.g., structured fields from all event fields, batch processors).

The actual log export implementation is deferred to a **future PR**, which can:

- Upgrade OTEL crates to versions with better log builder support, or
- Introduce a dedicated log bridge crate such as `opentelemetry-appender-tracing`, or
- Implement a custom `LogProcessor` as per the SDK docs once we commit to a specific API.


High-Level Design (Future Logs Export)
======================================

1. Build-time:

   - Once we are ready to implement logs, we may enable the `logs` feature on `opentelemetry`,
     `opentelemetry_sdk`, and `opentelemetry-otlp` via `Cargo.toml`, or upgrade to a newer version
     with stable log APIs and builders.
   - Reuse the existing `otel` / `otel-otlp` Cargo features; do not introduce new Cargo features
     specifically for logs.

2. Runtime (future implementation):

   - When telemetry is enabled (`AIFO_CODER_OTEL` not set to off) and an OTLP endpoint is in use,
     and when an env toggle `AIFO_CODER_OTEL_LOGS` does not explicitly disable logs, we will:

       - Build a local `SdkLoggerProvider` with:

         - Resource from `build_resource()`.
         - A log processor chain (e.g., `SimpleLogProcessor` or `BatchLogProcessor`) wired to
           an OTLP HTTP log exporter.

       - Integrate `tracing` events into OTEL logs, either via:

         - A dedicated log bridge crate (e.g., `opentelemetry-appender-tracing`), or
         - A custom `LogProcessor` that handles `SdkLogRecord` as described in the
           `opentelemetry_sdk::logs::LogProcessor` documentation.

   - For the current branch, this integration does **not** exist; any `OtelLogLayer::on_event`
     implementation remains a no-op to avoid touching internal SDK APIs.

3. Flood control (future behavior):

   - The log pipeline will:

     - Export only events with `level >= INFO` to OTEL logs (even if `RUST_LOG` allows more).
     - Respect `RUST_LOG` and `AIFO_CODER_TRACING_FMT` for which events are emitted at all.

   - Default behavior will keep log volume low (WARN/ERROR unless explicitly widened).

4. Guard and lifecycle:

   - A future `TelemetryGuard` may own a `SdkLoggerProvider` when OTLP logs are active, so the
     provider lives at least as long as the tracing subscriber.
   - For v1, we do not instantiate or manage a logger provider at all; the design is documented
     but not implemented, to keep the code compiling and focused on traces/metrics.


Current Branch Behavior
=======================

- OTEL logs are **not exported**; any log-related wiring is a no-op.
- Environment and documentation preparation:

  - `AIFO_CODER_OTEL_LOGS` is defined and documented as a toggle for future logs export.
  - Docs describe intended logs behavior (INFO+ events, same OTLP endpoint, opt-out env),
    but clearly note that logs export is not yet active in this version.

- Existing behavior (already implemented and working):

  - Traces via `tracing-opentelemetry` and `SdkTracerProvider`.
  - Metrics via `SdkMeterProvider` and OTLP HTTP exporter (or dev exporter).
  - CI-based baked-in OTLP endpoint and transport defaults.
  - OTEL naming conventions (`aifo_coder_*` names and attributes) for metrics and spans.


Plan for Future OTEL Logs Implementation
========================================

A future PR that wants to implement actual logs export should:

1. Reassess OTEL crate versions
   - Decide whether to stay on `0.30.x` with experimental logs, or upgrade to a newer OTEL
     version where log builders and exporters are more stable and better documented.

2. Choose integration strategy
   - Option A: Use `opentelemetry-appender-tracing` (or equivalent) to turn `tracing` events
     into OTEL logs using a supported API.
   - Option B: Implement a custom `LogProcessor` that:

     - Implements `opentelemetry_sdk::logs::LogProcessor`.
     - Uses `emit(&self, data: &mut SdkLogRecord, instrumentation: &InstrumentationScope)` to
       inspect and filter logs (e.g., severity cutoff, additional attributes).
     - Wraps or delegates to a downstream `LogExporter` for OTLP.

3. Wire provider and processor
   - Build `SdkLoggerProvider` with:

     - Resource = `build_resource()`.
     - One or more `LogProcessor`s configured with an OTLP log exporter.

   - Keep all of this local; avoid global logger APIs that do not exist in the crate versions used.

4. Connect tracing to logs
   - Once a supported bridge mechanism is chosen, wire `tracing` events into logs via the
     chosen approach (bridge crate or processor).

5. Maintain flood control & env toggles
   - Respect `AIFO_CODER_OTEL_LOGS` as an opt-out.
   - Respect `RUST_LOG` and `AIFO_CODER_TRACING_FMT` as volume control.
   - Default to low-volume exports (WARN/ERROR, optionally INFO) unless explicitly widened.


Test & CI Considerations
========================

- The existing golden stdout tests (`ci/telemetry-smoke.sh`) and unit tests remain unchanged.
- When logs export is eventually added:
  - Ensure that enabling OTEL logs does not change CLI stdout or exit codes.
  - Any failures in log export must be best-effort and must not fail commands.
  - Consider adding a small smoke test that verifies `telemetry_init()` succeeds with logs
    enabled and that log export failures do not panic.

End of updated v1 spec.
