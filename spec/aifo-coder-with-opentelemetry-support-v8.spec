Title: OpenTelemetry support for aifo-coder (v8)
Status: Proposed
Owner: aifo-coder maintainers
Last-Updated: 2025-09-03

Overview

This v8 specification consolidates and refines v6 and v7 into a single, production-ready plan with explicit safeguards and acceptance criteria:
- stderr-only development exporters (traces and metrics), never stdout
- strict opt-in for fmt logging layer (no surprise stderr logs)
- reliable flush for short-lived CLI runs (traces and metrics)
- explicit, stable runtime enablement and fallback rules
- explicit W3C TraceContext propagator setup only (no Baggage by default)
- error status propagation on spans (via OpenTelemetry span extensions)
- metrics with clear units, cumulative temporality, and low cardinality
- idempotent initialization and safe shutdown (no panics, no exit code changes)
- cohesive phased delivery, testing guidance, and golden stdout acceptance criteria
- developer commands section reinstated for contributor ergonomics

Telemetry is fully optional (compile- and runtime-gated) and must not alter CLI stdout, default stderr messages, or exit codes.

1) Goals

- Add optional, opt-in OpenTelemetry tracing and metrics with zero changes to default CLI UX or tests.
- Provide low-overhead spans across high-value operations (docker orchestration, toolchain sidecars, proxy requests, registry probing, lock acquisition).
- Include metrics (counters, histograms) behind a runtime gate; disabled by default.
- Support local development with “stdout” exporters routed to stderr or a safe file sink; support production export via OTLP/gRPC.
- Enforce privacy-by-default (salted hashes and counts) with an explicit, unsafe debugging override for PII.

2) Non-goals

- Telemetry is never mandatory; default build compiles it out.
- No new CLI flags; runtime control via environment variables only.
- No changes to user-facing stdout/stderr by default; telemetry must never write to stdout.
- No third-party span linking beyond in-process and (later) aifo-shim boundary propagation.

3) Build and feature gating (Cargo)

Features (off by default):
- otel: enables tracing and OpenTelemetry (stdout exporters for traces and metrics).
- otel-otlp: extends otel with OTLP exporter and a private Tokio runtime dependency.

Dependencies in Cargo.toml (all optional = true; versions pinned):
- tracing = { version = "0.1", features = ["std"], optional = true }
- tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt"], optional = true }
- opentelemetry = { version = "0.24", optional = true }
- opentelemetry_sdk = { version = "0.24", optional = true }
- tracing-opentelemetry = { version = "0.25", optional = true }
- opentelemetry-stdout = { version = "0.4", optional = true }
- opentelemetry-otlp = { version = "0.17", features = ["grpc-tonic"], optional = true }  # only with otel-otlp
- tokio = { version = "1", features = ["rt-multi-thread"], optional = true }              # only with otel-otlp
- once_cell = "1"  # idempotent initialization

Features section:
- otel = ["tracing", "tracing-subscriber", "opentelemetry", "opentelemetry_sdk", "tracing-opentelemetry", "opentelemetry-stdout"]
- otel-otlp = ["otel", "opentelemetry-otlp", "tokio"]

Default build: no features; telemetry code compiles out.

4) Runtime configuration (environment)

Enablement (explicit and stable):
- AIFO_CODER_OTEL=1
  - Enables telemetry initialization.
- OTEL_EXPORTER_OTLP_ENDPOINT
  - If set and non-empty, telemetry is enabled as well (even if AIFO_CODER_OTEL is not set).
- If both unset/empty: telemetry_init() is a no-op even when compiled with features.

