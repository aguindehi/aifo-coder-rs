# AIFO Coder: Refactor Proxy and Toolchain Shims for Signal Propagation
# and Removal of Proxy-Imposed Timeouts (v1)

Status: Draft for discussion
Owner: Toolchain/Proxy
Last updated: 2025-09-15

Summary
- Introduce end-to-end signal propagation so Ctrl-C (and other signals) sent by the user to the
  shim are forwarded to the proxy and then to the running command inside the toolchain container.
- Remove the default proxy timeout that prematurely kills long-running commands and may “lock” the
  toolbox afterward; replace with an optional opt-in max runtime that escalates signals.
- Ensure reliable termination on client disconnects (shim killed, network drop): the proxy detects
  the disconnect and initiates a termination sequence for the corresponding container process group.
- Maintain backward compatibility for existing shims; improve behavior when they are killed
  (disconnect detection will trigger termination sequence).

Goals
- Propagate user signals: SIGINT (Ctrl-C), SIGTERM, SIGHUP at minimum; support SIGKILL for final
  escalation.
- Ensure that when the client connection drops, the running command is terminated (TERM then KILL).
- Eliminate proxy-imposed hard timeouts by default, supporting long-running workloads.
- Provide opt-in, per-request or per-env maximum runtime with graceful escalation (INT→TERM→KILL).
- Avoid toolbox “lock” situations after timeouts or misbehaving commands.
- Keep behavior consistent across proto v1 (buffered) and proto v2 (streaming), preferring v2.

Non-Goals
- Do not replatform from docker CLI to the Docker Engine API in v1 (considered for future).
- Do not add arbitrary signal forwarding beyond the defined subset initially (can extend later).
- Do not change existing output formats/trailers (X-Exit-Code) beyond adding an optional header.

Terminology
- Shim: Client-side POSIX shell wrapper (aifo-shim) that invokes the proxy via HTTP.
- Proxy: In-process HTTP dispatcher that spawns docker exec into sidecars and streams output.
- Sidecar: Toolchain container running the requested tool command.
- ExecId: A per-execution unique identifier used to reference a running command for signaling.

Background and Issues Today
- The proxy applies a global timeout that can kill long-running commands and leave the toolbox
  effectively “locked” (busy/unavailable) for a period.
- Ctrl-C at the user terminal typically kills curl/shim but does not reach the tool process.
- When the client connection drops, the tool process may keep running in the sidecar indefinitely.

High-Level Design Overview
- Assign an ExecId to each proxied execution.
- Start the tool command inside the sidecar in a new session/process group (setsid), record its
  process group id (PGID) in a well-known path within the container ($HOME/.aifo-exec/<ExecId>/pgid).
- Extend the proxy with a registry mapping ExecId → metadata (container name, state, timestamps).
- Extend the protocol:
  - Shim adds header X-Aifo-Exec-Id to /exec.
  - Proxy returns X-Exec-Id in the 200 prelude for streaming responses.
  - Add a new endpoint POST /signal to forward signals to an ExecId.
- Shim traps signals and POSTs /signal to the proxy for the current ExecId; repeated Ctrl-C escalates
  INT→TERM→KILL.
- On client disconnect, the proxy detects broken pipe and initiates termination (TERM then KILL).
- Remove proxy default timeout; add optional opt-in max runtime with signal escalation.

Detailed Design

1) Identifiers and Generation
- ExecId must be collision-resistant; generate 128-bit random hex (e.g., using existing random_token()
  on the proxy for server-generated IDs, and a similar approach on the shim with fallback to pid-time).
- Shim behavior:
  - Prefer to generate ExecId at shim start. Include in header X-Aifo-Exec-Id.
  - If missing, the proxy generates one and returns X-Exec-Id to the client (back-compat).
- Scope: ExecId is unique per running execution. No persistent storage is required.

2) Protocol Extensions
- Headers (case-insensitive on receipt):
  - From shim to proxy on /exec: X-Aifo-Exec-Id: <id>
  - From proxy to client (streaming prelude): X-Exec-Id: <id>
