Title: OpenTelemetry support for aifo-coder (v2)
Status: Proposed
Owner: aifo-coder maintainers
Last-Updated: 2025-09-03

Overview

This document supersedes v1 with refined goals, risk mitigations, metrics support, and a phased implementation plan. It keeps telemetry fully optional and non-invasive, preserves current CLI UX by default, and provides robust choices for both local development and production export.

1) Goals

- Add optional, opt-in OpenTelemetry tracing and metrics to aifo-coder without changing current UX or test behavior by default.
- Provide low-overhead spans around key operations (docker orchestration, toolchain sidecars, proxy requests).
- Include metrics instruments (counters/histograms) behind a runtime gate; default disabled.
- Allow local development validation via stdout exporter; support production export via OTLP when configured.
- Keep PII out of telemetry by default (redact file paths and arguments; emit counts and salted hashes only).

2) Non-goals

- No mandatory telemetry; compiled-out by default build.
- No configuration via CLI flags in v2; runtime control via environment variables only.
- No changes to existing stdout/stderr UX by default (no additional fmt/log layer unless opted in).
- No span linking to external services; v2 focuses on in-process tracing and the built-in shim/proxy.

3) Build and feature gating (Cargo)

Features (off by default):
- otel: enables tracing and OpenTelemetry (stdout exporter for traces by default).
- otel-otlp: extends otel with the OTLP exporter and a Tokio runtime dependency.

Dependencies in Cargo.toml (all optional=true):
- tracing = { version = "0.1", features = ["std"], optional = true }
- tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt"], optional = true }
- opentelemetry = { version = "0.24", optional = true }
- opentelemetry_sdk = { version = "0.24", optional = true }
- tracing-opentelemetry = { version = "0.25", optional = true }
- opentelemetry-stdout = { version = "0.4", optional = true }
- opentelemetry-otlp = { version = "0.17", features = ["grpc-tonic"], optional = true }  # only with otel-otlp
- tokio = { version = "1", features = ["rt-multi-thread"], optional = true }             # only with otel-otlp

Cargo features:
- otel = ["tracing", "tracing-subscriber", "opentelemetry", "opentelemetry_sdk", "tracing-opentelemetry", "opentelemetry-stdout"]
- otel-otlp = ["otel", "opentelemetry-otlp", "tokio"]

Default build has neither feature enabled; all telemetry code compiles out.

4) Runtime configuration (environment)

Enablement:
- AIFO_CODER_OTEL=1
  - Enables telemetry initialization.
- OTEL_EXPORTER_OTLP_ENDPOINT
  - If set and non-empty, telemetry is enabled as well (even if AIFO_CODER_OTEL is not set).
- If both are unset/empty, telemetry_init() is a no-op even if compiled with features.

Exporters:
- OTEL_EXPORTER_OTLP_ENDPOINT
  - Example: http://otel-collector:4317 or https://collector.example.com:4317
- OTEL_EXPORTER_OTLP_HEADERS
  - Optional auth headers (e.g., "authorization=Bearer abc,another=val").
- OTEL_EXPORTER_OTLP_TIMEOUT
  - Request timeout (e.g., "5s"). Recommended for CLI stability.

Sampling and logging:
- OTEL_TRACES_SAMPLER / OTEL_TRACES_SAMPLER_ARG
  - Respect standard sampler configuration (defaults to parentbased_always_on).
- RUST_LOG
  - If tracing fmt layer is installed (opt-in), honor env-filter; default filter is conservative (warn).
- AIFO_CODER_TRACING_FMT=1
  - Opt-in to install a fmt layer so tracing events appear on stderr. By default, fmt is not installed to preserve UX.

PII handling:
- AIFO_CODER_OTEL_PII
  - Default "0": redact PII (paths/args). When "1", include raw cwd and args in spans (unsafe mode for debugging).

Metrics:
- AIFO_CODER_OTEL_METRICS
  - Default "0": metrics disabled. When "1", initialize metrics exporter and instruments.

5) Initialization design

