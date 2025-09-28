Here’s a focused assessment of your current test coverage, the key gaps and risks, and a concrete plan of new and improved tests to strengthen the harness for a coding agent where consistent coverage is crucial.

What’s already covered well

 • Fork lifecycle:
    • Repo detection, branch/label sanitize, snapshot, clone (including submodules and LFS markers), merge strategies (fetch, octopus), metadata
      writes/updates, stale notice and autoclean, color and plain rendering, CLI subcommands and JSON outputs.
    • Safety protections and prompts: dirty/ahead/base-unknown refusal paths, keep-dirty/force behaviors, planning JSON and executed JSON summaries.
    • Locking: per-repo hashed runtime path, CWD and temp path order, concurrent exclusive lock behavior.
 • Toolchains, shims and previews:
    • Docker run/exec command previews across agents and toolchains; mounts and env passes; PATH and user flags; cache volumes (Rust named volumes ownership
      with stamps), node/tsc and Python venv precedence.
    • Image selection (agent images, registry prefix envs and probes; rust, node, python, go toolchain images).
    • Shims: script writer produces aifo-shim and tool wrappers; basic runtime failure when env missing; wrappers auto-exit behavior visible in acceptance
      tests.
 • Proxy/server:
    • Multiple acceptance paths for TCP and UDS streaming (proto v2), header case handling, LF-only header terminator, signal forwarding endpoint,
      concurrency stress (mixed 401/426), disconnect logging and cleanup, spawn-failure plain 500 semantics.
    • Notifications policy: absolute exec path enforcement, allowlist, trailing {args} policy, args mismatch, max args truncation, noauth path basic
      behavior; verbose parsed/result logs teeing.
 • Doctor and banner:
    • CLI doctor prints registry and security options; editors inventory and workspace writability when images are present; AppArmor portability and
      env-driven fallbacks; cached registry invalidation.
 • Utilities:
    • Shell escaping/join, JSON escaping, header finding, URL decoding; color precedence (CLI/env/NO_COLOR); registry probe override modes and quiet probe
      behavior.

Key correctness gaps and risks

 1 Buffered exec (proto v1) timeout HTTP status mismatch

 • Current code returns 200 OK with X-Exit-Code 124 after watcher kills the child on timeout (src/toolchain/proxy.rs).
 • Tests expect 504 Gateway Timeout for v1 timeouts (tests/proxy_error_semantics.rs, tests/proxy_timeout.rs). Some are not ignored and will fail when images
   are present.
 • Impact: inconsistent API semantics and failing test lanes once docker is available.

 2 HTTP parser guardrails not enforced

 • src/toolchain/http.rs has no header count limit and no Content-Length sanity checks; RFC-contradictory cases (both TE: chunked and Content-Length) are not
   explicitly resolved.
 • Current tests only exercise LF-terminator tolerance and endpoint classification. No tests for:
    • Excessive headers → 431
    • Content-Length mismatch (less than bytes present) → 400
    • TE: chunked plus Content-Length (prefer chunked)

 3 Streaming backpressure and slow clients

 • Unbounded stdout channel (std::sync::mpsc) may grow if client stops reading; graceful disconnect is covered, but no tests simulate long-running,
   slow-consumer behavior to assert proper escalation/log sequence under sustained output.

 4 Notifications spawn error path

 • Policy and mismatch paths are covered, but no test deliberately triggers ExecSpawn errors (absolute path pointing to a non-executable or non-existent
   basename allowed by allowlist) to assert 500 mapping and message.

 5 Env forwarding blocks

 • apply_passthrough_envs blocks PROHIBITED_PASSTHROUGH_ENV (RUSTUP_TOOLCHAIN, RUSTUP_HOME, CARGO_HOME, CARGO_TARGET_DIR). There are no tests asserting the
   block (vs allowed proxy envs), especially when host envs are set.

 6 Official rust image helpers

 • is_official_rust_image and official_rust_image_for_version helpers aren’t directly unit-tested (e.g., registry host:port prefixes, path segments).

 7 Node cache ownership stamps

 • Rust named volumes have a stamp-file E2E test; consolidated Node cache volume ownership init is not validated with a stamp-file E2E test.

 8 Shim native client coverage

 • Most shim coverage focuses on writer scripts and env-missing exits. The compiled Rust shim’s native HTTP corner cases (chunk extensions, trailer parsing,
   disconnect wait mapping, signal-exit code mapping) are not unit-tested. Acceptance tests cover some paths, but targeted unit tests would harden behavior.

  9 Duplication across tests

 • Many tests reimplement URL→port extraction and raw HTTP request rendering. You already have tests/support helpers; several files still duplicate logic.
   Consolidation reduces drift and flakiness.