- New endpoint:
  - POST /signal
  - Auth: same Authorization Bearer token and X-Aifo-Proto as /exec.
  - Body: application/x-www-form-urlencoded
    - exec_id=<id> (required)
    - signal=<SIGINT|SIGTERM|SIGKILL|SIGHUP> (case-insensitive; defaults to SIGTERM if omitted)
  - Status codes:
    - 204 No Content: signal accepted (idempotent; ESRCH treated as success if already exited)
    - 400 Bad Request: invalid/missing parameters
    - 401/403/426: per existing auth/proto policy
    - 404 Not Found: unknown exec_id
    - 409 Conflict: execution already finished and cleaned from registry
    - 500 Internal Server Error: unexpected errors

3) Shim Behavior (aifo-shim)
- Generate ExecId:
  - If uuidgen exists, use it; else fallback to a 128-bit hex from POSIX shell (seeded by time and pid).
- Set headers when invoking /exec:
  - -H "X-Aifo-Exec-Id: $exec_id"
  - Keep Authorization, X-Aifo-Proto, TE: trailers as today.
- Signal traps (POSIX sh):
  - trap 'send_signal INT' INT
  - trap 'send_signal TERM' TERM
  - trap 'send_signal HUP' HUP
  - Maintain an escalation counter for repeated INT within a short window:
    - 1st INT → send INT
    - 2nd INT → send TERM
    - 3rd+ INT → send KILL
- send_signal() implementation:
  - POST to /signal with Authorization and X-Aifo-Proto and form-encoded exec_id and signal.
  - Respect unix socket URL (when configured) by using curl --unix-socket.
- Backward behavior:
  - If shim is killed (SIGKILL) or user kills the terminal, the proxy sees a disconnect and initiates
    termination sequence itself.

4) Proxy: Execution Registry and Lifecycle
- Add ExecRegistry: Arc<Mutex<HashMap<String, ExecHandle>>> where ExecHandle contains:
  - container_name, kind, started_at, last_signal_at, state (Running, Finished {code}), etc.
- On /exec:
  - Obtain ExecId: from header or generate if absent.
  - Insert ExecHandle in registry as Running.
  - Include ExecId in streaming prelude header as X-Exec-Id (v2).
- Spawn behavior inside sidecar:
  - Inject env AIFO_EXEC_ID=<id> for docker exec.
  - Wrap user command with a small shell that:
    - mkdir -p "$HOME/.aifo-exec/$AIFO_EXEC_ID"
    - Launch user command in a new session/process group: ( setsid sh -c "exec <user_cmd>" ) & pg=$!
    - printf "%s" "$pg" > "$HOME/.aifo-exec/$AIFO_EXEC_ID/pgid"
    - wait "$pg"
    - rm -rf "$HOME/.aifo-exec/$AIFO_EXEC_ID" || true
  - This ensures we can signal the entire process group via negative PGID with kill(1).
- Streaming (proto v2):
  - Remove the timeout watcher; stream until child exits or connection breaks.
  - On client write error (BrokenPipe/EPIPE), initiate termination sequence:
    - Send TERM to process group; after a 2-second grace, if still running, send KILL.
    - Wait for docker exec to finish; mark ExecHandle Finished; remove from registry.
- Buffered (proto v1):
  - Remove the deadline/timeout loop; wait until child exits.
  - If the client disconnects before response write, on write error apply the same termination policy.
- Optional max runtime (opt-in):
  - Env: AIFO_TOOLEEXEC_MAX_SECS (global default) or request arg: __aifo_max_secs=NNN
  - If set, a background watcher triggers escalation: at T send INT, after 5s TERM, after another 5s KILL.
  - Default is “no max runtime” (feature disabled).

5) Proxy: /signal Handling
- Authenticate and validate proto as with /exec.
- Parse exec_id and signal (INT/TERM/KILL/HUP; case-insensitive).
- Lookup ExecHandle in registry; if not running, return 409 or 404 as appropriate.
- Perform signal inside the container:
  - docker exec <container> sh -lc 'pg=$(cat "$HOME/.aifo-exec/<id>/pgid" 2>/dev/null); [ -n "$pg" ] && kill -s <SIG> -"$pg" || true'
  - Using negative PGID signals the entire process group.
- Update last_signal_at; return 204.

