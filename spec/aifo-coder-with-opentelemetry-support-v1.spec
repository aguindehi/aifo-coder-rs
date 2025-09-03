Title: OpenTelemetry support for aifo-coder (v1)
Status: Draft
Owner: aifo-coder maintainers
Last-Updated: 2025-09-03

1) Goals

- Add optional, opt-in OpenTelemetry tracing to aifo-coder without changing current UX or test behavior by default.
- Provide low-overhead spans around key operations (docker orchestration, toolchain sidecars, proxy requests).
- Allow local development validation via stdout exporter; allow production export via OTLP when configured.
- Keep PII out of telemetry by default (redact file paths and arguments; emit counts and hashes only).

2) Non-goals (v1)

- No mandatory telemetry, no metrics (counters/histograms) in v1; metrics can follow in v1.1+.
- No configuration via CLI flags; runtime control is via environment variables only.
- No third-party logging changes; keep existing stdout/stderr messages intact.

3) Build and feature gating

- Add two Cargo features (both off by default):
  - otel: enables tracing and OpenTelemetry (stdout exporter by default).
  - otel-otlp: extends otel with the OTLP exporter and a Tokio runtime dependency.
- Default build has neither feature enabled; all telemetry code compiles out.
- With features enabled, telemetry is still disabled at runtime unless AIFO_CODER_OTEL=1 (or an OTEL endpoint is configured).

Dependencies to add in Cargo.toml:
- tracing = { version = "0.1", features = ["std"], optional = true }
- tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt"], optional = true }
- opentelemetry = { version = "0.24", optional = true }
- opentelemetry_sdk = { version = "0.24", optional = true }
- tracing-opentelemetry = { version = "0.25", optional = true }
- opentelemetry-stdout = { version = "0.4", optional = true }
- opentelemetry-otlp = { version = "0.17", features = ["grpc-tonic"], optional = true }  # only with otel-otlp
- tokio = { version = "1", features = ["rt-multi-thread"], optional = true }             # only with otel-otlp

Cargo features section:
- otel = ["tracing", "tracing-subscriber", "opentelemetry", "opentelemetry_sdk", "tracing-opentelemetry", "opentelemetry-stdout"]
- otel-otlp = ["otel", "opentelemetry-otlp", "tokio"]

4) Runtime configuration (environment)

- AIFO_CODER_OTEL=1
  - Enables telemetry initialization. If not set, telemetry_init() is a no-op even if compiled with otel.
- OTEL_SERVICE_NAME
  - Defaults to "aifo-coder" when unset.
- OTEL_EXPORTER_OTLP_ENDPOINT
  - If set, and feature otel-otlp is enabled, initialize OTLP exporter (gRPC via tonic).
  - Example: http://otel-collector:4317 or https://collector.example.com:4317
- OTEL_EXPORTER_OTLP_HEADERS
  - Optional auth headers (e.g., "authorization=Bearer abc,another=val").
- OTEL_TRACES_SAMPLER / OTEL_TRACES_SAMPLER_ARG
  - Respect standard sampler configuration (defaults to parentbased_always_on).
- AIFO_CODER_OTEL_PII
  - Default "0": redact PII (paths/args). When "1", include raw cwd and args in spans.
- RUST_LOG
  - If tracing subscriber is installed, honor env-filter, defaulting to "info" when unset.

5) Initialization design

Public API in src/lib.rs (guarded by cfg(feature="otel")):
- pub fn telemetry_init() -> Option<TelemetryGuard>
  - Behavior:
    - Check env AIFO_CODER_OTEL=1; if not set, return None immediately.
    - Build Resource:
      - service.name = OTEL_SERVICE_NAME or "aifo-coder"
      - service.version = env!("CARGO_PKG_VERSION")
      - service.namespace = "aifo"
      - service.instance.id = "<pid>-<start_nanos>" (best-effort)
    - Decide exporter:
      - If feature "otel-otlp" is enabled and OTEL_EXPORTER_OTLP_ENDPOINT is set (non-empty):
        - Use opentelemetry-otlp pipeline (traces via tonic gRPC).
        - Use batch span processor (Tokio runtime).
      - Else:
        - Use opentelemetry-stdout exporter (pretty JSON or compact).
        - Use batch or simple processor (simple is acceptable for local; batch preferred).
    - Install tracing_subscriber with:
      - EnvFilter from RUST_LOG or default "info"
      - fmt layer (keep current logs visible on stderr)
      - tracing_opentelemetry layer bound to the tracer
    - Return a TelemetryGuard which calls global shutdown on Drop to flush spans.

