# AIFO Coder Toolchain Shim Implementation — v4 (Production-Ready Unified Rust Shim + Shell Wrappers, Host Override, Native HTTP)

Status: In progress (v4.0 baked Rust shim + wrappers; v4.1 parity; v4.2 native HTTP; v4.3 hardening)
Owner: Toolchain/Proxy
Last updated: 2025-09-17

Executive summary
- v3 clarified the direction: make a compiled Rust aifo-shim the default in images, bake sh/bash/dash wrappers, preserve host override, achieve full parity with the rich POSIX shell shim, and remove curl by adopting a native HTTP client.
- v4 finalizes this plan with a production-ready specification, precise Dockerfile requirements, explicit signal/UX/logging parity, and an optimized phased implementation plan with verifiable acceptance gates and risk mitigations.

Goals (what v4 delivers)
- Unified shim behavior across all agents:
  - /opt/aifo/bin/aifo-shim is a compiled Rust binary (authoritative).
  - /opt/aifo/bin/{sh,bash,dash} wrappers prevent “drop into shell” after /run … and on Ctrl-C.
  - Tool symlinks under /opt/aifo/bin all exec aifo-shim.
- Feature parity with the host-generated POSIX shim:
  - ExecId header (X-Aifo-Exec-Id) and per-exec markers ($HOME/.aifo-exec/<ExecId>/…).
  - Signal handling: INT/TERM/HUP hooks → POST /signal with escalation (INT → TERM → KILL).
  - Parent-shell termination (Linux, best-effort) and disconnect wait UX in verbose mode.
  - Unified logs (include exec_id) and exit-code semantics (exit 0 defaults).
- Host override preserved:
  - AIFO_SHIM_DIR mounted read-only to /opt/aifo/bin overrides the baked binary and wrappers.
- Native HTTP roadmap:
  - Rust shim replaces curl with a built-in HTTP/1.1 client (TCP + Linux UDS), supporting chunked transfer and X-Exit-Code trailers.
- Production-grade acceptance criteria and tests for each phase.

Validated current state (v3 reality checks)
- Images still install an inline printf-based shell aifo-shim (curl client); compiled Rust shim is not installed into final images.
- sh/bash/dash wrappers are not present in images (SHELL=/opt/aifo/bin/sh cannot help without wrappers).
- Rust shim shells out to curl; no POSIX-style signal traps, no /signal POST escalation, no parent-shell termination; partial disconnect UX only.
- Proxy behavior is correct (INT → TERM → KILL) but original module comment was stale; ensure comments reflect reality.
- Host override via AIFO_SHIM_DIR works and must remain the authoritative dev/debug override.

High-level architecture (unchanged)
- Tool entrypoints (cargo, npm, gcc, …) are symlinks to /opt/aifo/bin/aifo-shim inside the agent container.
- aifo-shim POSTs to the in-process toolexec proxy /exec (proto v2 streaming).
- Proxy runs on the host (TCP or Linux UDS), wraps docker exec with setsid, records PGID to $HOME/.aifo-exec/<ExecId>/pgid in the sidecar, and performs disconnect/signal escalation to -PGID.
- Agent-side markers from shim help the proxy clean up the transient “/run …” shell in the agent container.

Shim variants in v4

1) Image-baked Rust aifo-shim (new default)
- Location in agent container:
  - /opt/aifo/bin/aifo-shim (ELF binary; authoritative client).
  - /opt/aifo/bin/{cargo,rustc,node,npm,npx,tsc,ts-node,python,pip,pip3,gcc,g++,cc,c++,clang,clang++,make,cmake,ninja,pkg-config,go,gofmt,notifications-cmd} → symlinks to aifo-shim.
  - /opt/aifo/bin/sh, /opt/aifo/bin/bash, /opt/aifo/bin/dash → auto-exit wrappers (scripts).
