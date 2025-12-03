Title: OpenTelemetry support for aifo-coder (v9)
Status: Draft
Owner: aifo-coder maintainers
Last-Updated: 2025-12-03

Overview

This v9 specification intentionally diverges from the v6–v8 “opt‑in telemetry” model. It defines a
deployment profile where:

- Telemetry code (tracing + OTLP) is always compiled into the binary.
- Telemetry is enabled by default for users who do nothing special.
- A default OTLP endpoint is used when none is configured:
  http://alloy-collector-az.service.dev.migros.cloud
- Environment variables control:
  - whether telemetry is enabled at all,
  - which endpoint/timeouts/sampler are used,
  - whether a stderr fmt layer is installed for span logging.
- All telemetry continues to avoid stdout, never changes CLI exit codes, and is best‑effort.

This spec is intended for internal deployments where default “phone‑home” telemetry is acceptable and
desirable, and may not be suitable for upstream open‑source defaults.

0) Changes from v8 to v9

v9 is an evolution of v8 with different defaults and stronger assumptions:

Compile‑time behavior:
- v8: Telemetry code is compile‑time optional via Cargo features `otel` and `otel-otlp`. Default build has telemetry compiled out.
- v9: Telemetry dependencies are always linked; crate code is always built with tracing/OTEL support. No feature gating is used to remove telemetry at compile time.

Runtime defaults:
- v8: Telemetry is runtime‑opt‑in: only enabled when `AIFO_CODER_OTEL=1` or `OTEL_EXPORTER_OTLP_ENDPOINT` is non‑empty. Default CLI behavior does not emit telemetry and does not install any exporters.
- v9: Telemetry is runtime‑opt‑out: enabled by default if `AIFO_CODER_OTEL` is unset. A default endpoint (`http://alloy-collector-az.service.dev.migros.cloud`) is used when `OTEL_EXPORTER_OTLP_ENDPOINT` is unset/empty. Users who do nothing special will send telemetry.

Wrapper and Makefile:
- v8: Makefile and wrapper export OTEL env defaults; binary relies heavily on env to decide enablement.
- v9: The binary owns the defaults. The Makefile and `./aifo-coder` wrapper no longer hard‑set OTEL envs; they only optionally override or pass through configuration.

Telemetry UX:
- v8: Strong emphasis that default builds and default runs produce zero telemetry and do not alter stderr.
- v9: Telemetry is always active by default at runtime, but:
  - never writes to stdout,
  - only writes short init lines to stderr when `--verbose` is used (or when `AIFO_CODER_OTEL_VERBOSE=1`),
  - span logging to stderr is only enabled when `AIFO_CODER_TRACING_FMT=1`.

Privacy and PII:
- v8 and v9 share the same PII policy; v9 does not alter the PII story:
  - By default, cwd/args are hashed and redacted.
  - `AIFO_CODER_OTEL_PII=1` allows unsafe debugging with raw cwd/args.
  - Metrics remain low‑cardinality and do not include PII.

Migration and compatibility:
- v9 keeps the same environment variable names as v8 where possible.
- Existing tests that rely on a “no‑telemetry by default” invariant must be updated or feature‑guarded for the new deployment profile.
- A migration section (13) describes how to go from v8 behavior to v9 behavior in a controlled way.

1) Goals

- Always compile OpenTelemetry tracing and metrics support into the `aifo-coder` binary.
- Enable telemetry by default for standard CLI runs; an environment override can disable it.
- Default endpoint: send traces and (optionally) metrics to a corporate collector at
  `http://alloy-collector-az.service.dev.migros.cloud` when no endpoint is configured.
- Keep all telemetry best‑effort:
  - never write to stdout,
  - never change exit codes,
  - never panic the process.
- Preserve privacy defaults, PII controls, and low‑cardinality metrics as in v8.
- Keep the “fmt logging” layer strictly opt‑in via environment variables.
- Make the defaults live in Rust code, not duplicated between the Makefile and wrapper.

