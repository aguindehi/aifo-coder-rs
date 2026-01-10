Title: OpenTelemetry support for aifo-coder (v10)
Status: Draft
Owner: aifo-coder maintainers
Last-Updated: 2025-12-03

Overview

v10 refines the v9 telemetry behavior in three ways:

- Introduces a clear, layered model for the default OTLP endpoint:
  - code default: a safe, non-sensitive example endpoint (localhost),
  - optional build-time override via build.rs from an external config file or env,
  - runtime override via OTEL_EXPORTER_OTLP_ENDPOINT env.
- Clarifies the compile-time feature model:
  - telemetry remains compile-time optional for developers (via `otel` / `otel-otlp`),
  - internal builds and CI enable telemetry by default via CARGO_FLAGS.
- Removes remaining ambiguity about where telemetry defaults live:
  - binary code owns runtime defaults for enablement and endpoint,
  - Makefile and wrapper do not hard-set OTEL env defaults; they only influence features.

Telemetry continues to be:

- enabled by default at runtime when compiled with `otel`/`otel-otlp` and AIFO_CODER_OTEL is unset,
- fully opt-out via environment,
- best-effort and never writing to stdout or changing exit codes.

0) Changes from v9 to v10 (summary)

Endpoint defaults:

- v9:
  - Conceptually: default endpoint is `http://alloy-collector-az.service.dev.migros.cloud`.
  - Implementation: endpoint lived primarily in code and/or Makefile/wrapper env; not clearly factored.
- v10:
  - Code default is a non-sensitive example endpoint:
    - `"http://localhost:4317"`.
  - Internal deployments can *bake in* a different default via `build.rs` using:
    - AIFO_OTEL_ENDPOINT_FILE (file contents), or
    - AIFO_OTEL_ENDPOINT (env var).
  - Runtime env `OTEL_EXPORTER_OTLP_ENDPOINT` remains the highest-priority override.

Enablement & features (clarification rather than change):

- v9:
  - Telemetry code is still compile-time optional via features `otel` and `otel-otlp`.
  - Internal builds enable telemetry by default via CARGO_FLAGS.
  - Runtime defaults: enabled-by-default when compiled with telemetry and AIFO_CODER_OTEL is unset.
- v10:
  - Same compile-time feature model:
    - `otel` and `otel-otlp` gate telemetry code.
    - Internal builds (Makefile, wrapper, CI) default to `--features otel-otlp`.
    - Developers can build without telemetry by omitting otel* features.
  - The spec explicitly distinguishes:
    - “telemetry compiled in” (feature-enabled builds),
    - “telemetry compiled out” (no otel features),
    - and says runtime telemetry config is ignored when telemetry is compiled out.

Makefile & wrapper defaults:

- v9:
  - Intended: binary owns defaults; Makefile/wrapper no longer hard-set runtime OTEL envs.
- v10:
  - Confirms this as a requirement:
    - Makefile and `aifo-coder` wrapper **must not** set AIFO_CODER_OTEL or OTEL_EXPORTER_OTLP_ENDPOINT by default.
    - They may pass CARGO_FLAGS to enable features, and may set OTEL envs explicitly for specific jobs.

Behavioral invariants from v8/v9 are retained:

- No telemetry writes to stdout.
- Telemetry is best-effort; never changes exit codes.
- PII defaults unchanged: hashed cwd/args unless AIFO_CODER_OTEL_PII=1.
- fmt layer remains opt-in via AIFO_CODER_TRACING_FMT.

1) Goals

- Provide a robust, layered default OTLP endpoint model:
  - Local example default for open-source / developer builds.
  - Optional internal override via build.rs without committing secrets or internal URLs.
  - Runtime env overrides that always win over baked-in defaults.
- Maintain v9’s runtime behavior:
  - When built with telemetry features, telemetry is enabled by default (AIFO_CODER_OTEL unset).
  - AIFO_CODER_OTEL fully disables telemetry when falsy.
