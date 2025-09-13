AIFO Coder Toolchain Proxy Refactor (Rust) — Cleanup and Modularization Plan v1

Purpose
Elevate the proxy/exec subsystem to production readiness by modularizing responsibilities, normalizing routing and auth semantics, reducing duplication, and hardening error/timeout handling, while preserving existing behavior and tests where they are explicit.

Scope
- Files: src/toolchain.rs (extraction and glue), src/toolchain/proxy.rs (proxy + exec), src/toolchain/http.rs (HTTP parsing + endpoint classification), src/toolchain/auth.rs (auth/proto validation), src/toolchain/notifications.rs (notifications parsing/execution).
- Non-goals: modify sidecar/image selection logic, change external CLI/API surfaces, or alter container lifecycle beyond proxy exec orchestration.

Behavioral invariants (must preserve)
- Status codes and bodies for errors:
  - 401 Unauthorized → "unauthorized\n"
  - 403 Forbidden → "forbidden\n" (or reason + "\n" from notifications policy)
  - 404 Not Found → "not found\n"
  - 405 Method Not Allowed → "method not allowed\n"
  - 409 Conflict when no suitable running sidecar → message with suggestion + "\n"
  - 426 Upgrade Required → "Unsupported shim protocol; expected 1 or 2\n"
- Exit codes:
  - X-Exit-Code for buffered (v1) and notifications responses.
  - Streaming (v2) uses chunked transfer with X-Exit-Code trailer.
  - Timeout → exit code 124, body "aifo-coder proxy timeout\n" (504 for buffered path), and trailer 124 for streaming.
  - General proxy errors map to exit code 86 in headers for plain responses.
- Proto handling:
  - If Authorization is valid but proto missing/invalid → 426.
  - If Authorization missing/invalid → 401.
  - Proto v1 → buffered exec, Proto v2 → streaming exec.
- Defaults:
  - host.docker.internal remains the connection host.
  - Streaming uses TTY by default; AIFO_TOOLEEXEC_TTY=0 disables TTY.
  - Per-request timeout from AIFO_TOOLEEXEC_TIMEOUT_SECS (default 60s).
- HTTP parsing remains tolerant to CRLFCRLF and LFLF header termination; header cap remains 64 KiB.

Key issues found (from review) and decisions
- Monolithic handle_connection: Split responsibilities into modules and helpers. Keep a thin dispatcher in proxy.rs.
- Inconsistent notifications routing: Normalize routing strictly by endpoint path (accept legacy aliases). Eliminate use of tool=notifications-cmd for routing decisions.
- Dead “no-auth bypass” code path: If an unauth mode is maintained, ensure it is evaluated before allowlist checks and is opt-in via env to avoid accidental exposure.
- Asymmetric auth/proto handling: Centralize to produce consistent 401 vs 426 outcomes based on presence/validity of Authorization and X-Aifo-Proto.
- Duplication across buffered vs streaming execution: Factor shared logic; unify timeout semantics and logging.
- Log noise and cursor control: Replace with a single verbose logger; avoid stray carriage returns and repeated flushes; redact sensitive data.
- Fragile container-name scan for streaming args: Encapsulate this scan and document assumptions; future-proof by considering structured return (optional future enhancement).
- Orphan child processes on buffered timeouts: Ensure child processes are terminated on timeout (hardening).
- Accept-loop error visibility: Log non-WouldBlock errors and back off to avoid tight spin.

Architecture and modules
- proxy.rs
  - Public: toolexec_start_proxy(session_id, verbose).
  - Internal: listener setup (TCP/unix), accept loop, per-connection dispatcher.
  - Helpers: respond_plain, respond_chunked_prelude/write/trailer, exec_buffered, exec_streaming, build_streaming_exec_args.
  - Logging: log_verbose(ctx, msg), log_request_result(ctx, …).
- http.rs
  - HttpRequest model: method (GET/POST/Other), path_lc, query pairs, headers (lowercased), body bytes.
  - read_http_request(Read) -> io::Result<HttpRequest>: tolerant header termination; header cap 64 KiB; best-effort body read per Content-Length.
  - classify_endpoint(path_lc) -> Option<Endpoint>: Exec for "/exec", Notifications for "/notifications", "/notifications-cmd", "/notify".
  - parse_form_urlencoded: returns pairs; decoding policy defined below.
- auth.rs
  - authorization_value_matches: "Bearer <token>" scheme (case-insensitive).
  - validate_auth_and_proto(headers, token) -> AuthResult { Authorized{proto}, MissingOrInvalidAuth, MissingOrInvalidProto }.
  - Proto enum { V1, V2 } with explicit mapping from "1" / "2".