2) Non-goals

- v9 does not attempt to:
  - Preserve the v8 “zero‑telemetry by default” behavior.
  - Maintain “feature‑free build equals no telemetry code present”. Telemetry is always built in.
  - Introduce new CLI flags for telemetry; configuration remains environment‑only.
  - Change stdout content or exit codes due to telemetry.
  - Make telemetry mandatory in all distributions of the code; v9 describes a behavior profile, but forks may still reintroduce compile‑time gating.

3) Build and feature model (Cargo)

Dependencies in Cargo.toml (non‑optional):
- tracing = { version = "0.1", features = ["std"] }
- tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt"] }
- opentelemetry = { version = "0.24" }
- opentelemetry_sdk = { version = "0.24", features = ["rt-tokio"] }
- tracing-opentelemetry = { version = "0.25" }
- opentelemetry-stdout = { version = "0.5" }
- opentelemetry-otlp = { version = "0.17", features = ["grpc-tonic"] }
- tokio = { version = "1", features = ["rt-multi-thread"] }
- hostname = "0.3"
- humantime = "2.1"
- once_cell = "1"

Features section:
- The `otel` and `otel-otlp` features remain defined for backward compatibility but no longer gate code:
  - They may be left as empty feature sets or used only to adjust minor behavior (for example, enabling a couple of extra tests).
  - They do not control whether telemetry is compiled in; all telemetry code builds regardless of features.

Default build:
- Always includes telemetry support.
- There is no “build without telemetry code” configuration in v9.

4) Runtime configuration and defaults (environment)

The key design change is how telemetry enablement and endpoint selection work.

Enablement:

- AIFO_CODER_OTEL
  - If set and truthy (“1”, “true”, “yes”, case‑insensitive):
    - Telemetry is enabled.
  - If set and falsy (“0”, “false”, “no”, “off”, case‑insensitive):
    - Telemetry is disabled; `telemetry_init()` returns None and no exporters are installed.
  - If unset:
    - Telemetry is **enabled by default**.

- OTEL_EXPORTER_OTLP_ENDPOINT
  - If set and non‑empty: used as the OTLP/gRPC endpoint.
  - If unset or empty: v9 default endpoint is used:
    `http://alloy-collector-az.service.dev.migros.cloud`.

- Effective behavior:
  - Default (no env vars set): telemetry ON, using alloy endpoint.
  - Explicit `AIFO_CODER_OTEL=0`: telemetry OFF regardless of endpoint envs.
  - Explicit `AIFO_CODER_OTEL=1` and no endpoint: telemetry ON using alloy endpoint.
  - Explicit endpoint overrides default.

Other exporters and sinks (same semantics as v8 with new defaults):

- OTEL_EXPORTER_OTLP_HEADERS
  - Auth headers, never logged.

- OTEL_EXPORTER_OTLP_TIMEOUT
  - Request timeout (e.g. “5s”); default in v9: “5s” if unset.
  - Used for both traces and metrics OTLP exporters.

- OTEL_BSP_* environment:
  - OTEL_BSP_SCHEDULE_DELAY (default 2s)
  - OTEL_BSP_MAX_QUEUE_SIZE (default 2048)
  - OTEL_BSP_EXPORT_TIMEOUT (default 5s)
  - Respected by the OTLP batch span processor; defaults remain CLI‑friendly.

Sampling and logging:

- OTEL_TRACES_SAMPLER / OTEL_TRACES_SAMPLER_ARG
  - Standard sampler configuration; default `parentbased_always_on`.

- AIFO_CODER_TRACING_FMT
  - “0” (or unset): fmt logging layer **not** installed; spans/events do not produce stderr logs.
  - “1”: fmt logging layer installed with EnvFilter; allows span/event logging to stderr.

- RUST_LOG
  - Controls EnvFilter when fmt layer is installed. Default filter string when unset: “warn”.
  - Has no effect when fmt layer is not installed.