Public API in src/lib.rs (guarded by cfg(feature="otel")):
- pub fn telemetry_init() -> Option<TelemetryGuard>

Behavior:
- Check enablement:
  - If neither AIFO_CODER_OTEL=1 nor a non-empty OTEL_EXPORTER_OTLP_ENDPOINT is present, return None immediately.
- Build Resource with conservative attributes:
  - service.name = OTEL_SERVICE_NAME or "aifo-coder"
  - service.version = env!("CARGO_PKG_VERSION")
  - service.namespace = "aifo"
  - service.instance.id = "<pid>-<start_nanos>"
  - host.name, os.type, process.pid (best-effort standard attributes; avoid usernames/paths)
- Decide exporter (traces):
  - If feature "otel-otlp" is enabled and OTEL_EXPORTER_OTLP_ENDPOINT is set (non-empty):
    - Use opentelemetry-otlp pipeline (traces via tonic gRPC).
    - Use batch span processor (Tokio multi-thread runtime).
    - Keep a private Tokio runtime in TelemetryGuard; do not leak it globally.
  - Else:
    - Use opentelemetry-stdout exporter for traces.
    - Prefer a simple span processor to guarantee flush for short-lived CLI.
- Decide exporter (metrics), when AIFO_CODER_OTEL_METRICS=1:
  - With otel-otlp + endpoint: configure a periodic reader and OTLP exporter for metrics.
  - Otherwise: optional stdout metrics exporter for local dev (low volume).
- Install tracing_subscriber with:
  - Always: tracing_opentelemetry layer bound to the tracer.
  - fmt layer: only when AIFO_CODER_TRACING_FMT=1 OR RUST_LOG is set. Default EnvFilter to "warn" to avoid changing UX. Without fmt, user-facing logs remain exactly as today.
  - Use try_init(); on conflict (subscriber already set) emit one concise warning and return None.
- Return a TelemetryGuard which:
  - Holds any shutdown handles (provider/runtime).
  - Implements Drop to call opentelemetry::global::shutdown_tracer_provider() and shut down the private Tokio runtime if created.

Error handling:
- If initialization fails (exporter errors, subscriber conflicts), write a concise one-line warning to stderr and return None.
- If OTEL_EXPORTER_OTLP_ENDPOINT is set but feature otel-otlp is not compiled:
  - Log a concise warning and fall back to stdout exporter (if available) or disable telemetry cleanly.
- Never panic or abort the main program due to telemetry.

Idempotence:
- Prevent double-initialization with a process-wide OnceCell/flag; subsequent calls return None without side effects.

6) Instrumentation plan (spans; privacy-preserving)

General:
- Use #[cfg_attr(feature = "otel", tracing::instrument(...))] on functions to compile away when otel is off.
- Avoid heavy data collection; prefer attributes with booleans, counts, and short salted hashes.
- Levels:
  - Use info spans for top-level operations.
  - Use debug for verbose internals (e.g., previews). End-user stderr remains unchanged unless fmt is enabled.

Hash redaction helper:
- Implement a small FNV-1a 64-bit hash helper with a per-process salt derived from pid and start_nanos.
- When AIFO_CODER_OTEL_PII != "1", record only counts and hashes (args_count, cwd_hash).
- When AIFO_CODER_OTEL_PII = "1", include raw cwd and, with caution, args (still avoid file contents or secrets).

Functions to instrument:
- build_docker_cmd(agent, passthrough, image, apparmor_profile)
  - instrument(level="info", skip(passthrough, image, apparmor_profile), fields(agent=%agent))
  - Record preview_len (bytes), tty_enabled (bool), has_network (bool). Emit debug event with preview if helpful; rely on EnvFilter to suppress by default.

- toolchain_start_session(kinds, overrides, no_cache, verbose)
  - instrument(level="info", skip(overrides), fields(kinds=?kinds, no_cache=%no_cache))

- toolchain_run(kind_in, args, image_override, no_cache, verbose, dry_run)
  - instrument(level="info", skip(args, image_override), fields(kind=%kind_in, no_cache=%no_cache, dry_run=%dry_run))