- notifications.rs
  - parse_notifications_command_config(): replicate existing YAML parsing with support for inline arrays, lists, and block scalars.
  - notifications_handle_request(argv, verbose, timeout_secs): validate executable "say" and exact argument match; execute with timeout and return combined stdout/stderr; kill process on timeout.

Endpoint routing and policy
- Exec endpoint:
  - Only POST /exec permitted; otherwise 405 or 404.
  - Tool, cwd, argv collected from query and form (application/x-www-form-urlencoded). Combine pairs and percent-decode both keys and values.
  - tool missing:
    - If Authorization seen but proto missing/invalid → 426.
    - If Authorization missing/invalid → 401.
    - Else → 400.
  - tool present:
    - If Authorization ok but proto missing/invalid → 426.
    - If Authorization missing/invalid → 401.
    - If tool not allowed by any sidecar allowlist → 403.
    - Select sidecar kind via routing; ensure matching container exists → else 409 with helpful message.
    - Execute:
      - Proto v1 → buffered (aggregated), 200 with X-Exit-Code.
      - Proto v2 → streaming (chunked), 200 with X-Exit-Code trailer.
- Notifications endpoint:
  - Only POST accepted to any of the aliases ("/notifications", "/notifications-cmd", "/notify").
  - Default policy: require Authorization and valid proto (consistent with exec).
  - Optional bypass: if AIFO_NOTIFICATIONS_NOAUTH=1, allow unauthenticated notifications requests; this is evaluated before tool allowlist and does not reuse exec routing.
  - Parse argv from "arg=..." pairs. Execute notifications_handle_request.
  - On success: 200 with X-Exit-Code and body.
  - On rejection (policy mismatch or config mismatch): 403 with reason + "\n".
  - On auth present but bad/missing proto: 426.
  - On missing/invalid auth when required: 401.

HTTP parsing and decoding
- Header normalization: lowercased keys; retain raw values.
- Content-Length: if present, read that many bytes best-effort; do not block indefinitely.
- Percent-decoding: for form-urlencoded pairs, perform:
  - '+' → space
  - %XX → byte decode; invalid sequences decoded as literal text (best-effort) to match current tolerance.
- Limits:
  - Header cap: 64 KiB total header bytes (existing).
  - Body cap: introduce a soft cap (e.g., 1 MiB) for form bodies to avoid memory DoS; log and treat as bad request if exceeded. This cap is safe given the tiny payloads used by shim forms.

Authorization and protocol validation
- Do not log Authorization headers or tokens.
- Authorization uses Bearer scheme with ASCII whitespace separator; exact token match.
- X-Aifo-Proto must be "1" or "2" when Authorization is present/valid; otherwise return 426.
- Centralize decision to produce deterministic 401 vs 426 for all endpoints.

Execution semantics
- Buffered exec (v1):
  - Spawn docker runtime with preview args; aggregate stdout and stderr.
  - Timeout: kill child process and return 504 with "aifo-coder proxy timeout\n" and X-Exit-Code: 124.
  - On spawn/exec error: return 500 with diagnostic "aifo-coder proxy error: …\n" and X-Exit-Code: 1.
- Streaming exec (v2):
  - Build spawn args with optional TTY; wrap user command via sh -c "… 2>&1".
  - Precondition: attempt to spawn child before sending the HTTP chunked prelude. If spawn fails, respond with 500 plain (not chunked), preserving consistent client semantics.
  - While running: stream stdout chunks to client; drain stderr to avoid backpressure.
  - Timeout: kill process, send "aifo-coder proxy timeout\n" chunk, and trailer X-Exit-Code: 124.
  - Normal completion: send trailer with actual exit code.
- Arg building:
  - Encapsulate container-name index scan in build_streaming_exec_args; document assumption that preview args follow "docker exec … <container> …".
  - Future enhancement (optional): change build_sidecar_exec_preview to return structured data (pre-container args, container name, post-container args) to eliminate scanning.

Logging
- Replace multiple log helpers with:
  - log_verbose(ctx, msg: &str): single newline-terminated message; no cursor control codes by default.
  - log_request_result(ctx, tool, kind, code, started): include duration ms.
- Redact sensitive material; never log tokens or Authorization headers.
- Accept loop: on non-WouldBlock errors, log once per event in verbose mode and sleep briefly (e.g., 50ms) to avoid tight spins.