- Provisioning (Dockerfile):
  - COPY --from=rust-builder /workspace/target/release/aifo-shim → /opt/aifo/bin/aifo-shim; chmod 0755.
  - Install wrapper scripts /opt/aifo/bin/sh, /opt/aifo/bin/bash, /opt/aifo/bin/dash with:
    - Auto-exit for -c/-lc “cmd; exit”.
    - Immediate exit when controlling TTY matches a recent tool exec with no_shell_on_tty marker.
  - Create tool symlinks to aifo-shim.
  - Keep curl in base images until v4.2 (native HTTP).
- Methodology (v4.0–v4.1 behavior):
  - v4.0: Rust shim shells out to curl (transitionary; native HTTP in v4.2).
  - Adds X-Aifo-Exec-Id (from env AIFO_EXEC_ID or generated).
  - Records markers: agent_ppid, agent_tpgid (from /proc/self/stat), tty (readlink of fd 0/1), no_shell_on_tty (flag).
  - Verbose (AIFO_TOOLCHAIN_VERBOSE=1):
    - “aifo-shim: tool=<t> cwd=<p> exec_id=<id>”
    - “aifo-shim: preparing request to <url> (proto=2)”
  - Disconnect UX (no X-Exit-Code in trailers):
    - “aifo-coder: disconnect, waiting for process termination…”
    - Sleep AIFO_SHIM_DISCONNECT_WAIT_SECS (default 1).
    - “aifo-coder: terminating now” and a final blank line for a clean prompt.
  - Exit code semantics:
    - Normal: trailer X-Exit-Code numeric.
    - Traps (default): exit 0 (AIFO_SHIM_EXIT_ZERO_ON_SIGINT=1; legacy non-zero if set to 0).
    - Disconnect: exit 0 by default (AIFO_SHIM_EXIT_ZERO_ON_DISCONNECT=1; legacy 1 if set to 0).
  - v4.1 parity adds:
    - Signal hooks (INT/TERM/HUP) → POST /signal with escalation on repeated Ctrl-C.
    - Parent-shell termination on Linux (best-effort HUP/TERM/KILL PPID; also -PGID when PPID is leader), gated by AIFO_SHIM_KILL_PARENT_SHELL_ON_SIGINT=1.
    - Unified verbose lines including exec_id consistently with the shell shim.
- Pros:
  - Single, maintainable client with consistent UX and logs.
  - Wrappers eliminate post-Ctrl-C “drop into shell” by default.
- Cons:
  - Image rebuild required for updates (mitigated by host override).

2) Host-generated POSIX shell shims (override path)
- Generated by: src/toolchain/shim.rs::toolchain_write_shims(dir).
- Activation: AIFO_SHIM_DIR points to dir; mount read-only at /opt/aifo/bin to override baked files.
- Contents: aifo-shim (shell client using curl), tool wrappers, sh/bash/dash wrappers.
- Methodology: Full feature set (ExecId, /signal traps, markers, disconnect wait UX, wrappers).
- Pros: Rapid iteration; feature-complete today.
- Cons: Requires mount; depends on curl in container.

3) Legacy inline POSIX shim (removed in v4 images)
- The inline printf-based aifo-shim is removed from images (replaced by compiled Rust aifo-shim).

Proxy behavior (validated; docstrings aligned)
- Location: src/toolchain/proxy.rs.
- ExecId registry (HashMap<ExecId, ContainerName>) populated on /exec; v2 prelude includes X-Exec-Id.
- setsid wrapper: runs tool with new PGID; writes $HOME/.aifo-exec/<ExecId>/pgid; cleans on normal exit.
- Disconnect (stream write error):
  - eprintln “aifo-coder: disconnect” on a fresh line.
  - After ~150 ms grace, send INT; after ~500 ms, TERM; after ~1500 ms, KILL to -PGID.
  - Best-effort kill of transient “/run …” shell inside the agent container using agent_ppid/agent_tpgid/tty markers; may also inject “exit” and EOF.
- Optional max runtime:
  - AIFO_TOOLEEXEC_MAX_SECS (legacy AIFO_TOOLEEXEC_TIMEOUT_SECS): INT at T, TERM at T+5s, KILL at T+10s.