Exporters:
- OTEL_EXPORTER_OTLP_ENDPOINT
  - Endpoint for OTLP/gRPC via tonic (http://collector:4317 or https://collector.example.com:4317).
- OTEL_EXPORTER_OTLP_HEADERS
  - Optional auth headers (e.g., "authorization=Bearer abc,another=val"). Never log these values.
- OTEL_EXPORTER_OTLP_TIMEOUT
  - Request timeout (e.g., "5s"). Default 5s if unset; respected by exporter configuration.

Sampling and logging:
- OTEL_TRACES_SAMPLER / OTEL_TRACES_SAMPLER_ARG
  - Respect standard sampler configuration (default parentbased_always_on).
  - Recommended for busy dev flows: OTEL_TRACES_SAMPLER=parentbased_traceidratio and OTEL_TRACES_SAMPLER_ARG=0.1.
- AIFO_CODER_TRACING_FMT=1
  - Strict opt-in to install fmt layer so tracing events appear on stderr. Without this flag, fmt is NOT installed.
- RUST_LOG
  - Used only as an EnvFilter when fmt is installed. If fmt is not installed, RUST_LOG has no effect on user-visible logs.

PII handling:
- AIFO_CODER_OTEL_PII
  - Default "0": redact PII (paths/args). When "1", include raw cwd and args in spans (unsafe debugging; never for production).

Metrics:
- AIFO_CODER_OTEL_METRICS
  - Default "0": metrics disabled. When "1", initialize metrics exporter and instruments.
- AIFO_CODER_OTEL_METRICS_FILE
  - Optional override path for the development metrics file sink. Default fallback when stderr sink unsupported:
    ${XDG_RUNTIME_DIR:-/tmp}/aifo-coder.otel.metrics.jsonl.

5) Initialization design

Public API (guarded by cfg(feature = "otel")):
- pub fn telemetry_init() -> Option<TelemetryGuard>

Behavior:
- Enablement:
  - If neither AIFO_CODER_OTEL=1 nor non-empty OTEL_EXPORTER_OTLP_ENDPOINT is present, return None immediately.
- Resource attributes (conservative and typed):
  - service.name = OTEL_SERVICE_NAME or "aifo-coder"
  - service.version = env!("CARGO_PKG_VERSION")
  - service.namespace = "aifo"
  - service.instance.id = "<pid>-<start_nanos>"
  - process.pid (i64)
  - host.name (best-effort), os.type, process.executable.name (best-effort)
  - Optional: deployment.environment if present (do not derive)
- Propagator:
  - opentelemetry::global::set_text_map_propagator(TraceContextPropagator::new()).
  - Do not install Baggage or a composite propagator that includes baggage by default (avoid accidental PII propagation).
- Traces exporter selection:
  - If feature "otel-otlp" is enabled AND OTEL_EXPORTER_OTLP_ENDPOINT is non-empty:
    - Use opentelemetry-otlp (tonic gRPC) with a batch span processor tuned for CLI.
    - Defaults when OTEL_BSP_* unset (CLI-friendly):
      - OTEL_BSP_SCHEDULE_DELAY ≈ 2s
      - OTEL_BSP_MAX_QUEUE_SIZE ≈ 2048
      - OTEL_BSP_EXPORT_TIMEOUT ≈ 5s
    - Respect OTEL_BSP_* env overrides and OTEL_EXPORTER_OTLP_TIMEOUT.
    - Create and hold a private Tokio multi-thread runtime in TelemetryGuard; do not leak global runtime handles. Name threads with prefix "aifo-otel-*".
  - Else:
    - Use opentelemetry-stdout exporter for traces with a simple processor (predictable flush).
    - MUST: configure the exporter to write to stderr (not stdout), or an equivalent stderr writer sink.
- Metrics exporter selection (only when AIFO_CODER_OTEL_METRICS=1):
  - With otel-otlp + endpoint: configure an OTLP metrics exporter + PeriodicReader (interval 1–2s).
  - Otherwise (dev): prefer stdout metrics exporter writing to stderr if supported; if not supported, write JSONL to:
    - ${AIFO_CODER_OTEL_METRICS_FILE} if set and non-empty, else
    - ${XDG_RUNTIME_DIR:-/tmp}/aifo-coder.otel.metrics.jsonl.
  - Never write metrics to stdout.