Constants and configuration
- EXIT_HTTP_ERR: 86
- EXIT_TIMEOUT: 124
- Header/body caps defined in one place in http.rs.
- Env toggles:
  - AIFO_TOOLEEXEC_TTY=0 (disable TTY in streaming).
  - AIFO_TOOLEEXEC_TIMEOUT_SECS (u64 seconds; default 60).
  - AIFO_NOTIFICATIONS_NOAUTH=1 (enable notifications unauth bypass).
  - AIFO_TOOLEEXEC_USE_UNIX=1 (Linux-only; use AF_UNIX listener and expose path through AIFO_TOOLEEXEC_UNIX_DIR for container mount).
- PROXY_ENV_NAMES:
  - If used by mount/env logic, move to toolchain/env.rs as pub(crate).
  - If unused, remove from toolchain.rs.

Robustness and security hardening
- Kill child processes on timeout for both buffered and streaming paths to avoid orphan processes.
- Validate POST method strictly for both /exec and notifications endpoints; non-POST → 405.
- Normalize endpoint classification in a single place; never infer notifications from tool parameter for routing.
- Unix socket mode (Linux): set directory to 0700 and remove socket file on shutdown; ensure directory cleanup if feasible.
- Graceful shutdown: stop accept loop on running flag; existing connections complete; consider a global shutdown timeout in future.

Compatibility and edge cases
- Legacy notifications aliases accepted by path (not by tool parameter).
- Requests that previously posted tool=notifications-cmd to /exec will follow exec rules (likely 403) and are not treated as notifications.
- LF-only header termination remains tolerated.
- TSC special-casing remains: prefer ./node_modules/.bin/tsc else npx tsc.
- host.docker.internal used across platforms; on Linux, rely on host-gateway config and keep same URL.

Incremental implementation plan
1) Extract proxy into proxy.rs (no behavior change).
2) Extract notifications into notifications.rs (no behavior change).
3) Add http.rs with HttpRequest and read_http_request; integrate into proxy dispatcher (behavior preserved).
4) Add auth.rs; replace scattered 401/426 logic with validate_auth_and_proto (behavior preserved).
5) Normalize notifications routing by endpoint:
   - Require auth by default; add AIFO_NOTIFICATIONS_NOAUTH=1 bypass evaluated before allowlist checks.
   - Remove secondary/dead notifications block.
6) Factor exec paths:
   - exec_buffered/exec_streaming with shared timeout handling; ensure child kill on timeout.
   - Build streaming args via helper; document assumptions.
   - Only send chunked prelude after successful spawn.
7) Logging cleanup and accept-loop backoff on errors.
8) Optional robustness:
   - Body cap for forms; return 400 on cap exceed.
   - Unix socket directory perms and cleanup on exit.
   - Relocate/remove PROXY_ENV_NAMES.

Testing plan
Unit tests:
- auth::authorization_value_matches edge cases (case, whitespace, punctuation, quotes).
- http::classify_endpoint for aliases and unknown paths.
- http::read_http_request: CRLF/LF terminators; header cap; Content-Length; percent-decoding for forms (including '+' handling).
- notifications::parse_notifications_command_config for inline array, list, block scalar, and single-line cases.

Integration tests:
- Exec endpoint:
  - 401 on missing/invalid auth.
  - 426 when auth ok but proto missing/invalid.
  - 403 for disallowed tools.
  - 409 when sidecar missing.
  - v1 buffered returns combined output, X-Exit-Code, 200.
  - v2 streaming returns chunked output, trailer X-Exit-Code; spawn error returns 500 plain.
  - Timeouts → 504 for v1 with body; v2 timeout chunk + trailer 124.
- Notifications endpoint:
  - Authorized + good proto → executes and returns 200 with X-Exit-Code.
  - Authorized + bad/missing proto → 426.
  - Unauth:
    - Default policy → 401.
    - With AIFO_NOTIFICATIONS_NOAUTH=1 → 200 with X-Exit-Code or 403 with reason.

Acceptance criteria
- Existing tests pass unchanged.
- New tests cover endpoint normalization, 401 vs 426 split, and timeout process-kill behavior.
- Module boundaries compile with no public API break at crate root.
- Logging is concise in verbose mode and avoids sensitive data.
- No orphan processes remain after timeout scenarios.

Risks and mitigations
- Subtle 401 vs 426 differences: centralized validate_auth_and_proto enforces consistency; add tests for both paths.
- Streaming prelude timing: sending prelude only after successful spawn avoids protocol confusion; add test for spawn-fail case.
- Notifications unauth expectations: default to auth-required; bypass gated by explicit env.
- Container-name scan fragility: documented and encapsulated; revisit when sidecar preview returns structured data.

Rollout
- Land changes in small PRs per incremental plan with clear commit messages.
- Monitor for regressions under verbose logging in staging; expand tests as needed.
- After stabilization, consider documenting the notifications endpoint and env toggles for users.

End of specification.
