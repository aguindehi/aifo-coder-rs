# aifo-coder: implement smart shims (v1)

Status: draft  
Owner: aifo-coder  
Date: 2025-12-18  
Scope: add “smart shim” behavior to the embedded Rust shim (`aifo-shim`) so that
coding agents can safely run their own runtime toolchains locally while still
routing general tool execution through the toolchain proxy/sidecars.

This spec is **cross-agent** and addresses the core conflict observed with Letta:
`/opt/aifo/bin/node -> aifo-shim` causes node-based agents to accidentally route
their internal Node runtime through the toolchain proxy, breaking agent startup
and/or internal operation.

## Background / problem statement

aifo-coder agent images embed a shim directory `/opt/aifo/bin` and set it early
in PATH. Many tool names (node/python/…) are symlinked to `aifo-shim`:

- `/opt/aifo/bin/node -> aifo-shim`
- `/opt/aifo/bin/python -> aifo-shim`
- etc.

This is desired for toolchain routing (proxy -> sidecars), but it conflicts with
coding agents that **depend on a runtime tool** themselves:

- Node-based agents (e.g. Letta Code, Codex, Crush, OpenCode) may spawn `node`
  internally to run their own JS entrypoints or helper scripts installed in the
  agent image.
- Python-based agents (e.g. Aider, OpenHands) may spawn `python` internally for
  helper scripts/modules installed in the agent image.
- Similar patterns can exist for other runtime-based agents.

If these runtime invocations are routed into sidecars, they can fail because:
- the agent’s own installed package is inside the agent container image, not in
  the sidecar image,
- runtime environment differs (PATH, HOME, caches),
- sidecars may intentionally be minimal.

We cannot require “agent cooperation” (agents may invoke tools arbitrarily), and
we cannot rename tools globally. We therefore implement **smart shim decisions**
inside the shim itself: the shim determines when it must execute a local runtime
binary instead of proxying.

## Goals (v1)

1. Prevent node-based coding agents from breaking due to `node` resolving to the shim.
2. Prevent python-based coding agents from breaking due to `python` resolving to the shim.
3. Keep existing toolchain proxy behavior for typical developer tool invocations
   in the workspace (`/workspace`), so that toolchains remain useful.
4. Keep changes minimal, safe, and auditable:
   - default behavior must remain unchanged unless explicitly enabled by env
     (opt-in).
   - bypass rules must be deterministic and conservative to avoid creating an easy
     “escape hatch” around proxy logging/policy.

## Non-goals (v1)

- Perfectly routing *all* possible runtime subprocess patterns for all future
  agents. v1 defines an extensible policy mechanism and implements robust rules
  for Node and Python (the current runtime conflicts).
- Changing the toolchain proxy protocol or sidecar layout.
- Changing global tool names or requiring users to call alternative names.

## Terminology

- **Local execution**: run the real runtime binary inside the coding agent
  container (e.g. `/usr/local/bin/node`, `/usr/bin/python3`) without proxying.
- **Proxied execution**: current behavior; shim POSTs `/exec` to tool-exec proxy,
  which runs the tool in a sidecar and streams output/exit code back.

## Design overview

### 1) Smart-shim policy is per-agent and per-tool (opt-in)

The shim process sees only tool name + argv + cwd + env. To make decisions
consistent across agents, we introduce explicit env-driven policy:

- `AIFO_SHIM_SMART=1`
  - master opt-in for smart behavior in the shim.
- `AIFO_AGENT_NAME=<agent>`
  - agent label (aider/codex/crush/openhands/opencode/plandex/letta/…).
- Tool-specific toggles (v1 implements):
  - `AIFO_SHIM_SMART_NODE=1`
  - `AIFO_SHIM_SMART_PYTHON=1`

The launcher (docker run builder) is responsible for setting these env vars for
each coding agent container.

Default: smart shims are OFF unless explicitly enabled.

### 2) Conservative decision rule: “outside /workspace implies runtime-local”

We treat `/workspace` as the user project mount. The most reliable classifier
without agent cooperation is script/module location:

- If a runtime tool (`node` or `python`) is asked to execute a script/module that
  is **outside** `/workspace`, it is likely part of the agent’s own runtime or
  internal dependencies and must run locally.
- If it executes something under `/workspace`, it is more likely a project/tool
  operation that may benefit from proxy routing into toolchain sidecars.

This keeps the “special-case list” small, while providing correct behavior for
common agent runtime patterns.

### 3) Smart-node rules (v1)

When invoked as `node` and `AIFO_SHIM_SMART=1` and `AIFO_SHIM_SMART_NODE=1`:

- Parse argv to identify the “main program” (best-effort):
  - Support `--` separator: first arg after `--` is the program.
  - Skip known node options that consume the next value (`-r/--require`,
    `--loader`, `--import`, `--eval`/`-e`, `--print`/`-p`, etc.).
  - The first token that does not start with `-` after option handling is treated
    as the program path.
- If program is a path:
  - Resolve to an absolute-ish path:
    - if it starts with `/`, use it
    - else join with cwd (best effort; do not require canonicalize to succeed)
- If the resolved program path is **not** under `/workspace`:
  - Execute local node (must be an absolute path, to avoid recursion):
    - Prefer `/usr/local/bin/node`, fallback `/usr/bin/node`
- Otherwise:
  - Use proxied execution (existing behavior).

Edge cases:
- No program path (REPL) or `-e/-p` forms:
  - v1 default: proxy (keeps toolchain behavior for interactive/project usage).
  - If this breaks a specific agent in practice, add a narrow rule in v2.