- toolexec_start_proxy(session_id, verbose)
  - instrument(level="info", fields(session_id=%session_id, timeout_secs, use_unix))
  - Inside the request loop (HTTP and Unix socket):
    - Wrap each request in an info_span!("proxy_request", tool=%tool, kind=%kind, arg_count=argv.len(), cwd_hash=?hash(cwd), session_id=%session_id)
    - After execution, record exit_code (int) and dur_ms (u128).

- docker_supports_apparmor()
  - instrument(level="debug"), add events with detection results.

- desired_apparmor_profile() / desired_apparmor_profile_quiet()
  - instrument(level="debug"), record which profile was chosen and why.

- preferred_registry_prefix() / preferred_registry_prefix_quiet()
  - instrument(level="debug"), record selected "source" (env, curl, tcp, env-empty).

7) Metrics plan (opt-in via AIFO_CODER_OTEL_METRICS=1)

Exporters/readers:
- With otel-otlp + endpoint: configure OTLP metrics exporter with a periodic reader (reasonable interval for CLI, e.g., 2–5s).
- Else (local dev): optionally enable stdout metrics exporter (very low volume).

Instruments:
- Counters:
  - aifo_runs_total{agent}
  - docker_invocations_total{kind=run|exec|image_inspect|network}
  - proxy_requests_total{tool,result=ok|err|timeout}
  - toolchain_sidecars_started_total{kind}
  - toolchain_sidecars_stopped_total{kind}
- Histograms (milliseconds):
  - docker_run_duration_ms{agent}
  - proxy_exec_duration_ms{tool}
  - registry_probe_duration_ms{source=curl|tcp}
- Cardinality and PII:
  - Use low-cardinality labels only (agent, kind, tool, result, source). No paths, no user names, no hashes in metrics.

Instrumentation points:
- Increment counters at function entry/exit as appropriate.
- Time docker run and proxy exec with Instant and record durations.
- For registry probes, time curl/TCP attempts.

8) Privacy and PII safeguards

- Default AIFO_CODER_OTEL_PII != "1":
  - Do not record raw cwd or args; record arg_count and cwd_hash (salted).
- If "1":
  - Include raw strings cautiously for debugging; still avoid file contents or secrets.
- Never record secrets, env var values, or tokens.
- Do not read arbitrary files for telemetry beyond what the program already needs.

9) Performance considerations

- Telemetry is fully off unless:
  - compiled with feature "otel" AND
  - runtime-enabled via AIFO_CODER_OTEL=1 or OTEL_EXPORTER_OTLP_ENDPOINT set.
- Use simple processor for stdout to ensure predictable flush and low overhead for short CLI runs.
- Use batch processor for OTLP, with bounded queues and timeouts to avoid stalling the CLI.
- Avoid cloning large vectors; use skip() in #[instrument] to avoid formatting big args unless needed.

10) Failure modes and handling

- Missing/invalid OTLP endpoint:
  - Log a concise warning and fall back to stdout exporter (if compiled) or disable telemetry gracefully.
- Exporter backpressure or timeouts:
  - Use reasonable OTLP timeouts (e.g., 5s). Batch processor defaults should be acceptable; document tuning envs.
- Subscriber already set by external code:
  - try_init() will fail; log a concise warning and return None.
- Any init error:
  - Never panic; never change CLI exit codes due to telemetry.

11) Testing strategy

Default builds:
- With default build (no features), existing tests remain unchanged.

Feature builds:
- With "otel" feature enabled:
  - If not runtime-enabled, telemetry_init() returns None; tests remain unchanged.
- With "otel" enabled and AIFO_CODER_OTEL=1:
  - Manual smoke check (stdout exporter):
    - RUST_LOG=trace AIFO_CODER_OTEL=1 cargo run --features otel -- --help
  - Ensure no extra stderr logs unless AIFO_CODER_TRACING_FMT=1 or RUST_LOG is set.