OTEL verbosity:

- AIFO_CODER_OTEL_VERBOSE
  - When “1”, telemetry initialization prints concise informational lines to stderr describing:
    - Whether OTLP/exporters are enabled.
    - Which endpoint is in use.
    - Whether metrics are enabled and which sink is used.
  - v9 keeps the existing behavior: `main` sets this when `--verbose` CLI flag is used.

PII handling (unchanged from v8):

- AIFO_CODER_OTEL_PII
  - Default “0” or unset: redact PII (paths/args) and use salted hashes in spans.
  - When “1”: include raw cwd and args in spans (unsafe; for debugging only).

Metrics control:

- AIFO_CODER_OTEL_METRICS
  - Default “0” (or unset): metrics disabled; no meter provider installed.
  - “1”: metrics enabled. With endpoint present, use OTLP metrics; otherwise, dev metrics exporter (stderr/file) is used.

- AIFO_CODER_OTEL_METRICS_FILE
  - Optional JSONL file sink; default:
    `${XDG_RUNTIME_DIR:-/tmp}/aifo-coder.otel.metrics.jsonl` when dev metrics exporter is used and stderr is not desired.

5) Initialization design (v9)

Public API:

- `pub fn telemetry_init() -> Option<TelemetryGuard>`

Behavior:

- Enablement:
  - `telemetry_enabled_env()` implements the new default:
    - If `AIFO_CODER_OTEL` is unset: enabled.
    - If set to truthy: enabled.
    - If set to falsy: disabled.
  - If disabled: `telemetry_init()` returns `None` immediately and does nothing.

- Endpoint:
  - `effective_otlp_endpoint()`:
    - If `OTEL_EXPORTER_OTLP_ENDPOINT` is set and non‑empty, returns that value.
    - Else, returns the v9 default:
      `http://alloy-collector-az.service.dev.migros.cloud`.
  - This endpoint is used both for traces and metrics when OTLP is enabled.

- Resource attributes (same as v8):
  - service.name = `OTEL_SERVICE_NAME` or “aifo-coder”
  - service.version = `env!("CARGO_PKG_VERSION")`
  - service.namespace = “aifo”
  - service.instance.id = “<pid>-<start_nanos>”
  - process.pid, host.name, os.type, process.executable.name
  - Optional deployment.environment from env (no derivation).

- Propagator:
  - As in v8: `TraceContextPropagator` only; no Baggage by default.

- Traces exporter selection:
  - OTLP path:
    - Uses `opentelemetry-otlp` with gRPC tonic and batch span processor.
    - Endpoint is chosen by `effective_otlp_endpoint()`.
    - BSP environment (`OTEL_BSP_*`) respected.
    - The timeout for exporter calls uses `OTEL_EXPORTER_OTLP_TIMEOUT` with 5s default.
    - A private Tokio multi‑thread runtime is created in `TelemetryGuard` (same as v8), thread names prefixed `aifo-otel-*`.
  - Dev path:
    - If OTLP initialization fails (invalid endpoint, runtime error), fallback to the stderr exporter defined in v8:
      a simple `StderrSpanExporter` with `SimpleSpanProcessor`.
    - This exporter writes only concise “otel-span name=... trace_id=... span_id=...” lines to stderr and never stdout.

- Metrics exporter:
  - Enabled only when `AIFO_CODER_OTEL_METRICS=1`.
  - With OTLP endpoint available:
    - Use OTLP metrics exporter via `opentelemetry-otlp`, `PeriodicReader` with 1–2s interval.
  - Without OTLP:
    - Use dev metrics exporter:
      - `opentelemetry-stdout` with `stderr` writer when possible.
      - Otherwise, JSONL file at `AIFO_CODER_OTEL_METRICS_FILE` or default runtime path.