6) Removal of Proxy-Imposed Timeout
- Eliminate use of AIFO_TOOLEEXEC_TIMEOUT_SECS as a hard kill timer in both v1 and v2 paths.
- Keep compatibility: if AIFO_TOOLEEXEC_TIMEOUT_SECS is set, treat it as AIFO_TOOLEEXEC_MAX_SECS
  following the new escalation behavior (soft deprecation; log a warning).
- Ensure no “lock” conditions: registry is cleaned at exit or on forced termination; subsequent execs
  are unaffected.

7) Security and Isolation
- Authorization and X-Aifo-Proto apply to /signal as for /exec; no unauthenticated signaling.
- ExecId collision avoidance: 128-bit IDs are practically unique; if collision occurs, proxy refuses to
  overwrite existing Running entry (respond 409) and generates a new ID if it created it.
- Registry lives per proxy process instance; it is not shared across sessions or hosts.
- Limit signals allowed via the endpoint; reject others with 400 to minimize abuse.

8) Backward Compatibility
- Legacy shims without X-Aifo-Exec-Id:
  - Proxy generates an ExecId and returns it in X-Exec-Id (v2) but no signal calls from the client.
  - Disconnect behavior still ensures termination.
- Proto v1 remains supported; only difference is buffered output instead of streaming.
- Output trailers (X-Exit-Code) and notification paths remain unchanged.

9) Observability
- Log at verbose level:
  - Exec start with exec_id, tool, kind, args, cwd.
  - Signal received (which, from who) and forwarded; escalation steps.
  - Disconnect detected and termination sequence applied.
  - Exec end with duration, exit code.
- Counters:
  - exec_running, exec_completed, exec_forced_kill, signal_forwarded, signal_unknown_exec.
- Consider exposing lightweight metrics hooks behind verbose mode initially.

10) Failure Modes and Mitigations
- Lost pgid file: Signal handler returns 204 with best-effort; if missing due to fast exit, registry will be cleaned soon.
- Docker exec failure during signal: Return 204 if ESRCH; otherwise 500 and log.
- TTY interference: Default to TTY for improved interactive flushing; allow disabling via AIFO_TOOLEEXEC_TTY=0 (existing behavior).
- Stuck docker exec: Termination sequence uses KILL as last resort; if docker CLI hangs, the proxy process can still proceed for new execs (no global lock).

11) Testing Plan
- Unit tests:
  - http::classify_endpoint recognizes /signal.
  - auth on /signal matches /exec behavior.
- Integration tests (require docker):
  - Long-running command completes without proxy timeout.
  - Ctrl-C once sends SIGINT; command exits with 130/interrupt semantics.
  - Double Ctrl-C escalates to TERM/KILL; verify prompt termination.
  - Kill shim mid-exec → proxy detects disconnect → command terminated in sidecar.
  - Multiple concurrent execs receive signals routed to correct ExecId.
  - Optional max runtime escalates as specified when enabled.
- Regression tests:
  - Notifications endpoint unaffected.
  - TSC local resolution behavior unchanged.
  - Rust bootstrap unaffected aside from no default timeout.

12) Rollout and Phased Plan

Phase 0: Feature flag and scaffolding
- Add environment flag AIFO_TOOLEEXEC_SIGNALS=1 to enable new behavior by default in dev.
- Introduce http::Endpoint::Signal and stub handler returning 501 when disabled.
- Acceptance: Builds and runs with feature flag off; unit tests pass.

Phase 1: ExecId plumbing and registry (no signals yet)
- Shim adds X-Aifo-Exec-Id; proxy records ExecId and returns X-Exec-Id in v2 prelude.
- No timeout removal yet; no signal endpoint yet.
- Acceptance: Existing behavior preserved; headers visible under verbose logs.

Phase 2: setsid wrapper and pgid file in sidecar
- Proxy injects AIFO_EXEC_ID and wraps user command to create process group and pgid file.
- Acceptance: Commands still execute and exit codes preserved; pgid file appears and is cleaned.

Phase 3: /signal endpoint and shim traps
- Implement /signal forwarding to container PGID; shim traps forward INT/TERM/HUP with escalation on repeated INT.
- Acceptance: Ctrl-C stops jobs reliably; repeated Ctrl-C escalates; backward behavior unchanged when traps absent.