Environment variables (shim-side; both shell and Rust)
- AIFO_TOOLEEXEC_URL: http://host.docker.internal:<port>/exec or unix://… (Linux).
- AIFO_TOOLEEXEC_TOKEN: Bearer token.
- AIFO_TOOLCHAIN_VERBOSE: “1” → extra shim messages and disconnect wait UX.
- AIFO_EXEC_ID: Optional; otherwise generated by shim.
- AIFO_SHIM_EXIT_ZERO_ON_SIGINT: default “1” (exit 0 on INT/TERM/HUP); “0” → legacy 130/143/129.
- AIFO_SHIM_EXIT_ZERO_ON_DISCONNECT: default “1” (exit 0 on disconnect); “0” → legacy 1.
- AIFO_SHIM_KILL_PARENT_SHELL_ON_SIGINT: default “1”.
- AIFO_SHIM_DISCONNECT_WAIT_SECS: integer seconds (default 1).
- AIFO_SHIM_DIR (host): mount to override /opt/aifo/bin (binary + wrappers).

Launcher behavior (agent container)
- SHELL=/opt/aifo/bin/sh ensures transient shells prefer the wrapper (auto-exit).
- PATH includes /opt/aifo/bin so tool symlinks resolve to aifo-shim.
- AIFO_CODER_CONTAINER_NAME exported for proxy’s best-effort agent-shell cleanup logic.

Exit code semantics (unified across shims)
- Normal completion: numeric X-Exit-Code trailer.
- Traps (INT/TERM/HUP): default exit 0; legacy 130/143/129 when AIFO_SHIM_EXIT_ZERO_ON_SIGINT=0.
- Third Ctrl-C (KILL): default 0; legacy 137.
- Disconnect (no trailer): default 0; legacy 1 when AIFO_SHIM_EXIT_ZERO_ON_DISCONNECT=0.

Dockerfile requirements (production)
- Remove inline printf-based aifo-shim script from all final images.
- COPY compiled Rust aifo-shim into /opt/aifo/bin/aifo-shim; chmod 0755.
- Install sh/bash/dash wrappers into /opt/aifo/bin; chmod 0755.
- Create tool symlinks in /opt/aifo/bin pointing to aifo-shim.
- Keep curl until v4.2 (native HTTP cutoff); then remove from slim images if unused elsewhere.

Unified logging (verbose)
- Shim:
  - aifo-shim: tool=<t> cwd=<p> exec_id=<id>
  - aifo-shim: preparing request to <url> (proto=2)
  - On disconnect: “aifo-coder: disconnect, waiting for process termination…”, “aifo-coder: terminating now”, then a blank line.
- Proxy:
  - aifo-coder: proxy parsed tool=… argv=… cwd=…
  - aifo-coder: docker: docker exec …
  - aifo-coder: proxy exec: proto=v2 (streaming)
  - aifo-coder: proxy result tool=<t> kind=<k> code=<c> dur_ms=<ms>

Native HTTP client (target v4.2)
- HTTP/1.1 client in aifo-shim (Rust):
  - POST /exec with Transfer-Encoding: chunked; Trailer: X-Exit-Code (tolerant header parsing).
  - TCP by default; Linux UDS when AIFO_TOOLEEXEC_URL=unix://path (target http://localhost/exec).
  - Robust boundary detection (CRLFCRLF and LFLF).
  - Treat mid-stream write errors as disconnects (triggering disconnect wait UX and default exit code).

Verification checklist (production)
- No host override:
  - echo “$SHELL” → /opt/aifo/bin/sh.
  - file /opt/aifo/bin/aifo-shim → ELF executable; not a script.
  - head -n 40 /opt/aifo/bin/sh → wrapper script including “; exit”.
- Ctrl-C during tool run:
  - Shim prints disconnect-wait UX lines and a final blank line; prompt returns; no lingering shell.
- Host override:
  - docker preview shows -v <host_shims>:/opt/aifo/bin:ro.
  - head -n 40 /opt/aifo/bin/aifo-shim shows shell script version.
- UDS (Linux): unix mode works (v4.0 via curl; v4.2 via native HTTP).
- Legacy exit codes: With AIFO_SHIM_EXIT_ZERO_ON_SIGINT=0 and AIFO_SHIM_EXIT_ZERO_ON_DISCONNECT=0, legacy codes are returned (130/143/129 and 1).