- Subscriber installation:
  - Always install the tracing_opentelemetry layer bound to the tracer.
  - Install fmt layer only when AIFO_CODER_TRACING_FMT=1. When installed, honor RUST_LOG as EnvFilter; default filter "warn".
  - When fmt is not installed, do not produce any additional stderr/stdout logs beyond current behavior; EnvFilter is not required.
  - Use try_init(); on conflict (global subscriber already set) emit a concise one-line warning and return None.
- Idempotence:
  - Use once_cell::sync::OnceCell to prevent double-initialization. Subsequent calls return None without side effects.
- TelemetryGuard:
  - Holds any shutdown handles (provider/runtime; meter provider if created).
  - Implements Drop to:
    - opentelemetry::global::shutdown_tracer_provider()
    - force-flush the meter provider (short timeout) to avoid losing metrics in short CLI runs
    - shut down the private Tokio runtime if created (drop at end).
- Error handling:
  - If initialization fails (exporter errors, subscriber conflicts), write a concise one-line warning to stderr and return None.
  - If OTEL_EXPORTER_OTLP_ENDPOINT is set but feature otel-otlp is not compiled:
    - Log a concise warning and fall back to stdout exporter (if compiled) or disable telemetry cleanly.
  - Never panic or alter CLI exit codes due to telemetry.
  - Never log exporter headers or secrets.

6) Instrumentation plan (spans; privacy-preserving)

General:
- Use #[cfg_attr(feature = "otel", tracing::instrument(...))] so attributes compile away when otel is off.
- Avoid heavy data; use booleans, counts, and short salted hashes (see Hash helper).
- Levels:
  - Info spans for top-level operations.
  - Debug events for detailed internals (docker previews, decisions).
  - Default fmt not installed, so user-facing stderr remains unchanged unless explicitly opted in.

Hash redaction helper:
- Implement FNV-1a 64-bit with a per-process salt (derive from pid and start_nanos).
- When AIFO_CODER_OTEL_PII != "1":
  - Record counts and salted hashes (args_count, cwd_hash) instead of raw data.
- When AIFO_CODER_OTEL_PII = "1":
  - Include raw cwd/args strings with caution; still avoid file contents, secrets, and env values.

Functions to instrument (minimal but valuable coverage):
- build_docker_cmd(agent, passthrough, image, apparmor_profile)
  - instrument(level="info", skip(passthrough, image, apparmor_profile), fields(agent=%agent))
  - Record preview_len (bytes), tty_enabled (bool), has_network (bool). Debug event with preview when helpful.
- toolchain_start_session(kinds, overrides, no_cache, verbose)
  - instrument(level="info", skip(overrides), fields(kinds=?kinds, no_cache=%no_cache))
  - Emit events per sidecar started; on failure set span status to error.
- toolchain_run(kind_in, args, image_override, no_cache, verbose, dry_run)
  - instrument(level="info", skip(args, image_override), fields(kind=%kind_in, no_cache=%no_cache, dry_run=%dry_run))
  - On docker exec failure, set span status to error.
- toolexec_start_proxy(session_id, verbose)
  - instrument(level="info", fields(session_id=%session_id, timeout_secs, use_unix))
  - Inside request loop (HTTP and Unix):
    - Wrap each request in info_span!("proxy_request", tool=%tool, kind=%kind, arg_count=argv.len(), cwd_hash=?hash(cwd), session_id=%session_id)
    - After execution, record exit_code (int) and dur_s (f64). On timeout/error, set span status to error.
- docker_supports_apparmor()
  - instrument(level="debug"), add events with detection results.
- desired_apparmor_profile() / desired_apparmor_profile_quiet()
  - instrument(level="debug"), record chosen profile and fallback reasons.
- preferred_registry_prefix() / preferred_registry_prefix_quiet()
  - instrument(level="debug"), record selected source (env, curl, tcp, env-empty) and probe duration; avoid raw URLs or IPs.
