# Architecture Assessment & Evolution

## 1. System Characterization
- System type: Rust CLI orchestrating containerized coding agents and toolchain sidecars; entrypoint at src/main.rs with docker run orchestration in src/docker/run.rs.
- Purpose: run Codex/Crush/Aider/OpenHands/OpenCode/Plandex in isolated containers with predictable mounts, env, and optional toolchain proxying. (README.md)
- Problem domain: secure developer tooling/agent UX across Linux/macOS/Windows with Docker-based isolation and optional AppArmor. (README.md, docs/README-security-architecture.md)
- Runtime/deployment: depends on Docker CLI; uses docker run/exec, named volumes, session networks, shim-first PATH, and optional AppArmor profiles. (src/docker/runtime.rs, src/docker/run.rs)
- Persistence: binds /workspace plus curated config/state mounts (.gitconfig, aider/codex/opencode dirs) and toolchain caches/volumes for languages. (src/docker/run.rs, src/toolchain/mounts.rs)
- Integration points: registry resolution, OpenAI/Gemini/Azure env forwarding, GPG signing hints, host notifications command, pnpm migration helper. (src/docker/env.rs, src/main.rs, src/toolchain/notifications.rs, src/toolchain_session.rs)
- Users/use cases: developers needing reproducible agent runs and forked experiments; fork mode clones panes with isolated state. (README.md, src/fork/runner.rs)