Risks and mitigations
- Prompt overwrite race: mitigated by shim disconnect wait + final blank line.
- Lingering shells: mitigated by baked wrappers + parent-shell termination + proxy best-effort cleanup.
- Docker/kill flakiness: kill_in_container performs a retry; escalation includes INT → TERM → KILL.
- Curl dependency: retained until native HTTP ships; tracked in phases; removed post v4.2.
- Override precedence: documented and preserved (AIFO_SHIM_DIR wins).

Phased implementation plan (optimized and testable)

Phase 1 — Bake-in (images + wrappers; curl-backed Rust shim)
- Dockerfile:
  - COPY compiled Rust aifo-shim → /opt/aifo/bin/aifo-shim; chmod 0755.
  - Install /opt/aifo/bin/sh, /opt/aifo/bin/bash, /opt/aifo/bin/dash wrappers; chmod 0755.
  - Create tool symlinks → aifo-shim.
  - Keep curl packages.
- Launcher:
  - Continue setting SHELL=/opt/aifo/bin/sh; PATH includes /opt/aifo/bin.
- Acceptance:
  - file /opt/aifo/bin/aifo-shim shows ELF; wrappers present; Ctrl-C returns to agent (no lingering shell).

Phase 2 — Rust parity with shell shim (signals + parent-shell + unified logs)
- aifo-shim (Rust):
  - Add signal hooks for INT/TERM/HUP; POST /signal with escalation on repeated Ctrl-C.
  - Honor AIFO_SHIM_EXIT_ZERO_ON_SIGINT (default “1”); support legacy codes when “0”.
  - Parent-shell termination on Linux: HUP/TERM/KILL PPID when comm matches shell; also -PGID if leader; gated by AIFO_SHIM_KILL_PARENT_SHELL_ON_SIGINT=1.
  - Unified verbose start line: include exec_id.
- Proxy:
  - Ensure module header comment states disconnect INT → TERM → KILL (code already matches).
- Acceptance:
  - Ctrl-C once → posts INT, exit 0; twice → TERM; thrice → KILL; disconnect UX lines appear; prompt clean.

Phase 3 — Native HTTP client (curl-free)
- aifo-shim (Rust):
  - Implement HTTP/1.1 POST /exec with chunked request, parse Trailer: X-Exit-Code.
  - Implement Linux UDS transport (unix://path); target http://localhost/exec.
  - Tolerant CRLFCRLF/LFLF header boundary detection.
- Dockerfile:
  - Remove curl from slim images if unused elsewhere.
- Acceptance:
  - TCP and UDS stable; large output streaming; trailers parsed; disconnect behavior preserved.

Phase 4 — Hardening and release
- E2E tests:
  - Prompt cleanliness under Ctrl-C, override precedence (AIFO_SHIM_DIR), TCP/UDS, mixed sidecars, large output.
- Documentation and release notes:
  - Images contain Rust aifo-shim + wrappers by default.
  - curl removed post Phase 3; how to verify active shim and override with AIFO_SHIM_DIR.
- Keep host override documented and supported for dev/test.

Appendix: Implementation notes and exact content for wrappers
- sh/bash/dash wrapper exact behavior:
  - Auto-exit for -c/-lc “cmd” by executing “cmd; exit”.
  - If interactive, read controlling TTY from /proc/$$/fd/{0,1,2}; when it matches a $HOME/.aifo-exec/<ExecId>/tty and no_shell_on_tty exists, exit immediately.
- Parent-shell termination heuristic (Linux):
  - Consider comm in {sh,bash,dash,zsh,ksh,ash,busybox,busybox-sh}; avoid broad PGID kills unless PPID is the group leader; keep sleeps brief (50–100 ms) between HUP/TERM/KILL.

This v4 specification finalizes a production-ready plan: a single, image-baked Rust shim with full parity and crisp UX, shell wrappers that eliminate lingering shells, a preserved host override path, and a native HTTP roadmap that removes curl — all with optimized phases and clear acceptance gates.