- acquire_lock() / acquire_lock_at()
  - instrument(level="info"), record which path succeeded and failures; set span status to error on failure.
- create_network_if_possible() / remove_network()
  - instrument(level="info"/"debug"), record network name, existence check result, and timing.
- toolchain_bootstrap_typescript_global()
  - instrument(level="info"), record result and duration.
- toolchain_purge_caches()
  - instrument(level="info"), record volumes attempted and removed.

Span status and errors:
- On failures (docker non-zero, proxy timeouts, lock contention), set span status to error with a concise message using tracing_opentelemetry::OpenTelemetrySpanExt.

7) Metrics plan (opt-in via AIFO_CODER_OTEL_METRICS=1)

Exporters/readers:
- With otel-otlp + endpoint: configure OTLP metrics exporter with a PeriodicReader (interval ~1–2s).
- Else (dev): configure stdout metrics exporter and route to stderr; if unsupported, write to ${AIFO_CODER_OTEL_METRICS_FILE} or ${XDG_RUNTIME_DIR:-/tmp}/aifo-coder.otel.metrics.jsonl. Never stdout.

Force flush:
- TelemetryGuard::drop must force-flush the meter provider with a short timeout to avoid losing metrics in short runs.

Instruments (names, low-cardinality labels, units):
- Counters (unit "1"):
  - aifo_runs_total{agent}
  - docker_invocations_total{kind=run|exec|image_inspect|network}
  - proxy_requests_total{tool,result=ok|err|timeout}
  - toolchain_sidecars_started_total{kind}
  - toolchain_sidecars_stopped_total{kind}
- Histograms (unit "s", cumulative temporality):
  - docker_run_duration{agent}
  - proxy_exec_duration{tool}
  - registry_probe_duration{source=curl|tcp}

Temporality:
- Use cumulative temporality unless the backend requires delta; document defaults and changes.

Cardinality and PII:
- Only low-cardinality labels (agent, kind, tool, result, source). No paths, usernames, hashes, or secrets in metrics.

Instrumentation points:
- Increment counters at the correct points (around docker run/exec, proxy requests).
- Time docker run and proxy exec via Instant; record durations in seconds.

8) Privacy and PII safeguards

- Default AIFO_CODER_OTEL_PII != "1":
  - Do not record raw cwd or args; record arg_count and cwd_hash (salted).
- If "1":
  - Include raw strings cautiously only for debugging; still avoid file contents, secrets, or env var values.
- Never record secrets or token/header values (including OTEL_EXPORTER_OTLP_HEADERS and AIFO_TOOLEEXEC_TOKEN).
- Do not read arbitrary files for telemetry beyond normal program needs.
- Do not install Baggage or composite propagators by default.

9) Performance considerations

- Telemetry is fully off unless compiled with "otel" and runtime-enabled via AIFO_CODER_OTEL=1 or OTEL_EXPORTER_OTLP_ENDPOINT set.
- Stdout exporter uses a simple processor for reliable flush and minimal overhead.
- OTLP uses a batch processor with bounded queues and timeouts to avoid stalling the CLI; respect OTEL_BSP_* env.
- Keep spans concise; use skip() in #[instrument] for large args.
- Do not install fmt by default; only with AIFO_CODER_TRACING_FMT=1.

10) Failure modes and handling

- Missing/invalid OTLP endpoint:
  - Emit a concise warning; fall back to stdout exporter (if compiled) or disable telemetry gracefully.
- Exporter backpressure or timeouts:
  - Use timeouts (default 5s) and CLI-appropriate batch settings. Never block the CLI indefinitely.
- Subscriber already set:
  - try_init() fails; emit one warning and return None.
- Double initialization attempts:
  - Protected by OnceCell; subsequent calls return None without side effects.
- Any init or export error:
  - Never panic; never change CLI exit codes or write to stdout.

11) Testing strategy and acceptance criteria

Default builds:
- With default build (no features), existing tests remain unchanged and pass.

