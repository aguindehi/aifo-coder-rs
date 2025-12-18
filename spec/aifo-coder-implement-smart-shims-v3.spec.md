# aifo-coder: implement smart shims (v3)

This document is the Markdown version of `spec/aifo-coder-implement-smart-shims-v2.spec`.
It is intended to be the canonical, review-friendly spec going forward.

---

# aifo-coder: implement smart shims (v2)

Status: draft  
Owner: aifo-coder  
Date: 2025-12-18  

Phase 1 status (spec + design convergence): complete
- End state confirmed: a single fused Rust `aifo-shim` binary is authoritative for both:
  - smart routing (local vs proxy) and
  - proxied execution (existing proxy protocol semantics).
- Confirmed packaging direction:
  - Do not install a proxy-implementing POSIX `/opt/aifo/bin/aifo-shim` script in v2+.
  - Do not require a separate `/opt/aifo/bin/aifo-shim-proxy` artifact.
  - Tool wrappers/symlinks in `/opt/aifo/bin` must invoke the fused Rust `aifo-shim`.

Implementation notes (from current repo state)
- `src/bin/aifo-shim.rs` is the existing full-feature proxy shim implementation.
- `src/aifo-shim/main.rs` currently implements smart routing + delegates proxying via `aifo-toolexec`.
- `src/toolchain/shim.rs` currently generates a POSIX `aifo-shim` script; this must be removed or turned into a thin
  trampoline in Phase 2 so it cannot shadow the Rust binary.

These notes are part of Phase 1 to ensure the spec matches the implementation plan and to prevent diverging shims.

Scope: make the embedded shim (`aifo-shim`) the single source of truth for routing decisions
(local vs proxied), eliminate per-agent PATH special-casing in the launcher, and converge the shim
implementations to one auditable design.

This spec supersedes v1 and addresses gaps discovered during implementation:

- Multiple shim implementations exist in-tree (POSIX script shim, Rust proxy shim, Rust smart shim).
- Launcher PATH logic still varies by agent as a safety belt.
- The intended end-state is a unified, deterministic shim that always sits first in PATH and routes
  correctly based on strict policy.

## Background / problem statement

The agent images place `/opt/aifo/bin` early in PATH and symlink common tools (node/python/...) to
`aifo-shim` so tool execution can be proxied to toolchain sidecars and logged/policy-controlled.

However, runtime-based agents (Node/Python) may spawn their own runtime for internal entrypoints
located outside `/workspace`. If these invocations get proxied into sidecars, agents can fail
because their internal packages are in the agent image, not in the sidecar.

v1 introduced “smart shims”: the shim decides when to run locally vs proxy. During implementation,
additional complexity emerged:

- The repo contains:
  - a POSIX shim generator (`toolchain_write_shims`) that writes `/opt/aifo/bin/aifo-shim` as a
    shell script that proxies.
  - a Rust proxy shim (`src/bin/aifo-shim.rs`) implementing the full proxy protocol.
  - a Rust smart shim (`src/aifo-shim/main.rs`) implementing bypass and proxy delegation.
- PATH ordering is still adjusted per-agent in the launcher to avoid interception hazards.

v2 resolves these inconsistencies by converging to a single shim implementation and making PATH
consistently shim-first.

## Goals (v2)

1. **Single source of truth**: one shim implementation (`aifo-shim`) performs:
   - local-vs-proxy decisions (smart mode) and
   - proxied execution (existing proxy protocol).
2. **Uniform launcher PATH**: the launcher always uses the same PATH shape and always places
   `/opt/aifo/bin` first; no per-agent PATH reordering.
3. **Deterministic, conservative bypass rules**:
   - Smart behavior is opt-in via env and tool toggles (as in v1).
   - Bypass decisions are based only on tool name, argv, cwd, and fixed prefix checks (no
     user-controlled regex policy in v2).
4. **Backwards compatibility**:
   - Existing toolchain runs continue to work (proxy protocol, signals, exit codes, `/notify`
     behavior).
   - Tool entrypoints remain stable (node/python/pip/uv/etc. resolve to `aifo-shim`).
5. **Security**:
   - Local exec uses absolute runtime paths to avoid recursion/PATH injection.
   - Smart bypass remains gated by `AIFO_SHIM_SMART=1` and tool toggles.
   - No embedding of untrusted argv into control scripts (keep current invariants).
6. **Observability**:
   - When `AIFO_TOOLCHAIN_VERBOSE=1`, log a single concise line on smart bypass.

## Non-goals (v2)

- Smart routing for every possible runtime tool beyond node/python (ruby/java/etc).
- Complex user-configurable bypass policies (regex allowlists) in v2.
- Changing proxy protocol versions or sidecar layout.

## Terminology

- **Local execution**: run the real runtime inside the agent container using an absolute runtime
  path (e.g. `/usr/bin/python3`, `/usr/local/bin/node`).