- Stub for non-otel builds:
  - pub fn telemetry_init() -> Option<()> { None } (or same signature returning Option<TelemetryGuard> but under cfg(not(feature="otel")) with a zero-sized guard)

TelemetryGuard:
- Holds any shutdown handles required by the SDK provider.
- Implements Drop to call opentelemetry::global::shutdown_tracer_provider().

Error handling:
- If initialization fails (exporter errors, subscriber conflicts), write a concise warning to stderr and return None.
- Never panic or abort the main program due to telemetry.

6) Instrumentation plan (v1)

General:
- Use #[cfg_attr(feature = "otel", tracing::instrument(...))] on functions to compile away when otel is off.
- Avoid heavy data collection; prefer attributes with booleans, counts, and short hashes.

Functions in src/lib.rs to instrument:
- build_docker_cmd(agent, passthrough, image, apparmor_profile)
  - instrument(level="info", skip(passthrough, image, apparmor_profile), fields(agent=%agent, image=?image))
  - Record preview length (bytes), tty_enabled (bool), has_network (bool).
  - Emit tracing::info!(target="docker", preview=%preview) when verbose or always; rely on EnvFilter to suppress in normal runs.

- toolchain_start_session(kinds, overrides, no_cache, verbose)
  - instrument(level="info", skip(overrides), fields(kinds=?kinds, no_cache=%no_cache))

- toolchain_run(kind_in, args, image_override, no_cache, verbose, dry_run)
  - instrument(level="info", skip(args, image_override), fields(kind=%kind_in, no_cache=%no_cache, dry_run=%dry_run))

- toolexec_start_proxy(session_id, verbose)
  - instrument(level="info", fields(session_id=%session_id, timeout_secs, use_unix))
  - Inside the request loop (both HTTP and Unix socket paths):
    - Wrap each request in an info_span!("proxy_request", tool=%tool, kind=%kind, arg_count=argv.len(), cwd_hash=?hash(cwd), session_id=%session_id)
    - After execution, record exit_code and dur_ms.

- docker_supports_apparmor()
  - instrument(level="debug"), add events with detection results.

- desired_apparmor_profile() / desired_apparmor_profile_quiet()
  - instrument(level="debug"), record which profile was chosen and why.

- preferred_registry_prefix() / preferred_registry_prefix_quiet()
  - instrument(level="debug"), record selected "source" (env, curl, tcp, env-empty).

- helper: hash redaction
  - Add a small non-cryptographic hash helper (e.g., 64-bit FNV-1a) to hash cwd and maybe args joined; use only when AIFO_CODER_OTEL_PII != "1".

In src/main.rs:
- Call let _otel = aifo_coder::telemetry_init(); at the beginning of main() (right after dotenv load and CLI parse is okay, but earlier is better if feasible).
- For the toolchain subcommand path, add small spans for bootstrap and cleanup steps (optional in v1; can be added later).

PII handling:
- Default AIFO_CODER_OTEL_PII != "1" -> redact:
  - Do not record raw args or cwd; record arg_count and cwd_hash.
- If "1", include raw strings cautiously; still avoid contents of files.

7) Metrics (post-v1 idea, excluded from v1 scope)

- Counters:
  - aifo_runs_total{agent}
  - docker_invocations_total{kind=run|exec|network|image_inspect}
  - proxy_requests_total{tool,result=ok|err|timeout}
- Histograms:
  - proxy_exec_duration_ms{tool}
  - docker_run_duration_ms{agent}
- Implementation via opentelemetry::metrics; gated behind "otel" and a flag like AIFO_CODER_OTEL_METRICS=1.

8) Performance considerations

- Telemetry is fully off unless:
  - compiled with feature "otel" AND
  - AIFO_CODER_OTEL=1 (or OTEL_EXPORTER_OTLP_ENDPOINT set, if we choose to allow that as implicit opt-in).
- Use batch span processor to reduce overhead when enabled.
- Keep spans concise; avoid cloning large vectors (use skip() in #[instrument]).

9) Testing strategy

- With default build (no features), existing tests remain unchanged.
- With "otel" feature enabled:
  - If AIFO_CODER_OTEL is not set, telemetry_init() returns None; tests remain unchanged.
  - Add a developer smoke check (manual) to verify stdout exporter emits spans:
    - RUST_LOG=info AIFO_CODER_OTEL=1 cargo run --features otel -- --help
