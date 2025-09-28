# AIFO ToolExec Proxy Protocol

Overview

- Endpoints:
  - POST /exec: run a tool inside a language sidecar.
  - POST /notify: run a host notification command (allowlisted).
  - POST /signal: forward a signal (INT/TERM/HUP/KILL) to a running exec by ID.

- Auth: HTTP Authorization: Bearer <token> — scheme is case-insensitive; credentials must match.

- Protocol versions:
  - V1 (buffered): proxy buffers output until child exit, then responds with a single body.
  - V2 (streaming): proxy streams stdout as HTTP chunked, with exit code in a trailer.

Request formats

- Common headers:
  - Authorization: Bearer <token>
  - X-Aifo-Proto: 1 or 2 (required; 2 for streaming)
  - Content-Type: application/x-www-form-urlencoded

- /exec (form keys):
  - tool=<name>
  - cwd=<absolute path> (defaults to /workspace)
  - arg=<value> (repeatable)

- /notify (form keys):
  - cmd=<basename> (e.g., "say")
  - arg=<value> (repeatable)

Runtime details

- ExecId:
  - The shim sends X-Aifo-Exec-Id on /exec and the proxy mirrors it in streaming prelude.
  - The proxy keeps a registry mapping ExecId → container for signal routing.

- V2 streaming (preferred)
  - Response prelude:
    - HTTP/1.1 200 OK
    - Transfer-Encoding: chunked
    - Trailer: X-Exit-Code
    - X-Exec-Id: <id> (present when the shim provided it)
  - Body: chunks of stdout; each chunk is framed as "<hex-size>\r\n<payload>\r\n".
  - Trailer: "X-Exit-Code: <code>" followed by terminating 0-size chunk and blank line.

- V1 buffered
  - Response:
    - HTTP/1.1 200 OK
    - X-Exit-Code: <code>
    - Content-Length: <n>
  - Body: stdout + stderr concatenated (best-effort).

Timeouts and disconnects

- Max-runtime (optional):
  - Governed by AIFO_TOOLEEXEC_MAX_SECS (or AIFO_TOOLEEXEC_TIMEOUT_SECS).
  - Escalation on timeout: INT at T, TERM at T+5s, KILL at T+10s.
  - V1: timeout maps to "504 Gateway Timeout" with exit code 124.
  - V2: streaming continues until child exit; trailer carries exit code; escalation logs only.

- Client disconnect (V2):
  - The proxy emits a single disconnect line and escalates (INT → TERM → KILL).
  - If a recent /signal was observed for the same ExecId, escalation is suppressed.

Parser tolerances and limits

- Headers end: CRLFCRLF or LFLF are accepted.
- Header count cap: 1024 headers (excluding the request line) — exceeding yields 431/400 upstream.
- Transfer-Encoding vs Content-Length:
  - When Transfer-Encoding contains "chunked", chunked decoding is used and CL is ignored.
  - Multiple TE headers: the last header value is considered.
- Chunked decoder:
  - Accepts chunk extensions (e.g., "A;ext=foo=bar").
  - Invalid chunk sizes terminate decoding gracefully; no panics.
  - Body cap: 1 MiB to bound memory; remainder drained without appending.

Notifications

- Absolute path and basename allowlist enforced (default: ["say"]).
- Optional env trimming:
  - AIFO_NOTIFICATIONS_TRIM_ENV=1 clears child env and preserves PATH, HOME, LANG, LC_*,
    plus names listed in AIFO_NOTIFICATIONS_ENV_ALLOW (comma-separated).
- Timeout:
  - AIFO_NOTIFICATIONS_TIMEOUT_SECS overrides; default aligns with proxy timeout when set.

Security posture

- No privileged mode; tool allowlists enforce surface area per sidecar.
- Absolute path validation and basename allowlist for /notify.
- Bearer token required; proto header enforced with human-readable messages unchanged.