- **Proxied execution**: execute tool through the toolexec proxy (HTTP/UDS) into toolchain
  sidecars, preserving existing behavior (signals, streaming, exit code).

## Design overview

### 1) Single shim implementation and packaging

#### Current state (observed)

At least three shim implementations exist and can be wired at runtime:

- POSIX-generated `/opt/aifo/bin/aifo-shim` (from `toolchain_write_shims()`):
  - curl-based v2 client; handles `/exec`, `/signal`, `/notify`.
- Rust proxy shim (`src/bin/aifo-shim.rs`):
  - full proxy client; supports native HTTP/UDS + curl fallback, signals, `/notify`, trailer
    parsing, disconnect handling, and logging.
- Rust smart shim (`src/aifo-shim/main.rs`):
  - local-vs-proxy decisions for node/python, plus a proxy delegation path.

This violates “single source of truth” and makes it unclear which behavior actually executes when
`/opt/aifo/bin` is first in PATH.

#### v2 target state (fused shim)

There is exactly one authoritative shim implementation:

- **`/opt/aifo/bin/aifo-shim` is a Rust binary** implementing:
  - smart bypass decisions (node/python) and
  - proxied execution, including `/notify` and `/signal`, preserving the existing protocol/UX
    semantics.

In particular, v2 explicitly requires fusing what previously behaved like:

- `aifo-shim` (smart routing front-door) and
- `aifo-shim-proxy` (always-proxy implementation)

into a single binary (`aifo-shim`) with an internal “smart decision” branch.

#### Shim directory contents

The shim directory `/opt/aifo/bin` contains:

- `aifo-shim` (Rust binary, fused)
- tool entrypoints (`node`, `python`, `pip`, etc.) as wrappers/symlinks → `aifo-shim`
- optional shell wrappers (`sh`, `bash`, `dash`) used for session UX

#### Packaging invariants

- The generated POSIX `aifo-shim` script must not be installed as `aifo-shim` in v2.
  - If a POSIX wrapper is retained, it must be a thin `exec` trampoline into the Rust binary and
    must not implement routing/proxy itself.
- Tool wrappers must not create recursion (no `exec aifo-shim` via PATH): prefer
  `exec "$(dirname "$0")/aifo-shim" "$@"`.
- Local exec must always use absolute runtime paths (node/python) to avoid shim recursion.

### 2) Policy plumbing (launcher integration)

The launcher must set these env vars inside agent containers:

- `AIFO_AGENT_NAME=<agent>` (always set).
- `AIFO_SHIM_SMART=1`
  - set for agents known to need smart bypass (v2 list below), and/or allow host override.
- Tool toggles:
  - `AIFO_SHIM_SMART_NODE=1` for node-based agents.
  - `AIFO_SHIM_SMART_PYTHON=1` for python-based agents.

v2 agent mapping (same as v1, explicit):

- Node-based agents: `letta`, `codex`, `crush`, `opencode`
  - set `AIFO_SHIM_SMART=1` and `AIFO_SHIM_SMART_NODE=1`
- Python-based agents: `aider`, `openhands`
  - set `AIFO_SHIM_SMART=1` and `AIFO_SHIM_SMART_PYTHON=1`
- Other agents: do not set smart toggles by default.

### 3) Uniform PATH policy (launcher)

The launcher must stop per-agent PATH reordering.

v2 requirement:

- The agent container command must always be executed with:
  - `PATH="/opt/aifo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:$PATH"`
- `/opt/aifo/bin` is always first (shim-first).

The shim is responsible for correctness.

### 4) Smart-node rules (v2; unchanged from v1 intent)

When invoked as `node` and `AIFO_SHIM_SMART=1` and `AIFO_SHIM_SMART_NODE=1`:

- Parse argv to identify the “main program” (best-effort):
  - Support `--` separator: first arg after `--` is the program.
  - Skip known node options that consume the next value (e.g. `-r/--require`, `--loader`, `--import`, etc.).
  - Ignore eval/print/REPL (`-e/-p`) as “no program path” (proxy by default).
- If program is a path, resolve “absolute-ish”:
  - If it starts with `/`, use it.
  - Else join with cwd.
- If resolved program path is **not** under `/workspace`:
  - Run local node (absolute path):
    - prefer `/usr/local/bin/node`, fallback `/usr/bin/node`
  - Log (when verbose):
    `aifo-shim: smart: tool=node mode=local reason=outside-workspace program=<...> local=<...>`
- Else proxy.

### 5) Smart-python rules (v2; v1 intent)

When invoked as `python` or `python3` and `AIFO_SHIM_SMART=1` and `AIFO_SHIM_SMART_PYTHON=1`:

- If `-m <module>` is present: local python (v2 keeps v1 conservative default).
- Else if first non-flag argv token is a script path:
  - resolve absolute-ish against cwd
  - if not under `/workspace`: local python
  - else proxy
