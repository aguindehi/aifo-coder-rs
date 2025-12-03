# OpenTelemetry support in aifo-coder

This document describes how to build, enable and test the OpenTelemetry integration for
`aifo-coder`. Telemetry is:

- **compile-time optional** for developers (via Cargo features), and
- **enabled by default at runtime** in feature-enabled builds when `AIFO_CODER_OTEL` is unset.

Telemetry must not change the default CLI stdout or exit codes.

## 1. Build-time features

Telemetry is guarded by two Cargo features, both disabled by default:

- `otel`
  - Enables tracing and local development exporters (stderr/file sinks).
  - Pulls in `tracing`, `tracing-subscriber`, `opentelemetry`, `opentelemetry_sdk`,
    `tracing-opentelemetry`, and `opentelemetry-stdout`.
- `otel-otlp`
  - Extends `otel` with OTLP/gRPC export via `opentelemetry-otlp` and a private Tokio runtime.

Example builds:

```bash
cargo build --features otel
cargo build --features otel-otlp
```

When neither feature is enabled, all telemetry code compiles out and `telemetry_init()` is a no-op.

## 2. Runtime enablement

Telemetry is controlled purely via environment variables; no new CLI flags are added.

Enablement rules (when built with `--features otel`):

- `AIFO_CODER_OTEL=1`
  - Enables telemetry initialization.
- `OTEL_EXPORTER_OTLP_ENDPOINT` (non-empty)
  - Also enables telemetry; enables OTLP path when `otel-otlp` is compiled.
- If both are unset/empty, telemetry initialization returns early without installing any
  tracing subscriber or exporters.

Basic usage examples:

```bash
# Traces to stderr via stdout exporter (no fmt layer, no extra logs)
AIFO_CODER_OTEL=1 \
cargo run --features otel -- --help

# Traces with fmt layer enabled (logs on stderr; RUST_LOG respected)
AIFO_CODER_OTEL=1 \
AIFO_CODER_TRACING_FMT=1 \
RUST_LOG=info \
cargo run --features otel -- --help
```

### 2.1 OTLP exporter

When compiled with `--features otel-otlp` and `OTEL_EXPORTER_OTLP_ENDPOINT` is set:

```bash
AIFO_CODER_OTEL=1 \
OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317 \
cargo run --features otel-otlp -- --help
```

Notes:

- The exporter uses tonic gRPC and respects `OTEL_EXPORTER_OTLP_TIMEOUT` (default 5s) and
  `OTEL_BSP_*` batch settings.
- A private multi-threaded Tokio runtime is created for exporting spans/metrics; it is shut down
  cleanly via `TelemetryGuard::drop`.

## 3. Logging and fmt layer

By default, the OpenTelemetry integration only installs a `tracing_opentelemetry` layer bound to
the tracer; it does not install `fmt` logging.

To opt into stderr logs via `tracing-subscriber::fmt`:

- Set `AIFO_CODER_TRACING_FMT=1`.
- Optionally set `RUST_LOG` (default filter is `warn`).

Example:

```bash
AIFO_CODER_OTEL=1 \
AIFO_CODER_TRACING_FMT=1 \
RUST_LOG=aifo_coder=info \
cargo run --features otel -- --help
```

Without `AIFO_CODER_TRACING_FMT=1`, the fmt layer is not installed and `RUST_LOG` has no
user-visible effect.

## 4. Metrics

Metrics are opt-in at runtime and disabled by default.

Environment variables:

- `AIFO_CODER_OTEL_METRICS`
  - `"1"` enables metrics exporter and instruments.
- `AIFO_CODER_OTEL_METRICS_FILE`
  - Optional file path for the development metrics sink when stdout metrics cannot be directed
    to stderr. Default:
    `${XDG_RUNTIME_DIR:-/tmp}/aifo-coder.otel.metrics.jsonl`.

Example (stdout dev exporters; traces to stderr, metrics to stderr or file):

```bash
AIFO_CODER_OTEL=1 \
AIFO_CODER_OTEL_METRICS=1 \
cargo run --features otel -- --help
```

When `otel-otlp` is enabled and `OTEL_EXPORTER_OTLP_ENDPOINT` is set, metrics are exported via
OTLP with a `PeriodicReader` (interval ~2s).

## 5. Privacy and PII

By default, telemetry avoids recording raw paths or arguments:

- `AIFO_CODER_OTEL_PII` controls whether PII-rich fields are allowed.
  - Default `"0"` (or unset): record only counts and salted hashes for sensitive values.
  - `"1"`: record raw strings for debugging; never enable this in production.

Implementation details:

- A per-process 64-bit FNV-1a hash with a salt derived from PID and start time is used via
  `telemetry::hash_string_hex`.
- HTTP headers containing secrets (e.g., `OTEL_EXPORTER_OTLP_HEADERS`) are never logged.

## 6. CI: otel build + golden stdout test

The repository provides a small CI helper script:

- `ci/otel-golden-stdout.sh`

It performs:

1. `cargo build --features otel`
2. Golden stdout test:
   - Runs `cargo run --quiet -- --help` (baseline).
   - Runs `AIFO_CODER_OTEL=1 cargo run --quiet --features otel -- --help`.
   - Fails if stdout differs between the two runs.
3. Smoke run with metrics enabled:
   - `AIFO_CODER_OTEL=1 AIFO_CODER_OTEL_METRICS=1 cargo run --features otel -- --help`.

Example CI job snippet (pseudo YAML):

```yaml
otel-golden:
  stage: test
  script:
    - ci/otel-golden-stdout.sh
```

This job ensures:

- The crate builds successfully with `--features otel`.
- Enabling telemetry does not change the CLI stdout for a short run like `--help`.
- A metrics-enabled run succeeds without panics and with proper shutdown/flush.

An optional OTLP CI job can be added if a collector is available, e.g.:

```yaml
otel-otlp-smoke:
  stage: test
  script:
    - AIFO_CODER_OTEL=1 OTEL_EXPORTER_OTLP_ENDPOINT=http://otel-collector:4317 \
      cargo run --features otel-otlp -- --help
```

## 7. Troubleshooting

- **Build fails with otel features**  
  Run `cargo clean` and rebuild with:
  ```bash
  cargo build --features otel
  ```

- **No traces appear in local collector**  
  - Confirm `AIFO_CODER_OTEL=1` and `OTEL_EXPORTER_OTLP_ENDPOINT` are set.
  - Ensure network connectivity from the host to the collector.
  - Check for concise warnings on stderr about exporter initialization.

- **Unexpected stderr logs**  
  - Ensure `AIFO_CODER_TRACING_FMT` is not set (or set to `"0"`).
  - By default, fmt is not installed and no additional stderr output is produced.

- **Metrics not exported**  
  - Check `AIFO_CODER_OTEL_METRICS=1`.
  - For OTLP metrics, verify `OTEL_EXPORTER_OTLP_ENDPOINT` and collector configuration.
  - For dev metrics, inspect the JSONL file under
    `${AIFO_CODER_OTEL_METRICS_FILE}` or the default runtime path.

This document, together with `ci/otel-golden-stdout.sh`, completes Phase 6 by providing a
repeatable CI check for otel builds and a clear reference for enabling, tuning and troubleshooting
OpenTelemetry in `aifo-coder`.
