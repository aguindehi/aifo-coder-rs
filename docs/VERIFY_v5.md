# v5 Quick Verification Checklist

Prereqs
- Docker available on host (docker CLI in PATH).
- For Linux UDS checks: Linux host.

Steps

1) Verify baked shim and wrappers (agent image)
- Build images locally:
  make build
- Start an agent container (e.g., aider) and check shim/wrappers:
  - which cargo → /opt/aifo/bin/cargo
  - head -n 20 /opt/aifo/bin/sh → contains exec /bin/sh "$flag" "$cmd; exit"
  - echo $SHELL inside agent → /opt/aifo/bin/sh

2) Native HTTP (TCP)
- Run acceptance test:
  make test-accept-phase4
- Or manual quick check:
  - Start toolchain session + proxy:
    aifo-coder --toolchain rust aider -- --help (Ctrl-C to return)
  - Inside agent container run:
    /opt/aifo/bin/cargo --version
  - Expect version printed and clean prompt on Ctrl-C.

3) Native HTTP (Linux UDS)
- On Linux:
  make test-accept-phase4
- Or manual:
  - Export AIFO_TOOLEEXEC_USE_UNIX=1 before starting the agent session.
  - Verify proxy URL starts with unix:// (logs); agent mounts /run/aifo.

4) Signal UX and exit semantics
- Start a long-running build (e.g., /run cargo build) and press Ctrl-C:
  - Observe shim posts /signal INT; proxy logs show INT → TERM → KILL on disconnect if repeated.
  - Prompt returns cleanly (no lingering shells).
  - By default shim exits 0 on traps and disconnect (legacy mappings via env toggles).

5) Host override
- Generate shims on host and override baked binary:
  - Use toolchain_write_shims(dir) from a small helper or REPL.
  - Run agent with AIFO_SHIM_DIR bound to dir at /opt/aifo/bin:ro.
  - head -n 20 /opt/aifo/bin/sh shows script shim; behavior matches baked shim.

6) Curl retention policy
- Full images (codex/crush/aider): curl present (for agent workflows).
- Slim images (codex-slim/crush-slim/aider-slim): curl removed when KEEP_APT=0.

Troubleshooting
- Set AIFO_TOOLCHAIN_VERBOSE=1 for detailed shim/proxy logs.
- Use AIFO_SHIM_NATIVE_HTTP=0 to force curl fallback temporarily.
- Check $HOME/.aifo-exec/<ExecId>/ markers for diagnostics (agent_ppid, agent_tpgid, tty).