- Do not assert on logs/spans in existing tests.

10) Rollout plan

- Phase 1: Land code with features off by default. Ensure builds/tests pass unchanged.
- Phase 2: Optional CI job to build with --features otel and run a small smoke run (not mandatory).
- Phase 3: Documentation update for enabling telemetry and pointing to OTLP collector configuration.

11) Failure modes and handling

- Missing/invalid OTLP endpoint: log a warning and fall back to stdout exporter (or disable telemetry) depending on feature availability.
- Exporter backpressure or errors: SDK handles batching; no impact on CLI functionality.
- Subscriber already set by external code: detect and log a warning; avoid double-initializing if possible.

12) Security and privacy

- Default to redacted telemetry:
  - Replace raw cwd and args with counts and short hashes.
  - Use environment control AIFO_CODER_OTEL_PII=1 to permit raw values if strictly necessary.
- Do not send API keys or secrets; never include env var values in telemetry.
- Respect user locale and process boundaries; avoid file reads for telemetry beyond what's already needed.

13) Implementation checklist

Cargo.toml:
- Add dependencies (optional=true) and features described above.

src/lib.rs:
- Add cfg(feature="otel") imports:
  - use tracing::{instrument, info, warn, error};
  - use tracing_subscriber::{EnvFilter, prelude::*, fmt};
  - use opentelemetry::{global, KeyValue};
  - use opentelemetry_sdk::{trace as sdktrace, Resource};
  - conditional opentelemetry_stdout / opentelemetry_otlp usage.
- Implement telemetry_init() and TelemetryGuard.
- Add hash helper for redaction.
- Add #[cfg_attr(feature="otel", instrument(...))] attributes to listed functions.
- Add tracing::info! events for docker previews and proxy results where helpful.

src/main.rs:
- Call telemetry_init() near the top of main() and keep the guard alive for the process lifetime.

Docs (optional follow-up):
- README: How to enable telemetry, example env, tracing filters.
- CI: optional job building with --features otel.

14) Example pseudo-code (lib.rs)

#[cfg(feature = "otel")]
pub fn telemetry_init() -> Option<TelemetryGuard> {
    if std::env::var("AIFO_CODER_OTEL").ok().as_deref() != Some("1") &&
       std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok().filter(|s| !s.is_empty()).is_none() {
        return None;
    }
    let service_name = std::env::var("OTEL_SERVICE_NAME").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| "aifo-coder".to_string());
    let version = env!("CARGO_PKG_VERSION");
    let res = opentelemetry_sdk::Resource::new(vec![
        KeyValue::new("service.name", service_name),
        KeyValue::new("service.version", version),
        KeyValue::new("service.namespace", "aifo"),
    ]);
    #[cfg(feature = "otel-otlp")]
    let (tracer, guard_impl) = if let Ok(endpoint) = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT") {
        if !endpoint.trim().is_empty() {
            // build OTLP tracer with tonic + batch
            // ...
        } else { /* fallback to stdout */ }
    } else { /* stdout */ };

    #[cfg(not(feature = "otel-otlp"))]
    let (tracer, guard_impl) = {
        // stdout exporter tracer (simple or batch)
        // ...
    };

    let env_filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
    let fmt_layer = tracing_subscriber::fmt::layer();
    let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(env_filter))
        .with(fmt_layer)
        .with(otel_layer)
        .init();

    Some(TelemetryGuard { /* guard_impl */ })
}

#[cfg(feature = "otel")]
pub struct TelemetryGuard { /* fields if needed */ }

#[cfg(feature = "otel")]
impl Drop for TelemetryGuard {
    fn drop(&mut self) {
        opentelemetry::global::shutdown_tracer_provider();
    }
}

#[cfg(not(feature = "otel"))]
pub fn telemetry_init() -> Option<()> { None }

15) Developer commands (for reference)

- Build normally (no telemetry):
  - cargo build
- Run with stdout telemetry:
  - AIFO_CODER_OTEL=1 cargo run --features otel -- --help
- Run with OTLP exporter (collector must be reachable):
  - AIFO_CODER_OTEL=1 OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317 cargo run --features otel-otlp -- --help

This specification and plan keeps telemetry fully optional and non-invasive, adds minimal, high-value spans around the most important operations, and provides a clean path to production export via OTLP without impacting existing users or tests.
