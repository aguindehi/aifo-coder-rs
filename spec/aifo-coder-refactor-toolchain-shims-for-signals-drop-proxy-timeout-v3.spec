# AIFO Coder: Signal Propagation E2E and Removal of Proxy Timeouts (v3)

Status: Implemented
Owner: Toolchain/Proxy
Last updated: 2025-09-15

Purpose
- Deliver robust, minimal-risk signal propagation from shims to the proxy and on to the tool process
  group in the sidecar container.
- Remove the proxy-imposed default timeout while retaining an optional, opt-in max runtime with
  graceful escalation.
- Ensure the toolbox never “locks” after timeouts or client disconnects; state is cleaned reliably.

Key Outcomes
- User signals are propagated reliably: INT → TERM → KILL escalation.
- Client disconnects induce termination (TERM after disconnect, KILL after ~2s).
- No default proxy timeout; optional max runtime via env with graceful escalation.
- Backward compatibility for legacy shims and proto v1 (buffered) and v2 (streaming).

Design Summary

1) ExecId as the unit of control
- Each proxied execution is identified by ExecId (128-bit hex).
- Shim provides X-Aifo-Exec-Id; proxy generates when absent and returns X-Exec-Id in v2 prelude.
- Proxy keeps a per-process registry: ExecId → container name (minimal v3 mapping).

2) Process group supervision in the sidecar
- Tool commands run under setsid in a new process group.
- The PGID is persisted to $HOME/.aifo-exec/<ExecId>/pgid; wrapper waits and cleans the dir.
- Signals target the whole group (kill -s <SIG> -$PGID).

3) Protocol extensions (backward compatible)
- Request header: X-Aifo-Exec-Id: <id> on /exec (optional).
- Response header: X-Exec-Id: <id> (v2 prelude only).
- New endpoint: POST /signal
  - Body: exec_id=<id>&signal=<SIGINT|SIGTERM|SIGKILL|SIGHUP>
  - Auth/proto required.

4) Shim behavior
- Generate ExecId; add X-Aifo-Exec-Id header to /exec.
- Trap INT/TERM/HUP; POST /signal. INT escalates: INT → TERM → KILL on repeated Ctrl-C.
- If shim dies or terminal closes, the HTTP connection drops; proxy disconnect handling terminates.

5) Proxy changes
- Registry: in-process HashMap<ExecId, ContainerName>.
- /exec:
  - Accept/generate ExecId, add to registry; inject AIFO_EXEC_ID env for docker exec.
  - Wrap command with a sh script that: creates exec dir, setsid, records PGID, waits, cleans.
  - v2: include X-Exec-Id in prelude; stream; on write error trigger TERM→KILL sequence.
- /signal:
  - Validate; look up container by ExecId; inside the container: read PGID and kill -s SIG -PGID.
- Timeouts:
  - Default: none. Optional: AIFO_TOOLEEXEC_MAX_SECS or legacy AIFO_TOOLEEXEC_TIMEOUT_SECS.
  - If set, apply INT (at T), TERM (+5s), KILL (+5s) escalation (v2 first; v1 best-effort later).

Security and correctness
- Authorization and proto checks unchanged.
- Allowed signals: INT, TERM, HUP, KILL (case-insensitive). Others → 400.
- Registry cleans on normal or forced exit; no stale “lock” state.

Consistency and Validation
- setsid + negative PGID kill reliably covers subprocesses.
- Disconnect detected via stream write errors (v2); v1 buffered path unaffected except no timeout.
- Legacy shims still work; disconnect cleanup ensures no orphans.

Testing (selected)
- Long-running commands complete (no default timeout).
- Ctrl-C once stops job (INT), repeated escalates to TERM/KILL.
- Shim killed mid-exec → proxy disconnect cleanup.
- Multiple execs with distinct ExecIds.
- Optional max runtime triggers escalation when enabled.

Phased Implementation Plan (compressed)

Phase A: Protocol and scaffolding
- http.rs: add Endpoint::Signal; classify /signal.
- proxy.rs: parse X-Aifo-Exec-Id; include X-Exec-Id in v2 prelude; introduce minimal ExecId→container registry.
- shim.rs: add ExecId header generation (no traps yet).

Phase B: setsid wrapper and env
- sidecar.rs: inject AIFO_EXEC_ID env when provided.
- proxy.rs: wrap docker exec command for v2/v1 (shared wrapper) to create exec dir, setsid, pgid, wait, cleanup.

Phase C: Signals and disconnect termination
- shim.rs: add traps; POST /signal with escalation; unix-socket support preserved.
- proxy.rs: implement /signal; on v2 write errors, send TERM→KILL to PGID; clean registry on exit.

Phase D: Remove default timeout; add optional max runtime
- proxy.rs: remove hard timeouts in v1/v2; introduce optional max runtime env mapping with escalation.

Risks and mitigations
- Docker CLI flakiness under signal load: retry signal once; log failures (verbose).
- TTY behavior: default on; allow AIFO_TOOLEEXEC_TTY=0 to disable.
- Registry persistence: in-memory only; acceptable for v3.

This v3 spec refines consistency, removes ambiguity, and compresses the rollout while aligning with the
current architecture and implementation constraints.