Pragmatic test additions and improvements Prioritize low-risk, high-value tests. Where behavior changes are required, keep them behind #[ignore] or guard
with env until the implementation lands.

A) Fix and validate v1 timeout 504 semantics

 • Brief change: in src/toolchain/proxy.rs v1 path, after child.wait() when the watcher has timed_out, return “504 Gateway Timeout” and X-Exit-Code: 124.
   Keep 200 OK for normal completion and 500 for spawn errors.
 • Tests:
    • Unignore or add a small test mirroring tests/proxy_timeout.rs but guarded by docker image presence. Name: proxy_v1_timeout_status_504. Validate status
      line contains “504 Gateway Timeout” and “X-Exit-Code: 124”.
    • Keep current v2 streaming behavior intact (trailers: 124) to avoid regressions.

B) HTTP parser hardening tests (add now; implement soon)

 • Add unit tests in src/toolchain/http.rs:
    • Too many headers (e.g., 1,200 lines) -> expect read_http_request to return an error; map to 431 in proxy tests. Write a proxy-level integration test:
      send >1024 headers to /exec; expect “431 Request Header Fields Too Large” and X-Exit-Code: 86. Mark #[ignore] until implementation.
    • Content-Length smaller than pre-read body bytes -> expect 400; write proxy test “http_content_length_mismatch_400”. Mark #[ignore] until implemented.
    • Both TE: chunked and Content-Length present; ensure chunked is preferred and body is de-chunked; add a unit test in http.rs feeding a short chunked
      body with a conflicting Content-Length; assert decoded body equals chunk payload.

C) Streaming slow-consumer and backpressure observation

 • Add an integration test “proxy_streaming_slow_consumer_disconnect”:
    • Start a sidecar and proxy; POST a command that writes steadily (e.g., python -c ‘import time; [print("x"*8192); time.sleep(0.05)] in a loop’).
    • Delay reads intentionally; then close the socket; assert disconnect log appears and that the proxy performs INT→TERM→KILL sequence (logs present). Keep
      volume and runtime short to avoid flakiness.
 • This exercises write failure path more aggressively than accept_disconnect.

D) Notifications spawn error mapping

 • Add “notifications_exec_spawn_error_500”:
    • Config: absolute path pointing to a non-existent say stub (basename ‘say’, on allowlist).
    • Request /notify under noauth with expected args.
    • Expect 500 Internal Server Error and X-Exit-Code: 86 with the “host 'say' execution failed:” message.

E) Env forwarding block tests

 • Add “toolchain_env_blocklist_not_forwarded”:
    • Set RUSTUP_TOOLCHAIN=nightly, RUSTUP_HOME=… CARGO_HOME=… CARGO_TARGET_DIR=… on host.
    • Build rust sidecar run and exec previews.
    • Assert none of these blocked envs are present; assert normative replacements (RUSTUP_HOME=/usr/local/rustup etc.) are present.
 • Complement existing tests that assert proxy/cargo networking envs are passed.

F) Official rust image helper tests

 • Add unit tests:
    • is_official_rust_image("rust:1.80-slim") → true
    • is_official_rust_image("registry.local:5000/rust:1.80") → true
    • is_official_rust_image("registry/rust-toolchain:1.80") → false
    • official_rust_image_for_version(None) defaults to “rust:1.80-bookworm”; for Some("1.79") returns “rust:1.79-bookworm”.