### 4) Smart-python rules (v1)

When invoked as `python`, `python3`, `pip`, `pip3`, `uv`, or `uvx` and
`AIFO_SHIM_SMART=1` and `AIFO_SHIM_SMART_PYTHON=1`:

- For `python`/`python3`:
  - Detect “program”:
    - `python /path/to/script.py ...`
    - `python -m module ...` (module is not a path; treat as local when module is
      not workspace-related)
  - If executing a script path outside `/workspace` => local python.
  - For `-m module`:
    - v1 default: local if module is not obviously workspace-relative (module
      names never are), because python-based agents frequently use `-m` internally.
- For `pip`/`pip3`/`uv`/`uvx`:
  - v1 default: proxied unless we see strong evidence it is internal runtime
    management (this is tricky and can be security sensitive). Therefore:
    - do not implement automatic local bypass for pip/uv in v1.
    - only `python`/`python3` get smart bypass in v1.

Local python binary:
- Prefer `/usr/bin/python3` (common in Debian images), fallback `/usr/local/bin/python3`.

### 5) Security and correctness constraints

- Smart bypass must be **gated** by `AIFO_SHIM_SMART=1` and tool-specific toggles.
- The decision must be deterministic and based on:
  - tool name
  - argv/cwd
  - fixed prefix check: `/workspace`
- The shim must never “guess” based on untrusted patterns from env in v1 (no
  regex allowlists), to avoid injection-by-policy and to keep behavior auditable.
- Local exec must always be performed using an **absolute path** to avoid shim
  recursion and PATH tampering.
- When local exec happens:
  - shim should preserve the existing disconnect-handling markers (best-effort),
    but must not attempt to proxy signals via `/signal` (because no proxy exec id).
  - exit status should be the local process exit status.

### 6) Launcher integration (v1)

The docker runner must set:
- `AIFO_SHIM_SMART=1`
- `AIFO_AGENT_NAME=<agent>`

And enable tool toggles per agent:

- Node-based agents:
  - `AIFO_SHIM_SMART_NODE=1`
  - Agents: `letta`, `codex`, `crush`, `opencode`
- Python-based agents:
  - `AIFO_SHIM_SMART_PYTHON=1`
  - Agents: `aider`, `openhands`

Agents that are not runtime-tool dependent:
- `plandex`: smart toggles not required.

Note: This spec intentionally targets “runtime tools” (node/python). It does not
introduce smart routing for build tools (cargo/gcc/…) in v1.

### 7) Observability / debugging

- Add optional verbose log lines from the shim when smart mode triggers:
  - Controlled by existing `AIFO_TOOLCHAIN_VERBOSE=1`
  - Emit one line: `aifo-shim: smart: tool=node mode=local reason=outside-workspace program=/usr/local/...`
- Do not change user-visible strings elsewhere.

### 8) Test plan (v1)

Unit tests (no Docker required) should validate:

1) Shim decision logic:
- Given tool `node` and argv pointing to a script under `/usr/local/...`,
  decision is LOCAL.
- Given tool `node` and argv pointing to `/workspace/...`, decision is PROXY.
- Given tool `python` and argv `-m some.module`, decision is LOCAL (when enabled).
- Given tool `python` and argv `/workspace/script.py`, decision is PROXY.

2) Preview wiring:
- For each node-based agent preview, ensure `-e AIFO_SHIM_SMART=1` and
  `-e AIFO_SHIM_SMART_NODE=1` is present.
- For each python-based agent preview, ensure `-e AIFO_SHIM_SMART=1` and
  `-e AIFO_SHIM_SMART_PYTHON=1` is present.

Optional ignored integration tests (Docker required):
- Run a node-based agent `--help` with toolchain enabled and verify it starts.
- Run a python-based agent `--help` with toolchain enabled and verify it starts.

## Acceptance criteria

1) With toolchain enabled and shim directory present in PATH, node-based agents
   (letta/codex/crush/opencode) can start and function without `node` being
   proxied for their own internal entrypoints outside `/workspace`.
2) Python-based agents (aider/openhands) can start and function without `python`
   being proxied for their internal modules/scripts outside `/workspace`.
3) `make check` passes.

## Phased implementation plan (v1)

Phase 1: Spec + policy plumbing
- Define env variables in launcher and ensure they are injected into agent
  containers.

Phase 2: Smart-node implementation in Rust shim
- Implement deterministic parser for node argv to find main program.
- Implement outside-/workspace check and local exec.

Phase 3: Smart-python implementation in Rust shim
- Implement python argv rules (`script.py` and `-m module`) and local exec.

Phase 4: Launcher wiring per agent
- Set toggles for node-based and python-based agents consistently.

Phase 5: Tests
- Add unit tests for decision logic.
- Add/extend integration tests for preview env injection.

Phase 6: Validation
- Run `make check`.
- Manual smoke:
  - `aifo-coder --toolchain node letta -- --help`
  - `aifo-coder --toolchain python openhands -- --help` (or similar)
  - ensure no shim recursion and no toolchain proxy errors.

## Future extensions (explicitly out of scope for v1)

- Add fine-grained allowlists (regex/prefix lists) guarded by strict sanitization.
- Extend smart bypass to other runtime tools when needed (ruby/perl/java).
- Add an optional “always local for node/python when AIFO_AGENT_NAME=<x>” mode.
- Add metrics for local-vs-proxied decisions in OTEL builds.
