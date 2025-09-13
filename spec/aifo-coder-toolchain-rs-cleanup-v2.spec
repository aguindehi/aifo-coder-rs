AIFO Coder Toolchain Proxy Refactor (Rust) — Cleanup and Modularization Plan v2

Purpose
Reach production readiness by eliminating brittle routing, centralizing auth/proto validation, removing duplication, and hardening timeout and transport handling, while preserving current behavior where tests/assertions are explicit.

Scope
- Files: src/toolchain.rs (glue only), src/toolchain/proxy.rs (proxy + exec), src/toolchain/http.rs (HTTP parsing + endpoint classification), src/toolchain/auth.rs (auth/proto validation), src/toolchain/notifications.rs (notifications parsing/execution).
- Test relocation: extract inline tests from src/toolchain.rs and src/toolchain/proxy.rs to tests/*.rs for clarity and better integration coverage.

Behavioral invariants (must preserve)
- Status codes and bodies:
  - 401 Unauthorized → "unauthorized\n"
  - 403 Forbidden → "forbidden\n" (or reason + "\n" for notifications policy errors)
  - 404 Not Found → "not found\n"
  - 405 Method Not Allowed → "method not allowed\n"
  - 409 Conflict → helpful suggestion when no suitable running sidecar exists
  - 426 Upgrade Required → "Unsupported shim protocol; expected 1 or 2\n"
- Exit codes and transport:
  - Buffered (v1): 200 with combined output, X-Exit-Code header.
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

Key gaps and inconsistencies found
- Monolithic proxy flow: handle_connection remains large and continues to mix HTTP parsing, routing, auth, exec, timeouts, and response writing. This reduces clarity and increases bug surface.
- Notifications routing remains coupled to "tool=notifications-cmd" and path substrings. Early sidecar allowlist 403 can preempt the intended "no-auth bypass" path, leaving dead code.
- Duplicated helpers:
  - authorization_value_matches exists in both src/toolchain.rs and src/toolchain/auth.rs (conflict risk).
  - parse_form_urlencoded appears in both src/toolchain.rs and src/toolchain/http.rs with different decoding behavior (http.rs lacks percent-decoding).
- Streaming prelude timing: current streaming path sends the chunked prelude before attempting to spawn; on spawn failure returns chunked error instead of a plain 500. This contradicts the safer behavior: only send prelude after successful spawn to avoid protocol confusion.
- Buffered timeouts may leave orphan processes (child not killed on timeout), while streaming kills the child properly.
- Method policy: notifications paths are “exempt” from strict POST checks in current flow, which is inconsistent and surprising.
- Accept loop: non-WouldBlock errors are silently ignored; may lead to tight spins without visibility.
- Unix socket: directory perms and cleanup are not enforced; risk of leaking sockets and exposing paths due to permissive defaults.
- Logging: multiple eprintln! with cursor codes; inconsistent and noisy; risks leaking operational context. Authorization values must never be logged.

Architecture and modules
- proxy.rs
  - Public API: toolexec_start_proxy(session_id, verbose).
  - Internals: listener setup (TCP/unix), accept loop with backoff, per-connection dispatcher using http::read_http_request + http::classify_endpoint.
  - Helpers: respond_plain, respond_chunked_prelude/write/trailer, exec_buffered, exec_streaming, build_streaming_exec_args.
  - Logging: log_verbose(ctx, msg), log_request_result(ctx, …) with single-line messages and no cursor control codes.
- http.rs
  - HttpRequest { method, path_lc, query, headers(lowercased), body }.
  - read_http_request(Read) -> io::Result<HttpRequest>: tolerant header termination; 64 KiB header cap; best-effort Content-Length reading.
  - classify_endpoint(path_lc) -> Option<Endpoint>: Exec for "/exec"; Notifications for "/notifications", "/notifications-cmd", "/notify".
  - parse_form_urlencoded(s): implement consistent percent-decoding policy:
    - '+' → space
    - %XX → byte decode; invalid sequences left as literal percent+text (best-effort).
  - Introduce 1 MiB soft cap for form bodies; exceed → 400 with "bad request\n".
- auth.rs
  - authorization_value_matches: Bearer scheme, case-insensitive.
  - validate_auth_and_proto(headers, token) -> AuthResult { Authorized{proto}, MissingOrInvalidAuth, MissingOrInvalidProto } with Proto { V1, V2 }.
- notifications.rs
  - parse_notifications_command_config(): supports inline arrays, lists, and block scalars; preserves existing tolerance (e.g., trailing "\n" artifacts).
  - notifications_handle_request(argv, verbose, timeout_secs): validates executable "say" and exact args; executes with timeout; on timeout ensure child is killed (no orphan).

Endpoint routing and policy (normalized)
- Exec endpoint:
  - Only POST /exec allowed; else 405 or 404 as appropriate.
  - tool, cwd, argv parsed from query + application/x-www-form-urlencoded body (merged). Use percent-decoding policy above.
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
- Notifications endpoint:
  - Only POST accepted for "/notifications", "/notifications-cmd", or "/notify".
  - Default: require Authorization and valid proto (consistent with exec).
  - Optional bypass: AIFO_NOTIFICATIONS_NOAUTH=1 permits unauthenticated notifications; evaluated before any sidecar allowlist and independent of exec routing.
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
- For non-WouldBlock errors: log once per event (verbose) and back off (e.g., 50ms) to avoid tight spin.

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
Phase 1: Consolidation and duplication removal
- Remove authorization_value_matches and parse_form_urlencoded duplicates from src/toolchain.rs; use auth::authorization_value_matches and http::parse_form_urlencoded with the unified percent-decoding policy.
- Update http::parse_form_urlencoded to implement '+' and %XX decoding with best-effort tolerance.
- Introduce a BODY_CAP (1 MiB) in http::read_http_request; on exceed, return an error mapped to 400 "bad request\n".
- Move PROXY_ENV_NAMES to toolchain/env.rs as pub(crate), or remove if unused.

Phase 2: Proxy dispatcher and endpoint normalization
- In proxy.rs, replace ad-hoc parsing with http::read_http_request and http::classify_endpoint.
- Centralize auth/proto validation with auth::validate_auth_and_proto; map to 401 vs 426 deterministically for both endpoints.
- Notifications:
  - Route by path only; do not set tool to "notifications-cmd".
  - Enforce POST; handle AIFO_NOTIFICATIONS_NOAUTH early; never apply sidecar allowlist to notifications.
- Exec:
  - Enforce POST /exec.
  - Parse tool/cwd/argv from merged query+form pairs.
  - Enforce allowlist and sidecar existence → 403/409.
- Execution helpers:
  - exec_buffered: kill child on timeout; respond 504 + exit 124.
  - exec_streaming: only send chunked prelude after successful spawn; on spawn-fail respond 500 plain. Drain stderr to avoid backpressure.
- Accept loop: on non-WouldBlock errors, log (verbose) and sleep 50ms.

Phase 3: Unix socket hardening and tests extraction
- Set unix socket directory perms (0700) on creation; cleanup socket on shutdown; attempt directory removal.
- Extract inline #[cfg(test)] modules from src/toolchain.rs and src/toolchain/proxy.rs into tests/:
  - tests/auth_authorization_value_matches.rs
  - tests/http_parsing_tolerance.rs
  - tests/notifications_config_parse.rs
  - tests/proxy_notifications_policy.rs (auth-required vs NOAUTH=1)
  - tests/proxy_exec_timeout_kills_child.rs (v1 and v2)
  - tests/proxy_streaming_spawn_fail_plain_500.rs
- Ensure new tests reflect the 401 vs 426 split, endpoint normalization, and timeout child-kill guarantees.

Testing plan
Unit tests:
- auth::authorization_value_matches edge cases (case, whitespace, punctuation, quotes).
- http::classify_endpoint for canonical paths and aliases; unknown path → None.
- http::read_http_request: CRLF/LF terminators; header cap; Content-Length handling; percent-decoding (+ and %XX).
- notifications::parse_notifications_command_config for inline array, YAML list, block scalar, single-line; tolerance for trailing "\n".

Integration tests:
- Exec endpoint:
  - 401 on missing/invalid auth.
  - 426 when auth ok but proto missing/invalid.
  - 403 for disallowed tools.
  - 409 when sidecar missing, with suggestion message.
  - v1 buffered returns 200 with X-Exit-Code and combined output.
  - v2 streaming returns chunked output, trailer X-Exit-Code; spawn error returns 500 plain (no chunked prelude).
  - Timeouts → 504 for v1 with body; v2 timeout chunk + trailer 124; child is killed in both paths.
- Notifications endpoint:
  - Authorized + valid proto → 200 with X-Exit-Code and body.
  - Authorized + bad/missing proto → 426.
  - Unauth:
    - Default policy → 401.
    - With AIFO_NOTIFICATIONS_NOAUTH=1 → 200 or 403 with reason + "\n" depending on config/args.

Compatibility and edge cases
- Legacy requests posting tool=notifications-cmd to /exec follow exec rules (likely 403). Notifications are routed solely by path.
- LF-only header termination remains tolerated.
- TSC special-casing preserved: prefer ./node_modules/.bin/tsc else npx tsc.
- host.docker.internal remains the URL across platforms.

Acceptance criteria
- Existing tests pass unchanged where behavior was explicit.
- New tests cover endpoint normalization, 401 vs 426 split, and timeout process-kill behavior.
- Module boundaries compile with no public API break at crate root.
- Logging is concise in verbose mode; no sensitive data logged.
- No orphan processes remain after timeout scenarios.
- Unix socket directory perms set to 0700; socket file removed on shutdown.

Risks and mitigations
- 401 vs 426 split: centralized validate_auth_and_proto ensures consistent outcomes; tests assert both paths.
- Streaming prelude timing: sending prelude only after successful spawn avoids protocol confusion; add spawn-fail test to lock behavior.
- Notifications no-auth expectations: default to auth-required; unauth enabled only via explicit env; tested both ways.
- Container-name scan fragility: encapsulated in helper; document; future structured return plan.

Rollout
- Implement phases in order; each phase lands as a small PR with clear commit messages.
- Enable verbose logging in staging to monitor for regressions.
- After stabilization, document notifications endpoint and env toggles for users.

End of specification.