G) Node cache ownership stamp E2E (ignored by default)

 • Add “node_named_cache_ownership_stamp_files”:
    • Enable named cache (“AIFO_TOOLCHAIN_RUST_USE_DOCKER_VOLUMES” is for rust; Node uses consolidated cache always when not no_cache).
    • Run node sidecar and an innocuous command.
    • Inspect aifo-node-cache volume by running a helper container with same image, check stamp file “/home/coder/.cache/.aifo-init-done”.

H) HTTP parser robustness unit tests

 • In http.rs:
    • Chunk extensions tolerated: “A;ext=value” followed by payload; assert parser reads size 10 (hex) and ignores extension.
    • Invalid hex chunk size: return clean EOF rather than panic.
    • BODY_CAP enforcement: feed >1 MiB chunked stream; assert body len equals cap without blocking.

I) Shim native path unit tests (targeted)

 • Add small unit tests for aifo-shim’s url/form encoding helper behavior in isolation by pulling those into a submodule or making them test-only pub:
    • urlencode_component encodes all non-unreserved including “*”.
    • find_header_end (already in util) is used correctly in shim; verify boundaries with CRLF/LF.

J) Consolidate test helpers

 • Move repeated URL→port parsing functions into tests/support (some tests already do this).
 • Provide a helper to render HTTP requests with varied headers (case, missing proto) to reduce duplication.
 • Add small “docker_image_present(img)” helper for gating heavy tests uniformly.

K) Property-style sanitization tests (lightweight)

 • fork_sanitize_base_label: fuzz-like loop over random strings mixing separators and punctuation; assert invariants (only [a-z0-9-], length <= 48, no
   leading/trailing separators). Keep deterministic seeded strings to avoid flakiness.

Process and harness improvements

 • Organize test lanes:
    • Unit/fast: default nextest run (no Docker required).
    • Integration (Docker present): non-ignored tests gated by container_runtime_path() and local image presence checks (you already do this consistently).
    • Acceptance/E2E: #[ignore] tests; document make targets to run them (you already have acceptance tests).
 • Add feature flags or environment toggles for heavy tests:
    • AIFO_CODER_E2E=1 for truly heavy end-to-end runs (already used).
    • AIFO_CODER_HTTP_HARDENING=1 to enable hardening tests once implementation lands; keep them #[ignore] until then.
 • Stabilize flaky logs:
    • Where tests wait for proxy logs (disconnect), always tee to AIFO_TEST_LOG_PATH and use small, bounded polling windows with clear skip messages on
      platforms where FD capture fails.
 • Coverage instrumentation:
    • Use cargo tarpaulin or grcov for line coverage on unit tests (no docker).
    • Keep a coverage guideline doc listing critical surfaces to watch: proxy exec paths (v1/v2), auth/proto, http parsing, notifications, rust/node/python
      toolchain selection, mount/env policies.

Brief code changes for enabling tests

 • Proxy v1 timeout mapping: track a timed_out flag in v1 path watcher; after wait(), respond_plain(..., "504 Gateway Timeout", 124, b"timeout\n") if it is
   set.
 • HTTP parser limits: add header line count cap (e.g., 1024) and Content-Length sanity checks; treat TE: chunked as authoritative when present.
 • Expose small helpers under test cfg for shim encoding, or duplicate minimal logic in test modules to avoid public API changes.

Summary

 • Most core behaviors (fork orchestration, sidecar previews, proxy streaming, notifications policy) have good coverage and gating.
 • The biggest immediate correctness gap is the proto v1 timeout status; fix it and unignore tests or adjust gating.
 • Add guardrail tests for the HTTP parser, env blocklist, spawn error mapping, and node cache stamps.
 • Consolidate helpers and group tests by lanes, and consider adding light property-style tests for sanitization.

These changes will raise confidence in the agent’s behavior under varied environments and inputs, reduce flakiness, and make future refactors safer.