Phase 4: Remove proxy hard timeout; add optional max runtime escalation
- Remove AIFO_TOOLEEXEC_TIMEOUT_SECS as a hard kill; map to optional max runtime with escalation if set.
- Acceptance: Long-running commands complete; opt-in max runtime works.

Phase 5: Disconnect detection and termination sequence
- Detect write errors/EOF on client and initiate TERM→KILL sequence; registry cleaned.
- Acceptance: Killing shim causes sidecar command to terminate promptly.

Phase 6: Harden, document, and enable by default
- Update docs and examples; enable behavior by default; keep env to disable if needed temporarily.
- Acceptance: CI green, manual validation across proto v1/v2, macOS/Linux.

13) Implementation Notes (by module)

- src/toolchain/http.rs
  - Add Endpoint::Signal and match "/signal".
  - No change to existing parsing logic; maintain 64 KiB header cap.

- src/toolchain/proxy.rs
  - Add ExecRegistry and ExecHandle types; integrate into toolexec_start_proxy worker(s).
  - On /exec: obtain ExecId (from header or generate); include X-Exec-Id in v2 prelude.
  - Modify build_streaming_exec_args spawn to inject AIFO_EXEC_ID env via build_sidecar_exec_preview.
  - Replace user command with setsid wrapper script that writes pgid, waits, cleans.
  - Remove timeout watcher for v2; remove deadline loop for v1.
  - On client disconnect (write error), apply termination sequence via /bin/sh kill to negative PGID.
  - Implement /signal endpoint to perform in-container kill for provided ExecId.
  - Ensure registry cleanup on normal/abnormal exit.

- src/toolchain/sidecar.rs
  - In build_sidecar_exec_preview: when AIFO_EXEC_ID is set for a run, prepend env and shell wrapper:
    - sh -c 'set -e; d="$HOME/.aifo-exec/$AIFO_EXEC_ID"; mkdir -p "$d"; ( setsid sh -c "exec <user>" ) & pg=$!; printf "%s" "$pg" > "$d/pgid"; wait "$pg"; rm -rf "$d" || true'
  - Preserve current PATH/ENV injection behavior; keep AIFO_TOOLEEXEC_TTY behavior.

- src/toolchain/shim.rs
  - Generate ExecId; add X-Aifo-Exec-Id header to /exec.
  - Add POSIX traps that POST to /signal with same Authorization and unix-socket handling.
  - Maintain a temp dir for headers and clean it as today.

14) Alternatives Considered
- Using docker exec “exec create/start” via Engine API to obtain engine-level exec IDs and attach for
  direct signaling. Deferred to a future iteration due to increased complexity and dependency needs.
- Running tini in the container to manage processes and signals. Good practice but requires image changes;
  we opt for a shell-based setsid wrapper in v1 for zero-image changes.

15) Open Questions
- Should we allow arbitrary numeric signals in /signal? For v1, we constrain to a safe subset.
- Do we want to support cancel-on-EOF for proto v1 when buffering output entirely? We will attempt
  best-effort cancellation on client write error.

16) Risks and Mitigations
- Process group misbehavior under TTY: default to TTY for better flushing; allow disabling if it
  interferes; signals target process group to cover grandchildren.
- flakiness in docker CLI under heavy signaling: retry once for kill exec calls; log on failure.
- Registry leaks on proxy crash: acceptable; containers will eventually exit; future improvement could
  add a reconciler on startup.

Appendix: Example Wrapper Script (conceptual)
  sh -lc '
    set -e
    d="$HOME/.aifo-exec/$AIFO_EXEC_ID"
    mkdir -p "$d"
    ( setsid sh -c "exec '"$USER_CMD"'" ) & pg=$!
    printf "%s" "$pg" > "$d/pgid"
    wait "$pg"
    rm -rf "$d" || true
  '

Appendix: Example Signal Exec
  docker exec <container> sh -lc 'pg=$(cat "$HOME/.aifo-exec/<id>/pgid" 2>/dev/null); [ -n "$pg" ] && kill -s TERM -"$pg" || true'

This specification is intended to be the basis for implementation and review. It is
coherent with the current architecture, uses minimal new surface (one endpoint and
one header), prefers proto v2 streaming, and avoids breaking changes while solving
the timeout and signal propagation problems.
