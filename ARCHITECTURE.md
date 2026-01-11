# Architecture Assessment & Evolution

## 1. System Characterization
- System type: Rust CLI orchestrating containerized coding agents and toolchain sidecars; entrypoint at src/main.rs with docker run assembly in src/docker/run.rs.
- Purpose: run Codex/Crush/Aider/OpenHands/OpenCode/Plandex/Letta in isolated containers with predictable mounts, env, and optional toolchain proxying. (README.md, src/cli.rs)
- Problem domain: secure developer tooling/agent UX across Linux/macOS/Windows via Docker-based isolation and optional AppArmor. (README.md, docs/README-security-architecture.md)
- Runtime/deployment: depends on Docker CLI; uses docker run/exec, named volumes, session networks, shim-first PATH, and optional AppArmor profiles. (src/docker/runtime.rs, src/docker/run.rs)
- Persistence: binds /workspace plus curated config/state mounts (.gitconfig, aider/codex/opencode dirs) and toolchain caches/volumes per language. (src/docker/run.rs, src/toolchain/mounts.rs)
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
- Failure propagation: docker availability errors bubble early; proxy/shim failures abort tool exec; network isolation failures currently downgrade to bridge silently; toolchain startup errors can leave residual sidecars. (src/main.rs, src/toolchain/sidecar.rs, src/toolchain_session.rs)
- Change safety: extensive nextest suites cover registry resolution, proxy semantics, fork flows, and toolchain routing; E2E tests gated by docker presence. (tests/*)

## 6. Structural Strengths
- Clear separation between CLI orchestration, docker run composition, toolchain/proxy logic, and fork workflows.
- Aggressive mount validation and limited env forwarding reduce accidental host exposure. (src/docker/mounts.rs, src/docker/env.rs)
- Toolchain proxy defaults are safer: loopback bind, bounded connection count, max-runtime escalation. (src/toolchain/proxy.rs)
- Rich automated tests across unit/int/e2e lanes with deterministic plans for registry/proxy behavior. (docs/README-testing.md, tests/TEST_PLAN.md)
- Optional defense-in-depth via AppArmor selection and UID/GID mapping to avoid root-owned files. (src/docker/run.rs, src/apparmor.rs)
- Reusable shell builders and staging helpers reduce ad-hoc shell injection risks. (src/util/shell_script.rs, src/docker/run.rs)

## 7. Structural Liabilities
- Network isolation quietly downgrades to bridge when creation/inspection fails, reducing containment with no user-visible error. (src/toolchain/sidecar.rs:850-874)
- Toolchain session startup errors return early without rolling back already-started sidecars, leaving containers/networks running on partial failure. (src/toolchain/sidecar.rs:1077-1188; src/toolchain_session.rs:520-545)
- Proxy streaming uses blocking writes with no write deadlines; a slow/paused client can stall a worker thread until the socket drains despite the bounded channel, reducing concurrency headroom. (src/toolchain/proxy.rs:575-616, 1845-1995)
- Letta agent support exists in CLI but is undocumented in README/feature lists, creating discoverability and support gaps. (src/cli.rs:87-140; README.md)

## 8. Architectural Direction (Concrete Refactorings)
- Fail closed on network isolation: treat missing/failed network creation as an error (not a silent bridge fallback), surface a warning, and add an integration test that asserts isolation nets exist. (addresses liability 1)
- Add rollback for toolchain_start_session: track started sidecars/networks and stop/remove them when subsequent startups fail; cover with unit/int tests. (addresses liability 2)
- Add proxy write deadlines (or nonblocking with poll) for streaming responses so stalled clients release worker threads; emit verbose diagnostics and tests for slow-consumer behavior. (addresses liability 3)
- Document Letta agent support alongside other agents and align README feature list with CLI surface; add a short smoke example. (addresses liability 4)

## 9. Architectural Guardrails
- Treat isolation nets as mandatory when requested; fail fast on creation errors and avoid silent downgrades.
- Roll back sidecars on startup failure and keep session-scoped cleanup idempotent.
- Keep proxy listeners loopback/UDS-first with bounded connection counts and timeouts on both read and write paths.
- Maintain audited env/mount allowlists; add regression tests whenever expanding forwarded env or config staging.
- Keep README/CLI/docs aligned when adding agents or flags to avoid support drift.

## 10. Evidence Appendix
- CLI orchestration and warnings: src/main.rs
- Docker run/env/mount policy and PATH/shim wiring: src/docker/run.rs, src/docker/env.rs, src/docker/mounts.rs
- Toolchain session, proxy, HTTP parsing, and sidecar networking: src/toolchain_session.rs; src/toolchain/proxy.rs; src/toolchain/http.rs; src/toolchain/sidecar.rs
- Shim behavior and smart routing: src/bin/aifo-shim.rs
- Fork orchestration and tmux launch scripts: src/fork/runner.rs; src/fork/inner.rs; src/fork_impl/*
- Testing strategy and coverage signals: docs/README-testing.md; tests/TEST_PLAN.md; tests/int_* and tests/e2e_* suites