- Local python binary (absolute path):
  - prefer `/usr/bin/python3`, fallback `/usr/local/bin/python3`
- For `pip/pip3/uv/uvx` (if they resolve to the shim):
  - always proxy in v2 (same as v1 non-goal for auto-local bypass).

### 6) Proxy path requirements (must match existing behavior)

The shim proxy path must preserve:

- Authorization header (Bearer token)
- Proto header (`X-Aifo-Proto: 2`)
- Streaming behavior for exec (chunked with trailer `X-Exit-Code`) where applicable
- `/signal` forwarding used by shim signal traps
- `/notify` fast-path behavior for notification tools (e.g. `say`)
- Exit code conventions (86 for “proxy not configured”, etc.)
- Existing disconnect handling semantics (including shell-kill behavior) to avoid lingering shells
  after Ctrl-C or stream disconnects.

v2 decision: **fuse smart-routing into the existing Rust proxy shim implementation** so there is
a single binary:

- Start from `src/bin/aifo-shim.rs` (proxy client + signals + notify).
- Add smart decision logic (node/python) to choose local exec vs proxy path.
- Remove any reliance on:
  - the generated POSIX `aifo-shim` proxy script, and
  - any separate “proxy shim” binary such as `aifo-shim-proxy`.

### 7) Rollout / migration plan (phased)

#### Phase 1: Spec + design convergence (this doc)

- Confirm end-state: fused Rust shim binary is authoritative.
- Confirm that POSIX shim generation will no longer produce a conflicting `aifo-shim` implementation.

#### Phase 2: Fuse shim implementations (critical)

- Merge smart-routing logic from `src/aifo-shim/main.rs` into `src/bin/aifo-shim.rs`.
- Ensure the Rust `aifo-shim` binary supports:
  - smart bypass (local exec) for node/python and
  - proxied execution for everything else (including pip/uv always-proxy).
- Ensure the runtime shim directory installs `aifo-shim` as the Rust binary and does not install
  a proxy-implementing POSIX `aifo-shim` script.

Deliverables:

- One `aifo-shim` binary provides both local and proxied behavior.
- No `aifo-shim-proxy` artifact is required.
- Tool wrappers/symlinks point to `aifo-shim`.

#### Phase 3: Launcher policy wiring (agents)

- Ensure launcher always sets `AIFO_AGENT_NAME`.
- Set smart toggles per agent per section “Policy plumbing”.

Deliverables:

- v2 env contract is enforced for all relevant agent runs.

#### Phase 4: Launcher PATH policy

- Remove per-agent PATH reordering in launcher.
- Always set PATH with `/opt/aifo/bin` first.

Deliverables:

- PATH is uniform across agents.

#### Phase 5: Tests

Add/extend tests to ensure:

1) **Shim decision logic** (unit tests):

- node outside `/workspace` → local
- node under `/workspace` → proxy
- python `-m` → local
- python script under `/workspace` → proxy
- pip/uv always proxy even when smart python enabled

2) **Launcher wiring** (unit tests):

- For node agents: env includes `AIFO_SHIM_SMART=1`, `AIFO_SHIM_SMART_NODE=1`, and `AIFO_AGENT_NAME=<agent>`.
- For python agents: env includes `AIFO_SHIM_SMART=1`, `AIFO_SHIM_SMART_PYTHON=1`, and `AIFO_AGENT_NAME=<agent>`.
- PATH is uniform and shim-first for all agents.

3) **Integration smoke** (optional/ignored; requires Docker):

- Start node-based agent with toolchain enabled and verify it starts.
- Start python-based agent with toolchain enabled and verify it starts.
- Verify that `node` invoked on an agent-internal script outside `/workspace` does not hit the proxy
  (can be inferred by absence of proxy log lines).

#### Phase 6: Validation

- Run `make check`.

## Acceptance criteria

1. With toolchain enabled and `/opt/aifo/bin` first in PATH, node-based agents
   (`letta/codex/crush/opencode`) can start and function without proxying their internal runtime
   entrypoints outside `/workspace`.
2. Python-based agents (`aider/openhands`) can start and function without proxying internal
   module/script execution.
3. Tool invocations under `/workspace` continue to proxy as before.
4. No PATH special-casing remains in launcher code.
5. `/opt/aifo/bin/aifo-shim` is the only shim implementation required at runtime (no `aifo-shim-proxy`,
   no proxy-implementing POSIX script).
6. `make check` passes.

## Notes / known risks and mitigations

- Risk: additional runtime tools beyond node/python might need smart bypass in future.
  Mitigation: keep policy gated and extend conservatively (v3).
- Risk: conflicting shim artifacts in images (POSIX script vs Rust binary).
  Mitigation: phase 2 explicitly eliminates shadowing by ensuring a single installed shim.