Feature builds:
- With "otel" feature enabled but not runtime-enabled:
  - telemetry_init() returns None; tests unchanged.

Stdout-dev exporters (stderr/file sinks):
- With "otel" enabled and AIFO_CODER_OTEL=1:
  - Golden stdout test (acceptance): Run a short command (e.g., --help) with telemetry enabled (stdout-to-stderr exporters). Assert stdout is byte-for-byte identical to baseline. No stdout differences allowed.
  - Ensure no stderr logs appear unless AIFO_CODER_TRACING_FMT=1 is set.

OTLP builds:
- With "otel-otlp" feature enabled:
  - Misconfiguration test:
    - Set OTEL_EXPORTER_OTLP_ENDPOINT invalid; ensure CLI runs normally, with a single warning, and fallback (stdout exporter or none) works.
  - Local collector (manual):
    - AIFO_CODER_OTEL=1 OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317 cargo run --features otel-otlp -- --help

Metrics:
- With metrics enabled at runtime:
  - AIFO_CODER_OTEL=1 AIFO_CODER_OTEL_METRICS=1 cargo run --features otel -- --help
  - If stdout metrics exporter cannot target stderr, verify JSONL file written to ${AIFO_CODER_OTEL_METRICS_FILE} or default path.
- Short-run flush test: ensure traces and metrics are flushed in short runs (e.g., --help).

Idempotence test:
- Calling telemetry_init() twice should return None the second time with at most one concise warning if the subscriber is already set.

12) Rollout plan (phased implementation)

Phase 1: Scaffolding and safe initialization (no spans yet)
- Add Cargo features/dependencies.
- Implement telemetry_init() and TelemetryGuard with:
  - Enablement rules (AIFO_CODER_OTEL=1 or OTEL_EXPORTER_OTLP_ENDPOINT set).
  - Resource setup, propagator installation (TraceContext only).
  - Exporter selection (stdout-to-stderr simple; OTLP batch with private Tokio runtime named "aifo-otel-*").
  - Subscriber layers (otel layer always; fmt layer only if AIFO_CODER_TRACING_FMT=1; EnvFilter "warn" default when fmt installed).
  - Idempotence and error handling via OnceCell + try_init.
  - Drop: shutdown tracer provider; force-flush metrics; stop private runtime.
- Call telemetry_init() near the top of main() and keep the guard alive.

Phase 2: Minimal spans (privacy-preserving), no metrics yet
- Add #[cfg_attr(feature="otel", instrument(...))] attributes and lightweight events to functions listed in section 6.
- Add hash helper with per-process salt and PII gating.

Phase 3: OTLP exporter and fallback
- Implement OTLP path with batch processor and private Tokio runtime.
- Respect OTEL_TRACES_SAMPLER / OTEL_TRACES_SAMPLER_ARG and OTEL_EXPORTER_OTLP_TIMEOUT; honor OTEL_BSP_* for batch tuning.
- If endpoint set but otel-otlp not compiled, warn and fallback to stdout exporter (if compiled) or noop.

Phase 4: Metrics (opt-in at runtime)
- Initialize metrics exporter only when AIFO_CODER_OTEL_METRICS=1.
- Add counters and histograms per section 7 with low-cardinality labels; set proper units and cumulative temporality.
- Ensure force-flush on guard drop (meter provider).

Phase 5: Propagation to aifo-shim (optional, v8.x)
- Inject/propagate W3C traceparent across the shim/proxy boundary (HTTP/Unix). Use a standard "traceparent" header.
- Extract context on proxy side and create child spans for request execution, without adding dependencies to the shim.

Phase 6: CI and documentation
- CI job building with --features otel; smoke run and golden stdout test.
- Optional OTLP job if a collector is available.
- README updates for enabling tracing/metrics; env examples; tuning; troubleshooting.

13) Implementation checklist

Cargo.toml:
- Add optional dependencies and features per section 3; features off by default.

