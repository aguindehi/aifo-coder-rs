# AIFO Coder Toolchain Shim Implementation — v2 (Unified, Image-Baked Rust Shim + Wrappers)

Status: Implemented (design complete; rollout path defined)
Owner: Toolchain/Proxy
Last updated: 2025-09-17

Scope
- This v2 specification supersedes v1 and unifies shim behavior across the system by:
  - Baking a compiled Rust aifo-shim into agent images at /opt/aifo/bin/aifo-shim.
  - Baking sh/bash/dash auto-exit wrappers into agent images at /opt/aifo/bin to prevent
    “fall into shell” symptoms and to ensure SHELL=/opt/aifo/bin/sh resolves to our wrapper.
  - Preserving host override via AIFO_SHIM_DIR for rapid iteration, testing, and debugging.
  - Evolving the Rust shim to a native HTTP client (no curl dependency) while implementing
    the full feature set previously present only in the host-generated POSIX shell shim.
- “Shim” refers to the client-side entrypoint that forwards a tool invocation (cargo, npm, …) in
  the agent container to the in-process aifo-coder proxy, which orchestrates execution in a
  dedicated toolchain sidecar container.

High-level architecture
- Inside the agent container, tool commands (cargo, rustc, npm, node, python, gcc, …) are installed
  as symlinks under /opt/aifo/bin that all exec /opt/aifo/bin/aifo-shim.
- The aifo-shim is a compiled Rust binary that implements protocol v2 streaming with an in-process,
  native HTTP client. It no longer shells out to curl.
- The proxy listens on TCP (default) or unix-domain socket (Linux) and translates /exec requests into
  docker exec in the sidecar. A setsid wrapper in the sidecar runs the tool in its own process group
  and records PGID to $HOME/.aifo-exec/<ExecId>/pgid for later signal delivery.
- On client disconnect or explicit /signal posts, the proxy delivers INT → TERM → KILL to the
  recorded process group (-PGID). The proxy also uses agent-side markers to terminate the transient
  parent shell spawned by the agent’s “/run …” command.

Shim variants in v2

1) Image-baked Rust aifo-shim (new default)
- Location in container:
  - /opt/aifo/bin/aifo-shim (binary)
  - Tool names in /opt/aifo/bin (cargo, npm, …) are symlinks that exec this binary
  - Shell wrappers at /opt/aifo/bin: sh, bash, dash (scripts) with auto-exit behavior
- Provisioned by:
  - Dockerfile: COPY the compiled binary from the rust-builder stage into /opt/aifo/bin/aifo-shim,
    chmod 0755. Also install sh/bash/dash wrappers (from generator logic) into /opt/aifo/bin.
- Methodology:
  - Native HTTP client (no curl): streams stdout in chunked transfer; adds Trailer: X-Exit-Code
    and reads it to determine the final exit status.
  - Adds X-Aifo-Exec-Id header (from env AIFO_EXEC_ID or generated) to correlate the execution.
  - Records per-exec markers under $HOME/.aifo-exec/<ExecId>/:
    - agent_ppid, agent_tpgid (from /proc/self/stat)
    - tty (readlink of fd 0 or fd 1)
    - no_shell_on_tty (marker used by shell wrappers to avoid lingering interactive prompts)
  - Signal handling:
    - Catches INT/TERM/HUP via signal hooks (e.g., signal_hook or equivalent).
    - On first Ctrl-C: POST /signal INT; cleanup; optionally terminate parent shell; exit
      (default exit 0; legacy exit 130 with AIFO_SHIM_EXIT_ZERO_ON_SIGINT=0).
    - On second Ctrl-C: POST /signal TERM; cleanup; optionally terminate parent shell; exit
      (default 0; legacy 143).
    - On third Ctrl-C: POST /signal KILL; cleanup; optionally terminate parent shell; exit
      (default 0; legacy 137).
    - All /signal posts include exec_id and signal in application/x-www-form-urlencoded body.
  - Disconnect behavior (no exit-code header received or write failure):
    - Keeps the marker directory for proxy cleanup of transient shells.
    - If AIFO_TOOLCHAIN_VERBOSE=1: prints
      “aifo-coder: disconnect, waiting for process termination…”
      waits AIFO_SHIM_DISCONNECT_WAIT_SECS (default 1), then prints
      “aifo-coder: terminating now” and a final blank line to give the agent a clean prompt line.
    - Default exit code on disconnect is 0 (AIFO_SHIM_EXIT_ZERO_ON_DISCONNECT=1); set to 0 to
      restore legacy non-zero disconnect code (1).
  - Unix socket support (Linux):
    - When AIFO_TOOLEEXEC_URL starts with unix://, the shim connects to the socket path and uses
      http://localhost/exec as the effective request target for v2 semantics.
- Shell wrappers (baked):
  - sh, bash, dash installed at /opt/aifo/bin.
  - When invoked as sh -c/-lc "cmd" …, they append “; exit” so shells terminate after the command.
  - If interactive and controlling TTY matches a recent tool exec with no_shell_on_tty marker,
    the wrapper exits immediately (prevents lingering prompts).