OTLP builds:
- With "otel-otlp" feature enabled:
  - Manual check with a local collector:
    - AIFO_CODER_OTEL=1 OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317 cargo run --features otel-otlp -- --help
  - Misconfiguration test:
    - Set OTEL_EXPORTER_OTLP_ENDPOINT to an invalid URL; ensure the CLI runs normally, with a single warning, and fallback (stdout or none) works.

Metrics:
- With metrics enabled at runtime:
  - AIFO_CODER_OTEL=1 AIFO_CODER_OTEL_METRICS=1 cargo run --features otel -- --help (stdout metrics optional)
  - With otlp exporter, verify metrics reach the collector when available (manual).

12) Rollout plan (phased implementation)

Phase 1: Scaffolding and safe initialization (no spans yet)
- Add Cargo features/dependencies.
- Implement telemetry_init() and TelemetryGuard with enablement rules, exporter selection, subscriber layers (otel layer only by default), idempotence, and error handling.
- Call telemetry_init() near the top of main() and keep the guard alive.

Phase 2: Minimal spans (privacy-preserving), no metrics yet
- Add #[cfg_attr(feature="otel", instrument(...))] attributes and lightweight events in the functions listed in section 6.
- Add hash helper with per-process salt and PII gating.

Phase 3: OTLP exporter and fallback
- Implement OTLP exporter path with batch processor and a private Tokio runtime.
- If endpoint set but otel-otlp not compiled, warn and fallback.
- Respect OTEL_* sampler variables.

Phase 4: Metrics (opt-in at runtime)
- Initialize metrics exporter only when AIFO_CODER_OTEL_METRICS=1.
- Add counters and histograms per section 7 with low-cardinality labels.

Phase 5: Propagation to aifo-shim (optional)
- Inject/propagate W3C traceparent across the shim/proxy boundary (HTTP/Unix).
- Link end-to-end spans across shim -> proxy -> sidecar exec.

Phase 6: CI and documentation
- Optional CI job building with --features otel; smoke run.
- Optional OTLP job if a collector is available in CI.
- README updates for enabling tracing and metrics; env examples; troubleshooting.

13) Implementation checklist

Cargo.toml:
- Add optional dependencies and features per section 3.
- Keep features off by default.

src/lib.rs:
- Add cfg(feature="otel") imports:
  - use tracing::{instrument, info, debug, warn};
  - use tracing_subscriber::{EnvFilter, prelude::*, fmt};
  - use opentelemetry::{global, KeyValue, Context};
  - use opentelemetry_sdk::{trace as sdktrace, Resource};
  - conditional opentelemetry_stdout / opentelemetry_otlp usage.
- Implement telemetry_init() and TelemetryGuard.
  - Enablement rules (AIFO_CODER_OTEL or OTEL_EXPORTER_OTLP_ENDPOINT).
  - Exporter selection and fmt layer gating.
  - try_init() and idempotence.
- Add hash helper for redaction (FNV-1a 64-bit) with per-process salt.
- Add #[cfg_attr(feature="otel", instrument(...))] attributes to listed functions.
- Add tracing events (mostly debug) for docker previews and proxy results.

src/main.rs:
- Call telemetry_init() near the top (after dotenv) and keep the guard alive for process lifetime.

Docs:
- README: How to enable telemetry and metrics, env variables, privacy defaults, sampler examples.
- CI: Optional job with --features otel.

14) Example pseudo-code (lib.rs snippets)