src/lib.rs:
- Add cfg(feature="otel") imports:
  - tracing::{instrument, info, debug, warn, error, Span}
  - tracing_subscriber::{EnvFilter, prelude::*, fmt}
  - opentelemetry::{global, KeyValue}
  - opentelemetry::propagation::TraceContextPropagator
  - opentelemetry_sdk::{trace as sdktrace, Resource}
  - conditional opentelemetry_stdout / opentelemetry_otlp usage
  - tracing_opentelemetry::OpenTelemetrySpanExt for status/error
  - once_cell::sync::OnceCell for idempotence
- Implement telemetry_init() and TelemetryGuard:
  - Enablement rules, resource and propagator setup (TraceContext only; no baggage).
  - Exporter selection (stdout-to-stderr simple; OTLP batch + private runtime with "aifo-otel-*" thread names).
  - Subscriber: otel layer always; fmt layer only if AIFO_CODER_TRACING_FMT=1; EnvFilter "warn" default when fmt installed.
  - try_init(), idempotence via OnceCell, concise warnings on conflict.
  - Drop: shutdown tracer provider, force-flush metrics, shutdown runtime.
- Implement stdout exporters with a stderr writer sink for both traces and metrics; if metrics writer unsupported, write to ${AIFO_CODER_OTEL_METRICS_FILE} or ${XDG_RUNTIME_DIR:-/tmp}/aifo-coder.otel.metrics.jsonl.
- Add hash helper for redaction (FNV-1a 64-bit) with per-process salt.
- Add #[cfg_attr(feature="otel", instrument(...))] attributes and events.
- On failures, set span status error with concise messages via OpenTelemetrySpanExt.

src/main.rs:
- Call telemetry_init() near the top (after dotenv) and keep the guard alive for process lifetime; drop guard after joining any proxy threads/cleanup to ensure flush.

Docs:
- README: enabling tracing/metrics; env variables; privacy defaults; sampler examples; exporter timeouts; troubleshooting (including HTTPS root CA trust notes).
- CI: jobs to compile with features and run smoke tests + golden stdout; optional OTLP integration job.

14) Concrete defaults and recommendations

- Stdout exporters MUST write to stderr (or file sink) so CLI stdout remains unchanged.
- Default OTLP timeout: 5s (override via OTEL_EXPORTER_OTLP_TIMEOUT).
- Recommended CLI batch settings via env:
  - OTEL_BSP_SCHEDULE_DELAY=2s
  - OTEL_BSP_MAX_QUEUE_SIZE=2048
  - OTEL_BSP_EXPORT_TIMEOUT=5s
- Sampling examples (set via env):
  - Always on (default parent-based): OTEL_TRACES_SAMPLER=parentbased_always_on
  - 10% traceid ratio: OTEL_TRACES_SAMPLER=parentbased_traceidratio; OTEL_TRACES_SAMPLER_ARG=0.1
- Units and temporality:
  - Counters: unit "1"
  - Durations: unit "s"
  - Default temporality: cumulative.

15) Example pseudo-code (lib.rs snippets)

