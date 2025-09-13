AIFO Coder Toolchain Proxy Refactor (Rust) — Cleanup and Modularization Plan v3

Purpose
Reach production readiness by removing brittle routing, centralizing auth/proto validation, eliminating duplication, and hardening timeout and transport handling, while preserving behavior where tests/assertions are explicit. This v3 focuses on: a single notifications endpoint (/notify), deduplication (auth/http/notifications), consistent auth/proto decisions, and fewer, clearer phases.

Scope
- Files: src/toolchain.rs (glue only after refactor), src/toolchain/proxy.rs (proxy + exec), src/toolchain/http.rs (HTTP parsing + endpoint classification), src/toolchain/auth.rs (auth/proto validation), src/toolchain/notifications.rs (notifications parsing/execution).
- Tests: extract inline tests from src/toolchain.rs and src/toolchain/proxy.rs into tests/*.rs; add coverage for endpoint normalization (/notify only), 401 vs 426 outcomes, and timeout child-kill guarantees.

Behavioral invariants (preserve unless explicitly stated)
- Status codes and bodies:
  - 401 Unauthorized → "unauthorized\n"
  - 403 Forbidden → "forbidden\n" (or reason + "\n" for notifications policy errors)
  - 404 Not Found → "not found\n"
  - 405 Method Not Allowed → "method not allowed\n"
  - 409 Conflict → helpful suggestion when no suitable running sidecar exists
  - 426 Upgrade Required → "Unsupported shim protocol; expected 1 or 2\n"
- Exit codes and transport:
  - Buffered (v1): 200 with combined output; X-Exit-Code header.
  - Streaming (v2): chunked transfer; X-Exit-Code in trailer.
  - Timeout: exit code 124; body "aifo-coder proxy timeout\n"; 504 status for buffered path; trailer 124 for streaming.
  - General proxy errors in plain responses map to header X-Exit-Code: 86.
- Proto handling:
  - Valid Authorization but missing/invalid proto → 426.
  - Missing/invalid Authorization → 401.
  - Proto v1 → buffered; Proto v2 → streaming.
- Defaults:
  - host.docker.internal as connection host.
  - TTY default enabled for streaming; AIFO_TOOLEEXEC_TTY=0 disables.
  - Per-request timeout from AIFO_TOOLEEXEC_TIMEOUT_SECS (default 60s).
- HTTP parsing tolerant to CRLFCRLF and LFLF; header cap stays 64 KiB.

Key gaps, inconsistencies and improvements (v2 → v3)
- Single notifications endpoint: normalize to exactly /notify. Remove /notifications and /notifications-cmd aliases and any routing via the "tool" parameter. Requests to deprecated aliases should return 404.
- Endpoint classification: keep classify_endpoint, updated to recognize only "/exec" and "/notify". Do not use substring matches for routing.
- Deduplication:
  - Remove authorization_value_matches from src/toolchain.rs; use auth::authorization_value_matches and auth::validate_auth_and_proto everywhere.
  - Remove parse_form_urlencoded and notifications_* from src/toolchain.rs; use http::parse_form_urlencoded and notifications::… instead.
- Unified form decoding: http::parse_form_urlencoded must implement '+' → space and %XX percent-decoding with best-effort behavior. Callers should not re-decode.
- Streaming prelude timing: do not send chunked prelude until after a successful spawn; on spawn failure, respond with plain 500 (consistent headers) rather than chunked error.
- Buffered timeout hardening: ensure the child process is killed on timeout to avoid orphans (streaming already kills).
- Accept loop robustness: on non-WouldBlock errors, log once per event (verbose) and sleep 50ms to avoid tight spin.
- Unix socket hardening (Linux): create directory with 0700 perms, remove pre-existing socket, remove socket file on shutdown, and attempt to remove directory.
- Notifications policy: default requires Authorization + valid proto; optional unauth bypass controlled solely by AIFO_NOTIFICATIONS_NOAUTH=1 evaluated early. Notifications never go through sidecar allowlists and never route via exec path/tool name.
- Logging: replace cursor-control eprintln! with single-line, newline-terminated messages via log_verbose/log_request_result; never log Authorization or tokens.

Architecture and modules (steady-state)
- proxy.rs
  - Public API: toolexec_start_proxy(session_id, verbose).
  - Internals: listener setup (TCP/unix), accept loop with backoff, per-connection dispatcher using http::read_http_request + http::classify_endpoint.
  - Helpers: respond_plain, respond_chunked_prelude/write/trailer, exec_buffered, exec_streaming, build_streaming_exec_args.
  - Logging: log_verbose(ctx, msg), log_request_result(ctx, …).
- http.rs
  - HttpRequest { method, path_lc, query, headers(lowercased), body }.
  - read_http_request(Read) -> io::Result<HttpRequest>: tolerant header termination; 64 KiB header cap; best-effort Content-Length reading; 1 MiB body cap for forms → map to 400 "bad request\n".
  - classify_endpoint(path_lc) -> Option<Endpoint>: Exec for "/exec"; Notifications for "/notify" only.
  - parse_form_urlencoded(s): percent-decoding policy unified:
    - '+' → space
    - %XX → byte decode; invalid sequences left as literal percent + text (best-effort).
- auth.rs
  - authorization_value_matches: Bearer scheme, case-insensitive.
  - validate_auth_and_proto(headers, token) -> AuthResult { Authorized{proto}, MissingOrInvalidAuth, MissingOrInvalidProto } with Proto { V1, V2 }.
- notifications.rs
  - parse_notifications_command_config(): supports inline arrays, lists, and block scalars; tolerance for trailing "\n" artifacts preserved.
  - notifications_handle_request(argv, verbose, timeout_secs): validates executable "say" and exact args; executes with timeout; on timeout ensure child is killed (no orphan).

Endpoint routing and policy (normalized)
- Exec:
  - Only POST /exec allowed; else 405 or 404 as appropriate.
  - tool, cwd, argv parsed from merged query + application/x-www-form-urlencoded body (using http::parse_form_urlencoded).
  - tool missing:
    - If Authorization seen but proto missing/invalid → 426.
    - If Authorization missing/invalid → 401.
    - Else → 400.
  - tool present:
    - If Authorization ok but proto missing/invalid → 426.
    - If Authorization missing/invalid → 401.
    - If tool not allowed by any sidecar allowlist → 403.
    - Route to sidecar; if no running container → 409 with suggestion.
    - Execute:
      - Proto v1 → buffered (aggregate), 200 with X-Exit-Code.
      - Proto v2 → streaming (chunked), 200 with trailer X-Exit-Code.
- Notifications:
  - Only POST /notify is accepted. All other paths (including /notifications, /notifications-cmd) → 404.
  - Default: require Authorization and valid proto (consistent with exec).
  - Optional bypass: AIFO_NOTIFICATIONS_NOAUTH=1 permits unauthenticated /notify; evaluated before any sidecar allowlist and independent of exec routing.
  - Parse argv from "arg=..." pairs (form/query merged). Execute notifications_handle_request:
    - Success: 200 with X-Exit-Code and body.
    - Policy/config rejection: 403 with reason + "\n".
    - Auth present but bad/missing proto: 426.
    - Missing/invalid auth when required: 401.

Execution semantics and hardening
- Buffered exec (v1):
  - Spawn docker runtime; aggregate stdout+stderr.
  - On timeout: kill child process and return 504 with "aifo-coder proxy timeout\n", X-Exit-Code: 124.
  - On spawn/exec error: 500 plain with "aifo-coder proxy error: …\n" and X-Exit-Code: 1.
- Streaming exec (v2):
  - Attempt to spawn before sending HTTP chunked prelude. If spawn fails, respond 500 plain (not chunked) with consistent headers.
  - While running: stream stdout chunks; drain stderr to avoid backpressure.
  - On timeout: kill child; emit "aifo-coder proxy timeout\n" chunk and trailer X-Exit-Code: 124.
  - On completion: trailer with actual exit code.
- Arg building:
  - Encapsulate container-name index scan in build_streaming_exec_args; document current assumption.
  - Future: consider structured return from build_sidecar_exec_preview to eliminate scanning.

Unix socket transport (Linux)
- Gate via AIFO_TOOLEEXEC_USE_UNIX=1; set AIFO_TOOLEEXEC_UNIX_DIR to the directory for mounts.
- Create directory with 0700 perms; remove pre-existing socket file; remove socket file on shutdown and attempt to cleanup directory.

Accept loop robustness
- For WouldBlock: sleep 50ms.
- For non-WouldBlock errors: log once per event (verbose) and sleep 50ms to avoid tight spin.

Logging
- Replace cursor-control eprintln! with:
  - log_verbose(ctx, "..."): single newline-terminated line; redact sensitive data.
  - log_request_result(ctx, tool, kind, code, started): include duration ms.
- Never log tokens or Authorization headers.

Constants and configuration
- EXIT_HTTP_ERR: 86
- EXIT_TIMEOUT: 124
- Header cap: 64 KiB (http.rs constant).
- Body cap: 1 MiB soft cap for forms (http.rs).
- Env toggles:
  - AIFO_TOOLEEXEC_TTY=0 (disable streaming TTY).
  - AIFO_TOOLEEXEC_TIMEOUT_SECS (u64; default 60).
  - AIFO_NOTIFICATIONS_NOAUTH=1 (enable notifications unauth bypass).
  - AIFO_TOOLEEXEC_USE_UNIX=1 (Linux; AF_UNIX listener).
- PROXY_ENV_NAMES:
  - If used by mount/env logic, move to toolchain/env.rs as pub(crate).
  - If unused, remove from toolchain.rs (eliminate dead constants).

Phased implementation (minimized)
Phase 1: Normalize endpoints and deduplicate
- Update http::classify_endpoint to recognize only "/exec" and "/notify".
- Remove substring routing and tool=notifications-cmd hacks in proxy flow; route solely by classify_endpoint.
- Remove duplicates from src/toolchain.rs:
  - authorization_value_matches → use auth::authorization_value_matches.
  - parse_form_urlencoded → use http::parse_form_urlencoded (with percent-decoding).
  - parse_notifications_command_config and notifications_handle_request → use notifications::… module.
- Update http::parse_form_urlencoded to implement '+' and %XX decoding with best-effort tolerance.
- Introduce BODY_CAP (1 MiB) in http::read_http_request; on exceed, map to 400 "bad request\n".
- Tests: extract inline #[cfg(test)] from src/toolchain.rs to tests/auth_authorization_value_matches.rs (and others as needed).

Phase 2: Dispatcher and execution hardening
- Proxy dispatcher in proxy.rs:
  - Replace ad-hoc parsing with http::read_http_request and http::classify_endpoint.
  - Centralize auth/proto validation with auth::validate_auth_and_proto; deterministically map to 401 vs 426.
  - Notifications: enforce POST /notify; evaluate AIFO_NOTIFICATIONS_NOAUTH before anything else; never apply sidecar allowlist.
  - Exec: enforce POST /exec; parse tool/cwd/argv via http::parse_form_urlencoded; enforce allowlist and sidecar existence → 403/409.
- Execution helpers:
  - exec_buffered: kill child on timeout; respond 504 + exit 124.
  - exec_streaming: only send chunked prelude after successful spawn; on spawn-fail respond 500 plain. Drain stderr to avoid backpressure.
- Accept loop: on non-WouldBlock errors, log (verbose) and sleep 50ms.
- Unix socket: 0700 perms; cleanup socket on shutdown; attempt directory cleanup.

Phase 3: Tests and cleanup
- Move remaining inline tests from src/toolchain.rs and src/toolchain/proxy.rs into:
  - tests/auth_authorization_value_matches.rs
  - tests/http_parsing_tolerance.rs
  - tests/notifications_config_parse.rs
  - tests/proxy_notifications_policy.rs (auth-required vs NOAUTH=1)
  - tests/proxy_exec_timeout_kills_child.rs (v1 and v2)
  - tests/proxy_streaming_spawn_fail_plain_500.rs
  - tests/http_endpoint_routing.rs (/exec ok; /notify ok; /notifications and /notifications-cmd → 404)
- Ensure centralized validate_auth_and_proto drives 401 vs 426 replies.
- Remove dead constants and unused helpers from src/toolchain.rs; keep it as glue.

Testing plan
Unit tests:
- auth::authorization_value_matches edge cases (case, whitespace, punctuation, quotes).
- http::classify_endpoint: only /exec and /notify recognized; unknown and deprecated aliases → None.
- http::read_http_request: CRLF/LF terminators; header cap; Content-Length handling; percent-decoding (+ and %XX).
- notifications::parse_notifications_command_config for inline array, YAML list, block scalar, single-line; tolerance for trailing "\n".

Integration tests:
- Exec:
  - 401 on missing/invalid auth.
  - 426 when auth ok but proto missing/invalid.
  - 403 for disallowed tools.
  - 409 when sidecar missing, with suggestion message.
  - v1 buffered returns 200 with X-Exit-Code and combined output; timeout → 504 with body and exit 124; child is killed.
  - v2 streaming returns chunked output; trailer X-Exit-Code; spawn error returns 500 plain (no chunked prelude); timeout chunk + trailer 124; child killed.
- Notifications (/notify only):
  - Authorized + valid proto → 200 with X-Exit-Code and body.
  - Authorized + bad/missing proto → 426.
  - Default unauth → 401.
  - With AIFO_NOTIFICATIONS_NOAUTH=1 → 200 or 403 with reason + "\n" depending on config/args.
  - Requests to /notifications or /notifications-cmd → 404.

Compatibility and edge cases
- Intentional change: only /notify is supported; /notifications and /notifications-cmd now return 404. Update shims or clients accordingly.
- Legacy requests posting tool=notifications-cmd to /exec follow exec rules (likely 403). Notifications are routed solely by path.
- LF-only header termination remains tolerated.
- TSC special-casing preserved: prefer ./node_modules/.bin/tsc else npx tsc.
- host.docker.internal remains the URL across platforms.

Acceptance criteria
- Existing tests pass unchanged where behavior was explicit and not tied to deprecated notifications aliases.
- New tests cover endpoint normalization to /notify, 401 vs 426 split, and timeout process-kill behavior.
- Module boundaries compile with no public API break at crate root.
- Logging is concise in verbose mode; no sensitive data logged.
- No orphan processes remain after timeout scenarios.
- Unix socket directory perms set to 0700; socket file removed on shutdown.

Risks and mitigations
- Breaking change for notifications aliases: mitigate by documenting /notify as the only supported endpoint and updating shims/clients accordingly.
- 401 vs 426 split: centralized validate_auth_and_proto ensures consistent outcomes; tests assert both paths.
- Streaming prelude timing: only send prelude after successful spawn; add spawn-fail test to lock behavior.
- Container-name scan fragility: encapsulated in helper; future structured return planned to eliminate scanning.

Rollout
- Implement phases in order with clear commits and CI.
- Enable verbose logging in staging to monitor for regressions.
- Update shim scripts and documentation to use /notify endpoint exclusively.
- After stabilization, remove any leftover alias handling code (if temporarily retained during rollout).

End of specification.