- Logging:
  - aifo-shim: tool=… cwd=… exec_id=…
  - aifo-shim: preparing request to … (proto=2)
  - On disconnect (verbose): “disconnect, waiting …”, then “terminating now”, then a blank line
- Pros:
  - Single authoritative shim in images; robust, maintainable in Rust; no curl dependency.
  - Full-feature parity with the host-generated shell shim (ExecId, signal traps, disconnect UX).
  - Shell wrappers in-image eliminate “fall into shell” scenarios by default.
- Cons:
  - Requires a rebuild of the image to update shim logic (host override mitigates this).
  - Slightly larger footprint than a shell script (acceptable tradeoff).

2) Host-generated POSIX shell shims (override; unchanged behavior)
- Generated by:
  - src/toolchain/shim.rs::toolchain_write_shims(dir)
- Typical usage:
  - Generate shims on the host into a directory and set AIFO_SHIM_DIR=/path/to/dir so the agent
    container is launched with -v /path/to/dir:/opt/aifo/bin:ro.
  - This fully overrides the image-baked files in /opt/aifo/bin.
- Files written into dir:
  - aifo-shim (script; main client), tool wrappers (cargo, npm, …), and shell wrappers (sh, bash, dash).
- Methodology and behavior:
  - Identical feature set to v1’s host-generated shims, including:
    - ExecId header and /exec v2 streaming with curl.
    - INT/TERM/HUP traps with /signal POST and escalation.
    - Disconnect markers and user-facing disconnect wait messaging.
    - Shell wrappers with auto-exit and no_shell_on_tty checks.
- Pros:
  - Rapid iteration without rebuilding images; complete feature set.
- Cons:
  - Requires host mount to activate; depends on curl in the container.

3) Legacy image-baked POSIX shell shim (deprecated, replaced in v2)
- In v2 images, the inline printf-based shell client at /opt/aifo/bin/aifo-shim is removed and
  replaced by the compiled Rust aifo-shim. The legacy shim remains documented for historical
  reference (see v1), but is no longer installed in v2 images.

Runtime selection and precedence
- Default (no host override):
  - /opt/aifo/bin/aifo-shim (compiled Rust) + sh/bash/dash wrappers baked into the image.
  - All tool symlinks in /opt/aifo/bin resolve to the Rust aifo-shim.
- Host override:
  - If AIFO_SHIM_DIR is set on the host before launching the agent container, docker.rs mounts
    that directory at /opt/aifo/bin:ro, overriding the image-baked binary and wrappers.
  - In this mode the host-generated shims (shell aifo-shim + wrappers) are active.
- Agent shell selection:
  - docker.rs sets SHELL=/opt/aifo/bin/sh so transient shells prefer the baked wrapper, ensuring
    auto-exit and avoiding lingering interactive shells after “/run …”.

Proxy behavior (unchanged from v1, summarized)
- Location: src/toolchain/proxy.rs
- ExecId registry: HashMap<ExecId, ContainerName> populated on /exec.
- Streaming v2 prelude: includes X-Exec-Id header back to the client.
- setsid wrapper in sidecar:
  - docker exec uses sh -c "setsid … exec <tool> …" to create a new PGID and writes the PGID to
    $HOME/.aifo-exec/<ExecId>/pgid. Cleans the dir on normal exit.
- Disconnect handling (v2):
  - On write failure to the client, prints “aifo-coder: disconnect”.
  - After ~150 ms grace (to allow the shim to POST /signal, if any), delivers:
    INT, then after ~500 ms TERM, then after ~1.5 s KILL to -PGID in the sidecar.
  - Best-effort closure of the transient /run shell in the agent container:
    - Reads agent_ppid, agent_tpgid, and tty markers; HUP/TERM/KILL PPID and/or -PGID as needed; may
      inject “exit” and EOF to the controlling TTY.
- Optional max runtime:
  - AIFO_TOOLEEXEC_MAX_SECS (or legacy AIFO_TOOLEEXEC_TIMEOUT_SECS) triggers a background watcher
    that sends INT at T, TERM at T+5s, KILL at T+10s if the tool is still running.
- TTY streaming:
  - AIFO_TOOLEEXEC_TTY=1 allocates -t for docker exec to improve interactive flushing.