#[cfg(feature = "otel")]
pub fn telemetry_init() -> Option<TelemetryGuard> {
    use once_cell::sync::OnceCell;
    static INIT: OnceCell<()> = OnceCell::new();

    if INIT.get().is_some() {
        return None;
    }

    let aifo_otel = std::env::var("AIFO_CODER_OTEL").ok().as_deref() == Some("1");
    let otlp_ep = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok().filter(|s| !s.trim().is_empty());
    if !aifo_otel && otlp_ep.is_none() {
        return None;
    }

    // Resource
    let service_name = std::env::var("OTEL_SERVICE_NAME").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| "aifo-coder".to_string());
    let attrs = vec![
        opentelemetry::KeyValue::new("service.name", service_name),
        opentelemetry::KeyValue::new("service.version", env!("CARGO_PKG_VERSION")),
        opentelemetry::KeyValue::new("service.namespace", "aifo"),
        opentelemetry::KeyValue::new("process.pid", std::process::id() as i64),
    ];
    let res = opentelemetry_sdk::Resource::new(attrs);

    // Propagator (TraceContext only)
    opentelemetry::global::set_text_map_propagator(opentelemetry::propagation::TraceContextPropagator::new());

    // Select exporter
    let use_otlp = {
        #[cfg(feature = "otel-otlp")]
        { otlp_ep.is_some() }
        #[cfg(not(feature = "otel-otlp"))]
        { false }
    };

    // Build tracer
    let (tracer, maybe_rt) = if use_otlp {
        // Build OTLP tracer with tonic + batch; create private tokio runtime (threads named "aifo-otel-*")
        (/* tracer */, /* Some(tokio_runtime) */)
    } else {
        // stdout exporter tracer writing to stderr with simple processor
        (/* tracer */, None)
    };

    // Metrics (optional, runtime-gated)
    let _maybe_meter_provider = if std::env::var("AIFO_CODER_OTEL_METRICS").ok().as_deref() == Some("1") {
        if use_otlp {
            // Configure OTLP metrics + PeriodicReader (1–2s)
            Some(/* meter provider */)
        } else {
            // stdout metrics to stderr if supported; else JSONL file in ${AIFO_CODER_OTEL_METRICS_FILE} or ${XDG_RUNTIME_DIR:-/tmp}
            Some(/* meter provider */)
        }
    } else {
        None
    };

    // Layers
    let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);
    let mut reg = tracing_subscriber::registry().with(otel_layer);

    // Optional fmt layer (explicit opt-in only)
    if std::env::var_os("AIFO_CODER_TRACING_FMT").is_some() {
        let env_filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "warn".to_string());
        reg = reg.with(tracing_subscriber::EnvFilter::new(env_filter)).with(tracing_subscriber::fmt::layer());
    }

    if reg.try_init().is_err() {
        eprintln!("aifo-coder: telemetry init skipped (global subscriber already set)");
        return None;
    }

    let _ = INIT.set(());
    Some(TelemetryGuard { /* store maybe_rt, maybe_meter_provider */ })
}

#[cfg(feature = "otel")]
pub struct TelemetryGuard { /* fields: Option<tokio::runtime::Runtime>, Option<meter provider> */ }

#[cfg(feature = "otel")]
impl Drop for TelemetryGuard {
    fn drop(&mut self) {
        // force-flush metrics if configured (short timeout)
        // self.meter_provider.force_flush()...
        opentelemetry::global::shutdown_tracer_provider();
        // Drop/shutdown private tokio runtime if present
    }
}

#[cfg(not(feature = "otel"))]
pub fn telemetry_init() -> Option<()> { None }

16) Security and compatibility notes

- Never log exporter headers or secrets.
- Never record AIFO_TOOLEEXEC_TOKEN or other sensitive env values.
- Use once_cell::sync::OnceCell for idempotence (aligns with crate usage).
- Cross-platform: Linux/macOS/Windows with tonic/rustls. For HTTPS collectors, ensure root CA trust via system certs (rustls-native-certs if needed) or document configuration.
- Private Tokio runtime threads should have a recognizable name prefix (e.g., "aifo-otel-*") to simplify debugging.

17) Developer commands (reference)

- Build (no telemetry):
  - cargo build
- Run with stdout telemetry (traces to stderr), no fmt layer:
  - AIFO_CODER_OTEL=1 cargo run --features otel -- --help
- Run with stdout telemetry and fmt layer (stderr logs visible):
  - AIFO_CODER_OTEL=1 AIFO_CODER_TRACING_FMT=1 RUST_LOG=info cargo run --features otel -- --help
- Run with OTLP exporter (collector must be reachable):
  - AIFO_CODER_OTEL=1 OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317 cargo run --features otel-otlp -- --help
- Enable metrics (stdout or OTLP depending on features/env):
  - AIFO_CODER_OTEL=1 AIFO_CODER_OTEL_METRICS=1 cargo run --features otel -- --help
