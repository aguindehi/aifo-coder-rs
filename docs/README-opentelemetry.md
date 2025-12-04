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
  - Extends `otel` with OTLP HTTP exporter support (via `opentelemetry-otlp` with `http-proto`).
  - Uses a PeriodicReader for metrics when an endpoint is configured; no dedicated Tokio runtime is required for traces.

Example builds:

```bash
cargo build --features otel
cargo build --features otel-otlp
```

When neither feature is enabled, all telemetry code compiles out and `telemetry_init()` is a no-op.

## 2. Runtime enablement

Telemetry is controlled purely via environment variables; no new CLI flags are added.

Enablement rules (when built with `--features otel`):

- Default: telemetry is enabled when `AIFO_CODER_OTEL` is unset.
- Disable by setting `AIFO_CODER_OTEL=0`, `false`, `no` or `off`.
- `OTEL_EXPORTER_OTLP_ENDPOINT` (non-empty) selects the OTLP endpoint (HTTP/HTTPS); it does not by itself change enablement.
- CLI `--verbose` sets `AIFO_CODER_OTEL_VERBOSE=1` to print concise initialization info on stderr.

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
OTEL_EXPORTER_OTLP_ENDPOINT=https://localhost:4318 \
cargo run --features otel-otlp -- --help
```

Notes:

- The exporter uses OTLP over HTTP/HTTPS and respects `OTEL_EXPORTER_OTLP_TIMEOUT` (default 5s) and
  `OTEL_BSP_*` batch settings.
- A PeriodicReader is used for metrics export; no dedicated Tokio runtime is required for traces.

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

Metrics are enabled by default when telemetry is enabled. Disable with `AIFO_CODER_OTEL_METRICS=0`, `false`, `no` or `off`.

Environment variables:

- `AIFO_CODER_OTEL_METRICS`
  - Controls metrics instruments/exporter (default enabled).

Example (dev exporters; traces to stderr, metrics to stderr/file):

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
   - Runs `AIFO_CODER_OTEL=0 cargo run --quiet --features otel -- --help` (baseline, telemetry disabled).
   - Runs `cargo run --quiet --features otel -- --help` (telemetry enabled by default).
   - Fails if stdout differs between the two runs.
3. Smoke run with metrics enabled:
   - `AIFO_CODER_OTEL_METRICS=1 cargo run --features otel -- --help`.

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
    - OTEL_EXPORTER_OTLP_ENDPOINT=https://otel-collector:4318 \
      cargo run --features otel-otlp -- --help
```

## 7. Troubleshooting

- **Build fails with otel features**  
  Run `cargo clean` and rebuild with:
  ```bash
  cargo build --features otel
  ```

- **No traces appear in local collector**  
  - Ensure telemetry is not disabled (`AIFO_CODER_OTEL` unset or set to `1`).
  - Set `OTEL_EXPORTER_OTLP_ENDPOINT` (HTTP/HTTPS, e.g., `https://localhost:4318`).
  - Ensure network connectivity from the host to the collector.
  - For local visibility without an endpoint, enable the fmt layer (`AIFO_CODER_TRACING_FMT=1` and `RUST_LOG`).

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

## 8. Test plan and coverage

Goal: validate that enabling telemetry never changes CLI stdout or exit codes, and that core telemetry
paths initialize cleanly, are idempotent, and safe when disabled. Cover unit, integration (no Docker
assumptions) and optional e2e (Docker-required) scenarios.

Scope notes:
- CI “unit/integration” jobs run with AIFO_CODER_TEST_DISABLE_DOCKER=1 (no Docker).
- e2e tests that need Docker must be marked #[ignore] and will run only in Docker-enabled lanes.

### 8.1 Unit tests (crate-level invariants; no Docker required)

Focus:
- Idempotence of telemetry_init(): subsequent calls are no-ops.
- Safe disablement via AIFO_CODER_OTEL=0 (returns None; no panics).
- Optional fmt layer: AIFO_CODER_TRACING_FMT=1 must not panic and must not affect stdout.

Implementation details:
- tests/otel_basic_tests.rs:
  - otel_idempotent_second_none: call telemetry_init() twice; assert second call returns None.
  - otel_disabled_env_returns_none_both_calls: set AIFO_CODER_OTEL=0; call telemetry_init() twice and assert None for both.
  - otel_fmt_layer_no_panic: set AIFO_CODER_TRACING_FMT=1 + RUST_LOG=warn; call telemetry_init(); assert no panic.

These tests are agnostic to feature flags: with otel disabled at compile-time, telemetry_init() is a stub returning Option<()>;
with otel enabled, it returns Option<TelemetryGuard>. In both cases the invariants above hold.

### 8.2 Integration tests (feature-enabled builds; no external collector required)

Focus:
- Golden stdout for --help regardless of AIFO_CODER_OTEL setting (already covered by script).
- Smoke run with metrics enabled but without OTLP endpoint (dev exporters or no-ops).

Implementation details:
- Script ci/otel-golden-stdout.sh (kept source-controlled):
  - cargo build --features otel
  - Compare stdout of: “cargo run --features otel -- --help” vs “AIFO_CODER_OTEL=0 cargo run --features otel -- --help”
    and fail on diff.
  - Smoke run with metrics enabled: AIFO_CODER_OTEL_METRICS=1 cargo run --features otel -- --help

Notes:
- The default Makefile uses CARGO_FLAGS ?= --features otel-otlp; CI executes tests under otel-otlp by default.

### 8.3 Optional e2e tests (Docker-required; ignored by default)

Focus (optional future extension):
- Start the tool-exec proxy, invoke a simple tool via sidecar, and verify end-to-end streaming with a TRACEPARENT value injected.
- Validate that disconnect handling returns the canonical drop line and escalates signals according to policy.

Notes:
- These tests would live under tests/e2e_otel_*.rs and be annotated with #[ignore].
- They should be enabled only in a Docker-capable lane (e.g., test-e2e job family).

### 8.4 Running locally

- Unit/integration tests (no Docker):
  - make test
  - or: cargo nextest run
- Golden stdout and smoke (no Docker required):
  - ci/otel-golden-stdout.sh

### 8.5 Consistency and gap analysis

- Stdout invariants are enforced by a golden test script and do not rely on log capture.
- Idempotence and environment toggles are validated in-process without relying on private APIs.
- Metrics API is intentionally not depended upon directly by tests to avoid leaking non-public symbols;
  behavior is validated indirectly via “no panic” guarantees in otel and no-otel builds.
- OTLP endpoint and transport selection are exercised implicitly by initialization (no exporter hard-fail).
- e2e coverage is intentionally optional and isolated (ignored) to keep the default test suite portable.

This test plan keeps default runs fast and hermetic while providing a clear path to opt-in e2e validation.