Environment variables (shim-side, unified for shell and Rust)
- AIFO_TOOLEEXEC_URL: Proxy endpoint (http://host.docker.internal:<port>/exec or unix://…)
- AIFO_TOOLEEXEC_TOKEN: Bearer token to authorize calls
- AIFO_TOOLCHAIN_VERBOSE: “1” enables extra shim messages and disconnect wait messaging
- AIFO_EXEC_ID: Optional ExecId provided by the caller; otherwise generated by the shim
- AIFO_SHIM_EXIT_ZERO_ON_SIGINT: Default “1” → exit code 0 on INT/TERM/HUP; “0” → legacy 130/143/129
- AIFO_SHIM_EXIT_ZERO_ON_DISCONNECT: Default “1” → return 0 on stream disconnect; “0” → return 1
- AIFO_SHIM_KILL_PARENT_SHELL_ON_SIGINT: Default “1” → attempt to kill transient parent shell
- AIFO_SHIM_DISCONNECT_WAIT_SECS: Integer seconds to wait for proxy logs to flush on disconnect; default 1
- AIFO_SHIM_DIR (host): When set to a directory path, docker.rs mounts it at /opt/aifo/bin:ro,
  overriding the image-baked shim and wrappers

Agent-container/launcher-side (docker.rs)
- SHELL: Set to /opt/aifo/bin/sh to ensure our wrapper handles transient shells
- AIFO_AGENT_IGNORE_SIGINT: Default “0”; when “1”, ignore SIGINT in the container before exec
- PATH: docker.rs prepends /opt/aifo/bin and /opt/venv/bin when present
- AIFO_CODER_CONTAINER_NAME: Exported to identify the agent container (for proxy’s best-effort shell cleanup)

Exit code semantics and mapping (consistent across shims)
- INT/TERM/HUP traps:
  - Default: exit 0 (AIFO_SHIM_EXIT_ZERO_ON_SIGINT=1), which avoids dropping into interactive shells
  - Legacy: 130 (INT), 143 (TERM), 129 (HUP) when AIFO_SHIM_EXIT_ZERO_ON_SIGINT=0
- KILL trap (third Ctrl-C):
  - Default: exit 0; legacy: 137
- Disconnect (no X-Exit-Code read):
  - Default: exit 0; legacy: exit 1
- Normal completion:
  - Exit code is the numeric X-Exit-Code trailer value

Shell wrappers (baked) behavior details
- sh/bash/dash scripts at /opt/aifo/bin implement:
  - Auto-exit after sh -c/-lc "cmd" by transforming to "cmd; exit"
  - Early, silent exit when the controlling TTY matches no_shell_on_tty marker recorded by shim
- This design decisively prevents “fall into a shell” scenarios after Ctrl-C or normal completion.

Pros and cons overview

Image-baked Rust aifo-shim + wrappers (v2 default):
- Pros:
  - Single, authoritative, maintainable implementation; no curl.
  - Full feature set: ExecId header, signal traps and /signal POST, disconnect wait UX,
    per-exec markers, parent-shell termination, native unix-socket client (Linux).
  - Wrapped shells guarantee prompt-safe behavior by default.
- Cons:
  - Requires image rebuild for updates (mitigated by host override).

Host-generated POSIX shell shim (override):
- Pros: Complete feature set; easy to iterate without image rebuild; great for dev/test.
- Cons: Requires host mount (AIFO_SHIM_DIR); depends on curl.

Legacy baked shell shim (v1):
- Pros: Minimal and always present (historical).
- Cons: Missing traps, ExecId header from client, disconnect-wait messaging, parent-shell control.
  Removed in v2 images.

Migration and rollout notes
- Images:
  - Replace the inline printf-based aifo-shim in Dockerfile with COPY --from=rust-builder of the
    compiled binary to /opt/aifo/bin/aifo-shim. Install sh/bash/dash wrappers at /opt/aifo/bin.
- Host override:
  - Preserve AIFO_SHIM_DIR behavior; mount overrides at /opt/aifo/bin continue to take precedence.
- Launcher:
  - Continue setting SHELL=/opt/aifo/bin/sh, ensuring the baked wrapper is used.

Verification
- Inside a running agent container:
  - echo "$SHELL" should print /opt/aifo/bin/sh
  - head -n 40 /opt/aifo/bin/aifo-shim should show ELF header (binary), not a shell script
  - head -n 40 /opt/aifo/bin/sh should show the wrapper script with “; exit” logic
- With AIFO_TOOLCHAIN_VERBOSE=1:
  - Pressing Ctrl-C during a tool run prints:
    aifo-coder: disconnect, waiting for process termination…
    aifo-coder: terminating now
    <blank line>
  - Then returns to the agent prompt without dropping into a shell.

Appendix: Native HTTP client details (Rust shim)
- The Rust shim implements a small HTTP/1.1 client with:
  - POST /exec with Transfer-Encoding: chunked for streaming stdout
  - Trailer: X-Exit-Code parsed from the server’s trailer section
  - Optional unix-domain socket transport on Linux (connect to path from AIFO_TOOLEEXEC_URL)
  - Robust header parsing with a 64 KiB cap and tolerant CRLFCRLF/LFLF handling (mirrors proxy)
- Signal handling uses portable hooks to send /signal posts at the right moments and then exit
  according to configured semantics (zero by default).

This v2 document consolidates, unifies, and elevates shim behavior across shell and Rust paths by
baking the Rust shim and shell wrappers into images, preserving host override, and removing curl
at runtime while achieving full feature parity with the prior shell-based implementation.
