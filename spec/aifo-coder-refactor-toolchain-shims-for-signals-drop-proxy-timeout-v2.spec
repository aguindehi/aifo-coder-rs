# AIFO Coder: Signal Propagation E2E and Removal of Proxy Timeouts (v2)

Status: Draft for discussion
Owner: Toolchain/Proxy
Last updated: 2025-09-15

Purpose
- Provide a coherent, minimal-risk design to propagate user signals (Ctrl-C et al.) from shims to
  the proxy and then to the running command inside the toolchain container.
- Remove the proxy-imposed default timeout to allow long-running workloads while preserving the
  ability to cancel or forcibly stop misbehaving jobs.
- Ensure the toolbox never “locks” after a timeout or client disconnect; all state is cleaned.

Key Outcomes
- User signals reach the containerized tool reliably: INT → TERM → KILL escalation.
- Client disconnects trigger a termination sequence in the container (TERM then KILL).
- No default proxy timeout; optional opt-in max runtime with graceful escalation.
- Backward compatibility with existing shims and protocol versions (v1 buffered, v2 streaming).

Design Summary

1) ExecId as the unit of control
- Each execution is identified by a collision-resistant ExecId (128-bit hex).
- The shim sets X-Aifo-Exec-Id; if missing, the proxy generates one and returns it (X-Exec-Id).
- The proxy keeps a per-process registry of running execs keyed by ExecId.

2) Process group supervision in the sidecar
- The tool command runs in a new session/process group using setsid.
- The process group ID (PGID) is captured and written to $HOME/.aifo-exec/<ExecId>/pgid inside
  the sidecar; wrapper waits for the PGID and removes the directory on exit.
- Signals target the entire process group (kill -s <SIG> -$PGID).

3) Protocol extensions (minimal and backward compatible)
- Request header on /exec: X-Aifo-Exec-Id: <id> (shim generated; optional).
- Response header on v2 prelude: X-Exec-Id: <id> (always included so legacy clients can learn it).
- New endpoint: POST /signal with body exec_id=<id>&signal=<SIGINT|SIGTERM|SIGKILL|SIGHUP>.
- Headers follow existing auth and proto requirements; header names are case-insensitive (we normalize
  to lowercase in the parser).

4) Shim behavior: well-behaved process that forwards signals
- Generate ExecId at start; add -H "X-Aifo-Exec-Id: $exec_id" to the /exec request.
- Install traps for INT/TERM/HUP that POST /signal to the proxy; repeated INT escalates:
  1st INT → SIGINT, 2nd INT → SIGTERM, 3rd+ → SIGKILL.
- If the shim is killed or the terminal is closed, the HTTP connection drops and proxy disconnect
  handling terminates the container process group (TERM→KILL).

5) Proxy changes
- Registry: Arc<Mutex<HashMap<String, ExecHandle>>> tracks container name, kind, timestamps,
  and running/finished state for each ExecId.
- /exec handling:
  - Accept or generate ExecId; store in registry; inject AIFO_EXEC_ID as env to docker exec.
  - Wrap user command with a small shell that: creates the ExecId dir, runs setsid, records PGID,
    waits, and cleans up.
  - v2: include X-Exec-Id in the chunked prelude; stream until exit or disconnect.
- /signal handling:
  - Validate auth and proto; parse exec_id and allowed signal names.
  - Run docker exec sh -lc 'pg=$(cat "$HOME/.aifo-exec/<id>/pgid" 2>/dev/null); [ -n "$pg" ] &&
    kill -s <SIG> -"$pg" || true' against the sidecar container; return 204 on success or ESRCH.
- Disconnect handling:
  - On write error (BrokenPipe/EPIPE) or observed client EOF, send SIGTERM to the PGID; if still
    running after ~2s, send SIGKILL; then finalize and clean registry.

6) Timeouts
- Remove default timeout logic from both v1 and v2 paths.
- Soft-deprecate AIFO_TOOLEEXEC_TIMEOUT_SECS: if set, treat as opt-in max runtime (AIFO_TOOLEEXEC_MAX_SECS):
  at T send INT; after 5s send TERM; after another 5s send KILL. Log a deprecation warning in verbose mode.
- Per-request opt-in max runtime may also be supplied via a dedicated argument (kept internal).

7) Security, correctness, and operability
- Authorization and proto checks unchanged; required for /exec and /signal.
- Limit supported signals to a safe subset: INT, TERM, HUP, KILL; reject others with 400.
- Registry entries are removed on normal/forced exit; no “locking” state remains.
- Verbose logs capture ExecId, tool, args, cwd, signals, disconnects, exit codes, and durations.

Detailed Behavior

- ExecId lifecycle:
  - Created by shim (preferred) or proxy (fallback), stored on registry insert.
  - Included in v2 prelude as X-Exec-Id; for v1 it’s only in logs/registry.
  - Removed from registry when execution ends (normal or forced).

