# AIFO Coder — v5 Release Notes (Phased Shim Implementation)

Date: 2025-09-18

Summary
- v5 finalizes a production-grade toolchain shim with a compiled Rust aifo-shim, shell wrappers baked into images, host override support, a native HTTP client (TCP + Linux UDS), signal propagation, and unified UX/logging across shim and proxy. Phase 4 adds acceptance tests and golden checks to freeze behavior before broad rollout.

What’s new
- Image-baked Rust aifo-shim at /opt/aifo/bin/aifo-shim with PATH tool symlinks.
- Shell wrappers (/opt/aifo/bin/sh, bash, dash) that:
  - Auto-exit for -c/-lc command invocations.
  - Exit immediately when controlling TTY matches a recent exec with the no_shell_on_tty marker.
- Host override path (AIFO_SHIM_DIR) remains supported; mount read-only over /opt/aifo/bin to test host-generated shims.
- Native HTTP client (v5.2+):
  - HTTP/1.1 POST /exec with chunked request and tolerant trailer parsing (X-Exit-Code).
  - Transports: TCP everywhere; Linux UDS when AIFO_TOOLEEXEC_URL=unix://… (request target http://localhost/exec).
  - Enabled by default; set AIFO_SHIM_NATIVE_HTTP=0 to force curl-based fallback.
- Signal propagation and exit semantics:
  - INT→TERM→KILL escalation; TERM/HUP handled; default exit code 0 for traps/disconnect (legacy opt-outs available).
  - Parent-shell termination (Linux) to avoid lingering shells on Ctrl-C (env-gated).
- Proxy improvements:
  - v2 streaming with X-Exec-Id prelude; setsid wrapper; PGID recorded to $HOME/.aifo-exec/<ExecId>/pgid; disconnect termination sequence; optional max-runtime escalation.
  - Default TTY enabled for v2; disable with AIFO_TOOLEEXEC_TTY=0.
- Curl retention policy:
  - Slim images remove curl by default when KEEP_APT=0.
  - Coding agent full images retain curl to avoid breaking agent workflows that still depend on it (codex/crush/aider).
  - Builder stages keep curl as needed (e.g., uv install).

Environment toggles (shim/proxy)
- AIFO_SHIM_NATIVE_HTTP=0: force curl fallback (default is native HTTP).
- AIFO_SHIM_EXIT_ZERO_ON_SIGINT=0: legacy non-zero exit codes for traps (default 1 → exit 0).
- AIFO_SHIM_EXIT_ZERO_ON_DISCONNECT=0: legacy non-zero on disconnect (default 1 → exit 0).
- AIFO_SHIM_KILL_PARENT_SHELL_ON_SIGINT=0: disable Linux best-effort parent-shell termination (default 1).
- AIFO_SHIM_DISCONNECT_WAIT_SECS: integer seconds (default 1) for disconnect UX.
- AIFO_TOOLEEXEC_TTY=0: disable v2 TTY allocation.
- AIFO_TOOLEEXEC_MAX_SECS: opt-in max runtime; escalates INT→TERM(+5s)→KILL(+5s).
- AIFO_TOOLEEXEC_USE_UNIX=1 (Linux): proxy uses a unix domain socket and mounts /run/aifo.

Acceptance tests (Phase 4)
- New ignored-by-default tests (run via `make test-accept-phase4`):
  - accept_native_http_tcp: rust sidecar + TCP proxy; cargo --version via streaming; checks exit via trailer.
  - accept_native_http_uds (Linux): same via UDS transport.
  - accept_wrappers: verifies that the sh wrapper appends “; exit” for -c/-lc invocations in agent image.

Upgrade guidance
- Verify active shim:
  - echo $SHELL inside agent should show /opt/aifo/bin/sh.
  - which cargo in agent resolves to /opt/aifo/bin/cargo (symlink to aifo-shim).
- Override shim with host-generated scripts:
  - Generate shims: toolchain_write_shims(dir); run agent with AIFO_SHIM_DIR pointing at dir and bind-mount to /opt/aifo/bin:ro.
- Native HTTP rollout:
  - Default on; use AIFO_SHIM_NATIVE_HTTP=0 for fallback if needed during canary.

Known limitations
- UDS transport is Linux-only.
- Parent-shell termination is best-effort; guarded by env.
- Curl remains in full agent images to accommodate agent workflows that may still use it.

Rollback
- Use AIFO_SHIM_DIR to mount host-generated shims if issues arise.
- Re-tag last known good images if a baked binary regression is found.

Acknowledgements
- Toolchain/Proxy team for cohesive design and parity validation across shell and Rust shims.