- Keep telemetry compile-time-optional via features for developers:
  - Internal builds and CI still enable telemetry by default via CARGO_FLAGS.
- Preserve stdout invariants and safety guarantees:
  - No telemetry output to stdout.
  - No exit-code changes or panics due to telemetry.

2) Non-goals

- v10 does not:
  - Change the set of telemetry instruments or spans.
  - Change PII semantics or metric cardinalities.
  - Force telemetry to be compiled in for all builds; otel features remain optional.
  - Introduce new CLI flags; configuration stays environment-only.

3) Compile-time model (features)

Dependencies in Cargo.toml (unchanged from v9):

- tracing = { version = "0.1", features = ["std"], optional = true }
- tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt"], optional = true }
- opentelemetry = { version = "0.24", optional = true }
- opentelemetry_sdk = { version = "0.24", features = ["rt-tokio"], optional = true }
- tracing-opentelemetry = { version = "0.25", optional = true }
- opentelemetry-stdout = { version = "0.5", optional = true }
- opentelemetry-otlp = { version = "0.17", features = ["grpc-tonic"], optional = true }
- tokio = { version = "1", features = ["rt-multi-thread"], optional = true }
- hostname = { version = "0.3", optional = true }
- humantime = "2.1"
- once_cell = "1"

Features:

- `otel` enables tracing and development exporters:
  - "tracing"
  - "tracing-subscriber"
  - "opentelemetry"
  - "opentelemetry_sdk"
  - "tracing-opentelemetry"
  - "opentelemetry-stdout"
  - "hostname"
  - "humantime"
- `otel-otlp` adds OTLP exporter and Tokio runtime:
  - "otel"
  - "opentelemetry-otlp"
  - "tokio"

Default build modes:

- Internal (Makefile/wrapper/CI) default:
  - `CARGO_FLAGS ?= --features otel-otlp`
  - Telemetry code is compiled in and OTLP is available.
- Developer builds:
  - With telemetry:
    - `cargo build --features otel-otlp`
  - Without telemetry:
    - `cargo build` (no `--features otel*`), which compiles out telemetry code and provides a no-op `telemetry_init()` stub.

4) Runtime configuration and defaults

All runtime configuration described here applies only when telemetry is compiled in (i.e., built with `otel` / `otel-otlp`). When telemetry is compiled out, telemetry_init() is a no-op and all OTEL env settings are ignored.

4.1 Enablement (unchanged from v9)

- AIFO_CODER_OTEL:
  - Unset: telemetry **enabled by default**.
  - Truthy (“1”, “true”, “yes”, case-insensitive): telemetry enabled.
  - Falsy (“0”, “false”, “no”, “off”, case-insensitive): telemetry disabled; telemetry_init() returns None with no side effects.

- Effective behavior:
  - Default (no env): telemetry ON.
  - `AIFO_CODER_OTEL=0`: telemetry OFF.
  - `AIFO_CODER_OTEL=1`: telemetry ON.

4.2 OTLP endpoint selection (layered defaults)

The OTLP endpoint is selected in three layers:

1. Runtime env override (highest priority):
   - OTEL_EXPORTER_OTLP_ENDPOINT
     - If set and non-empty: used as the OTLP/gRPC endpoint.

2. Build-time baked-in default (optional):
   - At build time, `build.rs` may set a compile-time environment:
     - AIFO_OTEL_DEFAULT_ENDPOINT
   - If present, this defines the baked-in default OTLP endpoint for that binary.

3. Code default (fallback):
   - If neither the runtime OTEL_EXPORTER_OTLP_ENDPOINT nor AIFO_OTEL_DEFAULT_ENDPOINT is set, use a safe example endpoint:
     - `"http://localhost:4317"`.

The effective endpoint function in telemetry.rs:

```rust
fn effective_otlp_endpoint() -> Option<String> {
    if let Ok(v) = env::var("OTEL_EXPORTER_OTLP_ENDPOINT") {
        let t = v.trim();
        if !t.is_empty() {
            return Some(t.to_string());
        }
    }

    // Compile-time baked-in default from build.rs (if present)
    let default = match option_env!("AIFO_OTEL_DEFAULT_ENDPOINT") {
        Some(v) if !v.trim().is_empty() => v.trim(),
        _ => "http://localhost:4317",
    };

    Some(default.to_string())
}
```

4.3 Build-time endpoint injection via build.rs (new in v10)

`build.rs` is extended to optionally bake in a default OTLP endpoint from external configuration:

- Inputs (evaluated at build time):
  - AIFO_OTEL_ENDPOINT_FILE:
    - Path to a file containing the endpoint URL as text (single line or first line used).
  - AIFO_OTEL_ENDPOINT:
    - Endpoint URL as an environment variable.

- Priority and behavior:

```rust
if let Ok(path) = std::env::var("AIFO_OTEL_ENDPOINT_FILE") {
    if let Ok(contents) = std::fs::read_to_string(&path) {
        let trimmed = contents.trim();
        if !trimmed.is_empty() {
            println!("cargo:rustc-env=AIFO_OTEL_DEFAULT_ENDPOINT={trimmed}");
        }
    }
} else if let Ok(val) = std::env::var("AIFO_OTEL_ENDPOINT") {
    let trimmed = val.trim();
    if !trimmed.is_empty() {
        println!("cargo:rustc-env=AIFO_OTEL_DEFAULT_ENDPOINT={trimmed}");
    }
}
```

- Failure handling:
  - Missing file, unreadable file, or empty values are treated as “no override”.
  - No panics; builds always succeed regardless of these settings.

- Typical internal setup:
  - CI or internal build scripts set AIFO_OTEL_ENDPOINT or AIFO_OTEL_ENDPOINT_FILE to point to the corporate collector, e.g.:
    - `http://alloy-collector-az.service.dev.migros.cloud`.

4.4 Other runtime envs (unchanged from v9)

- OTEL_EXPORTER_OTLP_TIMEOUT:
  - Default “5s”; respected by OTLP tracer and metrics exporters.

- OTEL_BSP_*:
  - OTEL_BSP_SCHEDULE_DELAY (default 2s)
  - OTEL_BSP_MAX_QUEUE_SIZE (default 2048)
  - OTEL_BSP_EXPORT_TIMEOUT (default 5s)

- OTEL_TRACES_SAMPLER / OTEL_TRACES_SAMPLER_ARG:
  - Standard sampler configuration; default `parentbased_always_on`.

- AIFO_CODER_TRACING_FMT:
  - “0” (or unset): fmt layer not installed (no extra stderr logs).
  - “1”: fmt layer installed; RUST_LOG controls EnvFilter; default filter “warn”.

- AIFO_CODER_OTEL_VERBOSE:
  - When “1”, telemetry initialization prints concise lines to stderr describing:
    - whether OTLP/exporters are enabled,
    - which endpoint is used,
    - whether metrics are enabled and which sink is used.
  - main sets this when `--verbose` is passed.

- AIFO_CODER_OTEL_METRICS / AIFO_CODER_OTEL_METRICS_FILE:
  - As in v8/v9; metrics opt-in via AIFO_CODER_OTEL_METRICS=1.
  - Dev exporter uses stderr or a JSONL file sink; never stdout.

- AIFO_CODER_OTEL_PII:
  - “1” => raw cwd/args allowed (debug-only).
  - Else => hashed/salted; no raw PII in spans.

5) Initialization design (v10)

The telemetry initialization design is the same as v9 with the new endpoint layering and unchanged enablement:

- `pub fn telemetry_init() -> Option<TelemetryGuard>`:
  - Checks OnceCell INIT to ensure idempotence.
  - Checks `telemetry_enabled_env()` (default ON, AIFO_CODER_OTEL opt-out).
  - Sets TraceContext propagator.
  - Computes a Resource with:
    - service.name, service.version, service.namespace,
    - service.instance.id, process.pid, host.name, os.type, process.executable.name,
    - optional deployment.environment.
  - Calls `effective_otlp_endpoint()` for use_otlp decision.
  - Configures:
    - OTLP tracer (batch, private Tokio runtime) when `otel-otlp` compiled, and endpoint available.
    - Fallback stderr tracer exporter when OTLP init fails or feature missing.
    - Metrics provider when AIFO_CODER_OTEL_METRICS=1.
  - Installs tracing_subscriber layers:
    - always tracing_opentelemetry layer,
    - fmt layer only when AIFO_CODER_TRACING_FMT=1.

Error handling:

- Any init failure:
  - Emits at most one concise warning to stderr when AIFO_CODER_OTEL_VERBOSE=1.
  - Returns None; never panics; never changes exit codes.

6) Instrumentation and metrics (unchanged semantics)

All span instrumentation and metrics behavior from v8/v9 is unchanged:

- Spans:
  - `build_docker_cmd`, `toolchain_start_session`, `toolchain_run`, `toolexec_start_proxy` (and inner requests), registry probe functions, lock acquisition, AppArmor helpers, cache purge, bootstrap, etc.
  - PII controls via AIFO_CODER_OTEL_PII.
  - Span status set via OpenTelemetrySpanExt on errors/timeouts.

- Metrics:
  - Counters:
    - aifo_runs_total{agent}
    - docker_invocations_total{kind}
    - proxy_requests_total{tool,result}
    - toolchain_sidecars_started_total{kind}
    - toolchain_sidecars_stopped_total{kind}
  - Histograms:
    - docker_run_duration{agent} (s)
    - proxy_exec_duration{tool} (s)
    - registry_probe_duration{source=curl|tcp} (s)
  - Low-cardinality labels only; no PII.

7) Makefile, wrapper and CI behavior

- Makefile:
  - CARGO_FLAGS ?= --features otel-otlp
  - Must not export AIFO_CODER_OTEL or OTEL_EXPORTER_OTLP_ENDPOINT by default.
  - May set AIFO_OTEL_ENDPOINT or AIFO_OTEL_ENDPOINT_FILE for specific build jobs if desired.

- `aifo-coder` wrapper:
  - Must not set AIFO_CODER_OTEL or OTEL_EXPORTER_OTLP_ENDPOINT by default.
  - Its responsibility is:
    - to detect and prefer an installed binary on PATH when appropriate,
    - to build the Rust launcher with CARGO_FLAGS if the local binary is out of date,
    - to exec the binary with the caller’s environment.

- CI (`ci/telemetry-smoke.sh`):
  - Builds with `--features otel`.
  - Compares stdout with telemetry default ON vs AIFO_CODER_OTEL=0.
  - Smoke runs with AIFO_CODER_OTEL_METRICS=1.

8) Performance and safety (unchanged)

- Telemetry is enabled by default when compiled with otel* features and AIFO_CODER_OTEL is unset.
- Telemetry is off when:
  - built without telemetry features, or
  - AIFO_CODER_OTEL is explicitly falsy.
- Default exporter timeouts and BSP settings remain tuned for CLI usage:
  - OTEL_EXPORTER_OTLP_TIMEOUT=5s,
  - OTEL_BSP_SCHEDULE_DELAY=2s,
  - OTEL_BSP_MAX_QUEUE_SIZE=2048,
  - OTEL_BSP_EXPORT_TIMEOUT=5s.
- fmt layer remains opt-in to avoid noise on stderr.

9) Testing and acceptance criteria (v10)

Tests must cover:

- Build-time endpoint override:
  - With no AIFO_OTEL_ENDPOINT* at build time:
    - `telemetry_init()` must log (under verbose) that the endpoint is `http://localhost:4317` when OTEL_EXPORTER_OTLP_ENDPOINT unset.
  - With AIFO_OTEL_ENDPOINT set at build time:
    - Default run (no runtime endpoint) must use that baked-in endpoint.
  - With both baked-in and runtime OTEL_EXPORTER_OTLP_ENDPOINT:
    - Runtime endpoint must win.

- Stdout invariance:
  - `cargo run --features otel -- --help` stdout must match `AIFO_CODER_OTEL=0 cargo run --features otel -- --help`.

- Opt-out:
  - `AIFO_CODER_OTEL=0` must disable telemetry regardless of baked-in defaults.

- Compile-time off:
  - Build without otel features; `telemetry_init()` must be a no-op stub; env settings should not cause panics or behavior changes.

10) Migration guide: v9 → v10

This section outlines the minimal changes required to move from a v9-compatible implementation to v10.

1. `build.rs`:

   - Add build-time endpoint injection:

     ```rust
     // At end of main():
     if let Ok(path) = std::env::var("AIFO_OTEL_ENDPOINT_FILE") {
         if let Ok(contents) = std::fs::read_to_string(&path) {
             let trimmed = contents.trim();
             if !trimmed.is_empty() {
                 println!("cargo:rustc-env=AIFO_OTEL_DEFAULT_ENDPOINT={trimmed}");
             }
         }
     } else if let Ok(val) = std::env::var("AIFO_OTEL_ENDPOINT") {
         let trimmed = val.trim();
         if !trimmed.is_empty() {
             println!("cargo:rustc-env=AIFO_OTEL_DEFAULT_ENDPOINT={trimmed}");
         }
     }
     ```

2. `src/telemetry.rs`:

   - Replace the hard-coded DEFAULT_OTLP_ENDPOINT string with:

     ```rust
     const DEFAULT_OTLP_ENDPOINT: &str = match option_env!("AIFO_OTEL_DEFAULT_ENDPOINT") {
         Some(v) if !v.trim().is_empty() => v.trim(),
         _ => "http://localhost:4317",
     };
     ```

   - Ensure `effective_otlp_endpoint()` uses DEFAULT_OTLP_ENDPOINT as described in section 4.2.

3. Spec and documentation:

   - Update v9 references to “default OTLP endpoint = alloy collector” to state:
     - Code default is `http://localhost:4317`.
     - Internal deployments can bake in `http://alloy-collector-az.service.dev.migros.cloud` or other endpoints via AIFO_OTEL_ENDPOINT*.
   - Update README telemetry section to describe:
     - the three-layer endpoint precedence,
     - how to configure internal builds via build.rs envs.

4. Makefile and wrapper:

   - Confirm that:
     - Makefile does not export AIFO_CODER_OTEL / OTEL_EXPORTER_OTLP_ENDPOINT by default.
     - `aifo-coder` wrapper does not set OTEL env defaults, only CARGO_FLAGS.

5. CI:

   - Optionally set AIFO_OTEL_ENDPOINT or AIFO_OTEL_ENDPOINT_FILE in CI build jobs that produce internal binaries, to bake in the corporate collector endpoint.
   - Keep otel-golden-stdout.sh as is (it already tests env-driven behavior, independent of baked-in defaults).

6. Validation:

   - Run `make check` (or the CI test pipeline) in three configurations:
     1. Built with otel* features, no AIFO_OTEL_ENDPOINT* → default endpoint is localhost.
     2. Built with otel* features, AIFO_OTEL_ENDPOINT set to the corporate collector → default endpoint uses that value.
     3. Built without otel features → telemetry_init() is a stub; env OTEL_* must not affect behavior.

This concludes the v10 specification: telemetry remains compile-time optional but is enabled by default in feature-enabled builds, and the OTLP endpoint now has a clean, layered configuration model: runtime env > build-time baked-in > code default.