#[cfg(feature = "otel")]
pub fn telemetry_init() -> Option<TelemetryGuard> {
    use std::sync::OnceLock;
    static INIT: OnceLock<()> = OnceLock::new();

    if INIT.get().is_some() {
        return None;
    }

    let aifo_otel = std::env::var("AIFO_CODER_OTEL").ok().as_deref() == Some("1");
    let otlp_ep = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok().filter(|s| !s.trim().is_empty());
    if !aifo_otel && otlp_ep.is_none() {
        return None;
    }

    let service_name = std::env::var("OTEL_SERVICE_NAME").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| "aifo-coder".to_string());
    let version = env!("CARGO_PKG_VERSION");
    let mut attrs = vec![
        KeyValue::new("service.name", service_name),
        KeyValue::new("service.version", version),
        KeyValue::new("service.namespace", "aifo"),
    ];
    // add host.name, os.type, process.pid where feasible (best-effort)
    if let Ok(pid) = std::env::var("PID_PLACEHOLDER").or_else(|_| Ok(std::process::id().to_string())) {
        attrs.push(KeyValue::new("process.pid", pid));
    }
    let res = opentelemetry_sdk::Resource::new(attrs);

    #[cfg(feature = "otel-otlp")]
    let use_otlp = otlp_ep.is_some();
    #[cfg(not(feature = "otel-otlp"))]
    let use_otlp = false;

    let (tracer, maybe_rt) = if use_otlp {
        // Build OTLP tracer with tonic + batch; create private tokio runtime
        // Configure timeouts from env or defaults
        // ...
        (/* tracer */, /* Some(tokio_runtime) */)
    } else {
        // stdout exporter tracer with simple processor for reliable flush
        (/* tracer */, None)
    };

    // Layers: always otel layer; fmt only when AIFO_CODER_TRACING_FMT=1 or RUST_LOG is set.
    let mut reg = tracing_subscriber::registry().with(tracing_opentelemetry::layer().with_tracer(tracer));

    let install_fmt = std::env::var_os("AIFO_CODER_TRACING_FMT").is_some() || std::env::var_os("RUST_LOG").is_some();
    if install_fmt {
        let env_filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "warn".to_string());
        reg = reg
            .with(tracing_subscriber::EnvFilter::new(env_filter))
            .with(tracing_subscriber::fmt::layer());
    }

    if reg.try_init().is_err() {
        eprintln!("aifo-coder: telemetry init skipped (global subscriber already set)");
        return None;
    }

    let _ = INIT.set(());
    Some(TelemetryGuard { /* store maybe_rt, etc. */ })
}

#[cfg(feature = "otel")]
pub struct TelemetryGuard { /* fields if needed, e.g., Option<tokio::runtime::Runtime> */ }

#[cfg(feature = "otel")]
impl Drop for TelemetryGuard {
    fn drop(&mut self) {
        opentelemetry::global::shutdown_tracer_provider();
        // Drop/shutdown private tokio runtime if present
    }
}

#[cfg(not(feature = "otel"))]
pub fn telemetry_init() -> Option<()> { None }

15) Future work (v2.1+)

- Propagation across shim/proxy:
  - Inject W3C traceparent header from the CLI to the shim and from shim to proxy requests (HTTP and Unix socket path).
  - In the proxy, extract context and create child spans for request execution.
- Extended metrics:
  - Add gauges (e.g., sidecars_running{kind}) if useful.
  - Add exemplars linking trace/metrics when supported.

16) Developer commands (reference)

- Build normally (no telemetry):
  - cargo build
- Run with stdout telemetry (traces only):
  - AIFO_CODER_OTEL=1 cargo run --features otel -- --help
- Run with stdout telemetry and fmt layer (stderr logs):
  - AIFO_CODER_OTEL=1 AIFO_CODER_TRACING_FMT=1 RUST_LOG=info cargo run --features otel -- --help
- Run with OTLP exporter (collector must be reachable):
  - AIFO_CODER_OTEL=1 OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317 cargo run --features otel-otlp -- --help
- Enable metrics (stdout or OTLP depending on features/env):
  - AIFO_CODER_OTEL=1 AIFO_CODER_OTEL_METRICS=1 cargo run --features otel -- --help

This v2 specification preserves the no-op default behavior, avoids user-visible logging changes by default, adds robust optional tracing with strong privacy defaults (salted hashes), and includes an opt-in metrics plan with low-cardinality labels. It defines clear, stable enablement rules, resilient exporter behavior with a private Tokio runtime for OTLP, and a phased rollout to minimize risk.