## 2. Architectural Overview (As Implemented)
- CLI pipeline: clap-driven parsing applies global flags (color, flavor, cache, verbose), configures session/network ids, warns on missing LLM creds, and dispatches to agent/fork/toolchain/support/doctor commands. (src/main.rs)
- Agent execution: docker/run.rs builds docker run invocations (env whitelist, mount policy, UID/GID mapping, AppArmor flags, network selection) and shells into agent entrypoints with shim-first PATH. (src/docker/run.rs)
- Toolchain sidecars/proxy: toolchain_session.rs starts sidecar containers, optional bootstrap, then toolexec proxy; proxy dispatches shim requests with auth tokens, streaming/exec semantics, signal forwarding, and workspace diagnostics. (src/toolchain_session.rs, src/toolchain/proxy.rs)
- Shim layer: aifo-shim binary routes tools inside agent images to the proxy, handles env trampolines, smart local node/python fallbacks, and signal propagation to PGIDs. (src/bin/aifo-shim.rs)
- Fork orchestration: fork runner builds per-pane clones/branches, tmux launch scripts, and maintenance commands (list/clean/merge) with consistent env exports and cleanup policies. (src/fork/runner.rs, src/fork_impl/*, src/fork/inner.rs)
- Registry/config helpers: registry.rs manages internal/mirror prefix resolution with caching; docker/env.rs forwards curated env plus AIFO_ENV_*; docker/mounts.rs validates user mounts; docker/staging.rs cleans staging dirs. (src/registry.rs, src/docker/env.rs, src/docker/mounts.rs, src/docker/staging.rs)
- Telemetry/doctor/support: telemetry.rs provides optional OTEL; doctor/support commands emit diagnostics on docker, git identity, editor presence, and stale fork sessions. (src/telemetry.rs, src/doctor.rs, src/support.rs)

## 3. Core Concepts & Abstractions
- Session identity: create_session_id + env exports (AIFO_CODER_FORK_SESSION, AIFO_SESSION_NETWORK) standardize container names, networks, and pane metadata. (src/main.rs, src/toolchain/sidecar.rs)
- ToolchainSession RAII: encapsulates sidecar startup, proxy bootstrap, env export (AIFO_TOOLEEXEC_URL/TOKEN), and cleanup on Drop unless in fork panes. (src/toolchain_session.rs)
- Proxy contract: HTTP endpoints /exec, /notify, /signal with bearer token auth, v1/v2 streaming, ExecId registry, signal forwarding, optional timeout/escalation; request model in HttpRequest/Endpoint. (src/toolchain/proxy.rs, src/toolchain/http.rs)
- Shim abstractions: env trampoline parsing, smart shim selection, signal handling and PGID tracking, notification tool allowlist. (src/bin/aifo-shim.rs)
- Shell builders: ShellScript and ShellFile builders compose docker exec scripts, tmux launchers, and bootstrap helpers safely. (src/util/shell_script.rs, src/util/shell_file.rs)
- Fork model: ForkCleanOpts/ForkCmd drive list/clean/merge flows; pane env helpers and tmux script generation isolate user state per pane. (src/fork_args.rs, src/fork/inner.rs)

## 4. Dependency & Coupling Analysis
- External dependencies: Docker CLI required for all agent runs; proxy/shim assume curl, POSIX shell; optional AppArmor; OTEL feature gated. (src/docker/runtime.rs, src/toolchain/proxy.rs)
- Internal coupling: main.rs depends on registry, docker run builder, toolchain_session, fork orchestrator; proxy/shim coupling to env variables exported by ToolchainSession; fork helpers rely on git commands and tmux presence.
- Configuration surface: heavy reliance on env vars for registry overrides, proxy behavior, toolchain images, network selection, telemetry toggles. (src/docker/env.rs, src/toolchain/env.rs)
- Data flow: workspace and config mounts flow through docker/run.rs; shim/proxy exchange auth token via env; fork metadata stored in env and filesystem under .aifo-coder/. (src/docker/run.rs, src/bin/aifo-shim.rs, src/fork/runner.rs)
- Boundary enforcement: mount validation and size caps for staged configs; env allowlist; proxy authorization; AppArmor selection when supported. (src/docker/mounts.rs, src/docker/env.rs, src/toolchain/proxy.rs)

## 5. Change & Evolution Analysis
- Stable seams: Shell builders, registry resolution, mount policy helpers, and color/logging helpers are reused across commands. (src/util/*, src/registry.rs, src/docker/mounts.rs)
- Volatile areas: proxy/shim protocol (streaming, signals), toolchain routing/bootstraps, docker run env/mount policy, fork UX. (src/toolchain/proxy.rs, src/bin/aifo-shim.rs, src/docker/run.rs, src/fork/runner.rs)
- Failure propagation: docker availability errors bubble early; proxy/shim failures directly abort tool exec; network isolation misconfiguration surfaces as docker run failures; fork merge errors propagated with colorized messages. (src/main.rs, src/toolchain/proxy.rs, src/fork_impl/merge.rs)
- Change safety: extensive nextest suites cover registry resolution, proxy semantics, fork flows, and toolchain routing; E2E tests gated by docker presence. (tests/*)

## 6. Structural Strengths
- Clear separation between CLI orchestration, docker run composition, toolchain/proxy logic, and fork workflows.
- Aggressive mount validation and limited env forwarding reduce accidental host exposure. (src/docker/mounts.rs, src/docker/env.rs)
- Rich automated tests across unit/int/e2e lanes with deterministic plans for registry/proxy behavior. (docs/README-testing.md, tests/TEST_PLAN.md)
- Optional defense-in-depth via AppArmor selection and UID/GID mapping to avoid root-owned files. (src/docker/run.rs, src/apparmor.rs)
- Reusable shell builders and staging helpers reduce ad-hoc shell injection risks. (src/util/shell_script.rs, src/docker/run.rs)

## 7. Structural Liabilities
- Toolchain proxy binds 0.0.0.0 on Linux by default and lacks listener scoping, exposing the exec API beyond the host when docker sidecars are up. (src/toolchain/proxy.rs:697-735)
- Proxy connection handling is unbounded: each accept spawns a thread with no concurrency cap and default infinite read timeouts, enabling trivial slow-connection DoS. (src/toolchain/proxy.rs:520-620, 697-770)
- Chunked request parsing reads declared chunk sizes into memory without bounding the chunk size, allowing oversized chunk headers to drive large allocations despite a 1 MiB body cap. (src/toolchain/http.rs:64-189)
- CLI network isolation flag sets AIFO_SESSION_NETWORK for agent runs but never creates the network when toolchains are disabled, causing docker run failures and inconsistent behavior. (src/main.rs:84-130, src/docker/run.rs:1155-1182)

## 8. Architectural Direction (Concrete Refactorings)
- Default the proxy to loopback/UDS and require explicit opt-in for 0.0.0.0; add bind-host CLI/env and tests covering TCP vs UDS bindings. (fixes liability 1)
- Add connection acceptance limits, pooled worker model, and sane read/write timeouts to the proxy to prevent slowloris/connection floods; codify in int/e2e tests. (fixes liability 2)
- Reject chunked requests with per-chunk caps (<= BODY_CAP) before buffering; enforce total read caps and simplify draining to avoid large allocations. (fixes liability 3)
- Ensure --docker-network-isolate creates/cleans the session network even without toolchains (or disallow the flag in that mode); add docker run preview validation. (fixes liability 4)

## 9. Architectural Guardrails
- Bind proxy sockets to loopback or UDS by default; expose host-facing TCP only when explicitly configured and documented.
- Enforce bounded connection counts, per-connection timeouts, and body/chunk size limits on proxy endpoints.
- Treat network isolation as an atomic feature: create/manage networks alongside agent runs and clean them deterministically.
- Keep env/mount allowlists audited; add regression tests when expanding forwarded env or config staging.
- Preserve deterministic logs for registry/proxy/fork flows to keep tests and telemetry stable.

## 10. Evidence Appendix
- CLI orchestration and warnings: src/main.rs
- Docker run/env/mount policy and PATH/shim wiring: src/docker/run.rs, src/docker/env.rs, src/docker/mounts.rs
- Toolchain session, proxy, HTTP parsing, and sidecar networking: src/toolchain_session.rs; src/toolchain/proxy.rs; src/toolchain/http.rs; src/toolchain/sidecar.rs
- Shim behavior and smart routing: src/bin/aifo-shim.rs
- Fork orchestration and tmux launch scripts: src/fork/runner.rs; src/fork/inner.rs; src/fork_impl/*
- Testing strategy and coverage signals: docs/README-testing.md; tests/TEST_PLAN.md; tests/int_* and tests/e2e_* suites
