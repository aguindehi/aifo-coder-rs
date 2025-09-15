# AIFO Coder Signals Implementation Status (2025-09-15)

Summary: Implemented

Scope
- End-to-end signal propagation: shim → proxy → tool process group in sidecar.
- Removal of proxy-imposed default timeouts for tool executions.
- Optional max-runtime escalation (INT at T, TERM at T+5s, KILL at T+10s).
- Disconnect-triggered termination (TERM then KILL).
- Backward compatibility with proto v1 (buffered) and v2 (streaming).

Implemented details
- Protocol
  - /signal endpoint with Authorization and X-Aifo-Proto validation.
  - Shim sends X-Aifo-Exec-Id; proxy generates one if absent and includes X-Exec-Id (v2 prelude).
- Shim
  - Generates ExecId, traps INT/TERM/HUP, POSTs /signal; repeated Ctrl-C escalates INT→TERM→KILL.
  - Unix-socket support preserved.
- Proxy
  - Registry: in-process ExecId → container mapping for routing signals.
  - Docker exec wrapper (v1 and v2): setsid, write PGID to $HOME/.aifo-exec/<ExecId>/pgid, wait, cleanup dir.
  - v2 streaming: prelude with X-Exec-Id; on write error, disconnect termination TERM→KILL.
  - v1 buffered: same wrapper applied; no default timeout; outputs aggregated.
  - Optional max-runtime escalation when AIFO_TOOLEEXEC_MAX_SECS (or legacy AIFO_TOOLEEXEC_TIMEOUT_SECS) > 0.
- Notifications
  - Independent short timeout; configurable via AIFO_NOTIFICATIONS_TIMEOUT_SECS; no change to behavior of tool execs.

Notes
- Signals allowed via /signal: INT, TERM, HUP, KILL (case-insensitive).
- TTY allocation can be disabled via AIFO_TOOLEEXEC_TTY=0 if required by tooling.
- All tests pass: 230 passed, 24 skipped (as of 2025-09-15).