- Sidecar wrapper (conceptual):
  sh -lc '
    set -e
    d="$HOME/.aifo-exec/$AIFO_EXEC_ID"; mkdir -p "$d"
    ( setsid sh -c "exec '"$USER_CMD"'" ) & pg=$!
    printf "%s" "$pg" > "$d/pgid"
    wait "$pg"
    rm -rf "$d" || true
  '

- Signal forwarding (conceptual):
  docker exec <container> sh -lc \
    'pg=$(cat "$HOME/.aifo-exec/<id>/pgid" 2>/dev/null); [ -n "$pg" ] && kill -s TERM -"$pg" || true'

- Disconnect termination:
  - Triggered on streaming write error/EOF (v2) or write error during buffered response (v1).
  - TERM, wait ~2s, then KILL if still running.

Validation and Consistency Checks

- Coherence with current code:
  - http.rs: add Endpoint::Signal and mapping for "/signal"; headers are parsed to lowercase already.
  - proxy.rs: current v2 path uses a timeout watcher and fixed prelude; these must be replaced by
    no-timeout streaming and a prelude builder that includes X-Exec-Id.
  - sidecar.rs: introduce the setsid wrapper and AIFO_EXEC_ID env injection in exec preview.
  - shim.rs: augment generated script to trap signals and send /signal; add X-Aifo-Exec-Id header.
- Logical soundness:
  - setsid + negative PGID kill covers the full subtree of the tool process.
  - Disconnect events are reliably detected through write failures or EOF; termination sequence ensures
    no orphaned jobs persist.
  - Optional max runtime is explicit and graceful; default path is no timeout.
- Backward compatibility:
  - Legacy shims still work; disconnect termination ensures cleanup even without /signal support.
  - v1 buffered mode remains supported; only behavioral difference is lack of streaming.
- Security:
  - Signal endpoint uses the same Authorization and proto checks; no new trust boundary is introduced.

Testing

- Unit tests:
  - Endpoint classification for /signal; auth parity with /exec.
  - Parsing for X-Aifo-Exec-Id and response prelude header insertion logic.
- Integration tests (docker required):
  - Long-running command completes (no default timeout).
  - Ctrl-C once sends INT and stops job with expected exit.
  - Repeated Ctrl-C escalates to TERM then KILL.
  - Shim killed mid-exec → proxy disconnect termination; job stops.
  - Concurrent executions route signals to the correct ExecId.
  - Optional max runtime triggers INT→TERM→KILL sequence on schedule.
- Regression tests:
  - Notifications endpoint unchanged.
  - Tool allowlists and routing are unaffected.
  - Rust bootstrap and PATH behavior unaffected.

Compressed Phased Implementation Plan

Phase 1: Scaffolding and ExecId plumbing
- Add feature flag AIFO_TOOLEEXEC_SIGNALS=1 (default off initially).
- http.rs: add Endpoint::Signal; stub /signal handler in proxy.rs behind the feature flag (501 when off).
- proxy.rs: generate/accept ExecId; add X-Exec-Id to v2 prelude; introduce ExecRegistry (types + insert/remove).
- No behavioral change yet; existing timeouts remain; ensure compatibility.

Phase 2: Sidecar wrapper and no-default-timeout
- sidecar.rs: inject AIFO_EXEC_ID; wrap user command with setsid/pgid file logic.
- proxy.rs: remove default timeout in v2 and v1 paths; add optional max runtime with escalation when
  AIFO_TOOLEEXEC_MAX_SECS (or legacy AIFO_TOOLEEXEC_TIMEOUT_SECS) is set.
- Validate that normal and long-running commands work; registry cleans on exit.

Phase 3: Signal path and disconnect termination
- shim.rs: generate ExecId; send X-Aifo-Exec-Id; add traps for INT/TERM/HUP; POST /signal with escalation.
- proxy.rs: implement /signal to forward INT/TERM/HUP/KILL to PGID; implement disconnect termination (TERM→KILL).
- End-to-end signals functional; graceful on disconnect; no toolbox locking.

Phase 4: Harden and enable by default
- Logging/metrics polish; retries for docker kill path; documentation updates.
- Feature flag default to on; provide env to temporarily disable if necessary.
- Final CI validation across proto v1/v2 on macOS and Linux.

Risk Assessment and Mitigations
- Docker CLI flakiness under signaling: retry once for signal exec; log failures.
- TTY vs signals: continue allocating TTY by default (AIFO_TOOLEEXEC_TTY=1) but allow disabling;
  process group kill does not depend on terminal signals.
- Registry leaks on proxy crash: acceptable in v2; future enhancement could reconcile on next start.

Open Items (tracked for v3+)
- Consider switching to Docker Engine API exec create/start/attach for stronger lifecycle control.
- Possibly allow numeric signals in /signal with validation.
- Optional: ship a tiny init (tini) in images for even better signal semantics.

This v2 plan tightens and validates the design, reduces phases to the minimum safe steps, and aligns
precisely with the current architecture and constraints while achieving the required behavior changes.