- Subscriber installation:
  - Always install `tracing_opentelemetry` layer with the configured tracer.
  - Install fmt layer only when `AIFO_CODER_TRACING_FMT=1`:
    - Use `RUST_LOG` for EnvFilter; default “warn” when unset.
  - When fmt is not installed:
    - Do not produce extra stderr logs from tracing beyond OTEL init messages (when verbose).

- Idempotence:
  - Use `OnceCell` as in v8; multiple calls to `telemetry_init()`:
    - First call attempts initialization and may return `Some(TelemetryGuard)` or `None`.
    - Subsequent calls return `None` without side effects.

- TelemetryGuard drop:
  - On Drop:
    - Force flush metrics provider (if any) with a short timeout.
    - Call `opentelemetry::global::shutdown_tracer_provider()`.
    - Drop the private Tokio runtime if present.

- Error handling:
  - Initialization failures:
    - Write a concise one‑line warning to stderr only when `AIFO_CODER_OTEL_VERBOSE=1`.
    - Always return `None` on failure; never panic.

6) Instrumentation: spans and privacy (unchanged from v8)

v9 preserves the v8 instrumentation plan and privacy model:

- Use `#[cfg_attr(feature = "otel", tracing::instrument(...))]` attributes in v8. In v9,
  since telemetry is always compiled, these attributes become non‑conditional (plain `#[tracing::instrument]`).
- Avoid heavy data; use counts and salted hashes for PII fields by default.
- Span levels: `info` for top‑level operations, `debug` for internals.
- `AIFO_CODER_OTEL_PII` controls whether raw cwd/args are recorded.

Instrumentation targets (same as v8):

- `build_docker_cmd`, `docker_supports_apparmor`, `desired_apparmor_profile[_quiet]`
- `toolchain_start_session`, `toolchain_run`, `toolchain_bootstrap_typescript_global`, `toolchain_purge_caches`
- `toolexec_start_proxy` and inner proxy request loop
- Registry probing: `preferred_registry_prefix[_quiet]`, `preferred_internal_registry_prefix_*`
- Lock acquisition: `acquire_lock`, `acquire_lock_at`
- Sidecar network operations: `ensure_network_exists`, `remove_network`
- Span status set via `OpenTelemetrySpanExt` on failures/timeouts.

7) Metrics: unchanged semantics, new defaults

Metrics instruments remain as in v8:

- Counters:
  - `aifo_runs_total{agent}`
  - `docker_invocations_total{kind=run|exec|image_inspect|network}`
  - `proxy_requests_total{tool,result=ok|err|timeout}`
  - `toolchain_sidecars_started_total{kind}`
  - `toolchain_sidecars_stopped_total{kind}`

- Histograms:
  - `docker_run_duration{agent}` (unit “s”)
  - `proxy_exec_duration{tool}` (unit “s”)
  - `registry_probe_duration{source=curl|tcp}` (unit “s”)

Temporality:
- Cumulative by default unless overridden by OTLP backend.

PII and cardinality:
- Same constraints as v8:
  - No paths, usernames, hashes, or secrets in metrics.
  - Only low‑cardinality labels.

v9 only changes the defaults for exporter selection as described in section 5.

8) Privacy and PII safeguards (unchanged)

- Default `AIFO_CODER_OTEL_PII != "1"`:
  - Do not record raw cwd or args; record counts and salted hashes.
- If `AIFO_CODER_OTEL_PII = "1"`:
  - Record cwd/args strings with caution; never record file contents, env secrets or tokens.
- Never record:
  - `AIFO_TOOLEEXEC_TOKEN`
  - `OTEL_EXPORTER_OTLP_HEADERS` values
  - Any API keys or secrets.

9) Performance considerations

- Telemetry ON by default:
  - With OTLP endpoint reachable, overhead is governed by sampler and BSP configuration.
  - For busy dev flows, `OTEL_TRACES_SAMPLER=parentbased_traceidratio` and `OTEL_TRACES_SAMPLER_ARG=0.1` recommended.

- Default OTLP and BSP settings:
  - `OTEL_EXPORTER_OTLP_TIMEOUT=5s`
  - `OTEL_BSP_SCHEDULE_DELAY=2s`
  - `OTEL_BSP_MAX_QUEUE_SIZE=2048`
  - `OTEL_BSP_EXPORT_TIMEOUT=5s`

- fmt layer remains opt‑in to avoid extra logs on stderr for normal runs.
- Dev exporter (stderr/file) is only used when OTLP init fails or is explicitly disabled.

10) Failure modes and handling

- Invalid OTLP endpoint:
  - If the endpoint is syntactically invalid or unreachable:
    - Attempt an OTLP init; on failure, log a concise warning (when OTEL_VERBOSE) and fall back to stderr span exporter.
- Failed metrics exporter init:
  - Disable metrics and log a concise warning (when OTEL_VERBOSE).
- Global subscriber already set:
  - `try_init()` failure:
    - Log “telemetry init skipped (global subscriber already set)” once when OTEL_VERBOSE.
    - Return `None`.
- Double initialization attempts:
  - `OnceCell` prevents double init; subsequent calls return `None` without side effects.
- Any exporter or runtime error must never:
  - Panic,
  - Change exit codes,
  - Write to stdout.

11) Testing strategy and acceptance criteria (v9)

Important change: the golden stdout tests in v8 assumed “no telemetry when enabled with AIFO_CODER_OTEL=1 but default build off”. In v9, telemetry is on by default and always compiled; tests must adapt accordingly.

Default runs:

- A plain `cargo run -- --help`:
  - Must produce deterministic stdout identical across runs.
  - May produce no OTEL logs (if OTEL_VERBOSE is not set) or a fixed set (under `--verbose`).
- Golden stdout:
  - Maintain the v8 invariant: enabling telemetry must not change stdout. This still holds, but in v9 stdout is always telemetry‑neutral.

Feature builds (if features kept):

- `cargo build` and `cargo build --features otel` / `--features otel-otlp` should be equivalent w.r.t telemetry behavior. Features may only affect tests, not runtime telemetry logic.

OTLP path:

- With default endpoint:
  - Run a small command (e.g. `--help`) and verify:
    - No stdout differences vs baseline.
    - No panics or exit code changes when the OTLP endpoint is unreachably slow or down.
- Misconfiguration:
  - Set `AIFO_CODER_OTEL=1 OTEL_EXPORTER_OTLP_ENDPOINT="http://invalid:1234"` and ensure:
    - CLI runs to completion.
    - At most one concise warning on stderr (when `--verbose`).
    - Dev stderr exporter fallback works if configured.

Metrics:

- With metrics runtime enabled:
  - `AIFO_CODER_OTEL_METRICS=1` and default endpoint:
    - Ensure at least one metrics export is attempted (via mock collector or logs).
  - Without OTLP:
    - Ensure JSONL file or stderr sink is used, never stdout.

Idempotence:

- Two calls to `telemetry_init()` from different parts of the code:
  - First call either succeeds or fails.
  - Second call returns `None`; if OTEL_VERBOSE, only one warning is printed about subscriber conflicts.

12) Rollout plan (phased implementation for v9)

Phase 0: Agreement on policy shift
- Explicitly accept that v9 breaks v8’s “telemetry opt‑in and compiled‑out by default” requirement.
- Ensure this is acceptable in your deployment and document the change.

Phase 1: Build and crate wiring
- Update Cargo.toml to make telemetry deps non‑optional.
- Remove `cfg(feature = "otel")` and similar guards from:
  - `src/lib.rs`
  - `src/telemetry.rs`
  - instrumented modules.
- Ensure `telemetry_init()` is always available and used in `main`.

Phase 2: Runtime defaults and env model
- Implement `telemetry_enabled_env()` with default ON semantics.
- Implement `effective_otlp_endpoint()` with default alloy URL.
- Wire these into `build_tracer()` and `build_metrics_provider()`.
- Remove OTEL default exports from the Makefile and the `aifo-coder` wrapper; let the binary own defaults.

Phase 3: Wrapper and Makefile cleanup
- In `aifo-coder`:
  - Remove or simplify `set_otel_defaults()`; it should not force endpoint or AIFO_CODER_OTEL anymore.
  - Keep `CARGO_FLAGS` wiring as in v8, but telemetry features no longer have semantic meaning.
- In Makefile:
  - Remove the block that unconditionally `export`ed OTEL env vars.
  - Keep `CARGO_FLAGS` pointing at telemetry features if needed for compatibility (but runtime no longer depends on them).

Phase 4: Tests and CI
- Update `ci/otel-golden-stdout.sh`:
  - Remove assumptions that telemetry is off until `AIFO_CODER_OTEL=1` is set.
  - Instead, test invariants:
    - stdout identical for runs with different env telemetry settings.
    - `AIFO_CODER_OTEL=0` fully disables telemetry.
- Add tests to cover:
  - Default endpoint behavior.
  - `AIFO_CODER_OTEL=0` disables telemetry.
  - Endpoint overrides.
  - fmt layer enablement via `AIFO_CODER_TRACING_FMT=1`.

Phase 5: Documentation and README
- Update README telemetry section:
  - Clarify that telemetry is enabled by default.
  - Document `AIFO_CODER_OTEL` semantics (opt‑out).
  - Provide env examples for disabling telemetry and for changing endpoints/samplers.

Phase 6: Optional upstream reconciliation
- If upstream must preserve “no telemetry by default” policy, consider:
  - Keeping v8 as the upstream spec.
  - Making v9 behavior guarded behind a feature flag or compile‑time configure parameter used only in internal builds.

13) Migration guide: v8 → v9

This section summarizes the concrete changes needed to move from v8 to v9 behavior in a codebase already implementing v8.

1. Cargo.toml:
   - Remove `optional = true` from telemetry‑related deps.
   - Keep or neutralize `otel` / `otel-otlp` features so older scripts/builds remain valid, but do not gate telemetry code.

2. lib.rs:
   - Remove `#[cfg(feature = "otel")]` around `mod telemetry;` and `pub use telemetry::*;`.
   - Remove the non‑otel stub:
     ```rust
     #[cfg(not(feature = "otel"))]
     pub fn telemetry_init() -> Option<()> { None }
     ```
   - Ensure `telemetry_init()` is always available.

3. Telemetry init:
   - Replace v8’s `telemetry_enabled_env()` with the v9 version that:
     - Defaults to enabled when `AIFO_CODER_OTEL` is unset.
     - Disables when `AIFO_CODER_OTEL` is falsy.
   - Implement `effective_otlp_endpoint()` with default alloy URL.
   - Use this endpoint in OTLP trace and metrics exporter configuration.

4. Wrapper (`aifo-coder`):
   - Remove or minimize `set_otel_defaults()`; do not force OTEL env defaults from the wrapper.
   - Keep `CARGO_FLAGS` as before for consistency, but telemetry features are no longer essential.

5. Makefile:
   - Delete the OTEL env `export` block at the top.
   - Keep `CARGO_FLAGS ?= --features otel-otlp` if you want to preserve compatibility with older tooling; it is no longer required for telemetry at runtime.

6. Tests/CI:
   - Update the golden stdout test to reflect that telemetry is always possible; focus on ensuring that changing telemetry env vars does not change stdout.
   - Add explicit tests for `AIFO_CODER_OTEL=0`.

7. Documentation:
   - Update any docs that state “telemetry is off by default” to reflect the v9 behavior: “telemetry is enabled by default but opt‑out via AIFO_CODER_OTEL”.

This completes the v9 specification: always‑compiled telemetry, default OTLP export to a corporate collector, env‑based override model, and a phased migration from v8.

````
