Title: Toolchain Sidecars and Transparent Shims for aifo-coder (v3)

Status: Proposal
Authors: AIFO
Created: 2025-08-29
Target platforms: Linux, macOS, Windows (Docker Desktop / WSL2)
Scope: Runtime toolchain attachment for coding agents (Rust, Node/TypeScript, Python, C/C++, Go)

1) Summary

Enable aifo-coder to dynamically extend a running agent container with language/toolchain capabilities on demand via repeatable --toolchain flags. Tooling runs inside dedicated sidecar containers that share the project workspace and caches with the agent. Inside the agent, tiny “shim” binaries (cargo, node, tsc, python, pip, gcc, go, etc.) are placed on PATH and transparently forward tool invocations to a host-side proxy, which executes the command inside the appropriate sidecar via docker exec. This avoids bloating agent images, keeps security boundaries intact, and works uniformly across Linux, macOS, and Windows.

2) Goals

- Dynamically attach toolchains per run: --toolchain rust, --toolchain node, --toolchain typescript, --toolchain python, --toolchain c-cpp, --toolchain go.
- Keep agent images lean; do not bake heavy toolchains into every agent.
- Avoid mounting the Docker socket into agent containers.
- Make tool usage inside the agent seamless: cargo build, npm test, tsc, pip install, cmake, go test, etc. behave as if installed locally.
- Cross-platform operation (Linux, macOS, Windows with Docker Desktop / WSL2).
- Reuse existing security integrations (AppArmor) and coding conventions already present.
- Provide robust caching via named Docker volumes to accelerate builds.
- Allow image overrides and version pinning per toolchain.

3) Non-goals

- Full-blown remote build orchestration beyond Docker on the same host.
- Multi-node distributed toolchains.
- Replacing native host tools for local (non-container) workflows.

4) Terminology

- Agent: The coding agent container (e.g., aider, crush).
- Sidecar: A long-lived container for a toolchain (rust, node, python, c-cpp, go).
- Shim: A tiny binary on the agent PATH, forwarding tool invocations to the host proxy.
- Proxy: A host-side server spawned by aifo-coder; executes docker exec into sidecars.

5) User Experience and CLI

5.1 Flags
- --toolchain <kind[@version]> (repeatable)
  - kind ∈ {rust, node, typescript, python, c, cpp, c-cpp, go}
  - version optional, e.g., rust@1.80, node@20, python@3.12, go@1.22
- --toolchain-image <kind=image> (repeatable)
  - Override default image used for a kind, e.g., rust=rust:1.80-slim
- --no-toolchain-cache
  - Disable named cache volumes; workspace still shared.
- Optional: --toolchain-bootstrap <kind=mode>
  - For typescript: typescript=global to preinstall TypeScript globally in the node sidecar (off by default).

5.2 Examples
- aifo-coder aider --toolchain rust -- cargo build --release
- aifo-coder crush --toolchain node --toolchain typescript -- npx vitest
- aifo-coder aider --toolchain python -- python -m pytest
- aifo-coder aider --toolchain c-cpp -- cmake -S . -B build && cmake --build build -j
- aifo-coder aider --toolchain go -- go test ./...

6) Architecture

6.1 High-level sequence (when toolchains requested)
1. Generate session ID (random, short) and create user-defined Docker network: aifo-net-<id>.
2. Start sidecar containers for each requested toolchain:
   - docker run -d --rm --name aifo-tc-<kind>-<id> --network aifo-net-<id> [mounts] [env] [--user uid:gid] [apparmor] <image> sleep infinity
3. Start host-side “toolexec proxy”:
   - Listens via a cross-platform URL AIFO_TOOLEEXEC_URL (tcp://host.docker.internal:port with token; optionally unix:///socket on Linux).
   - Generates a random bearer token AIFO_TOOLEEXEC_TOKEN for auth.
4. Prepare shim directory and ensure it is available inside the agent:
   - Either bind-mount a host-built shim directory at /opt/aifo/bin, or use a pre-baked shim in the agent image.
   - Prepend PATH with /opt/aifo/bin.
5. Launch agent container:
   - Join the same network, share the same workspace bind mount.
   - Inject env AIFO_TOOLEEXEC_URL and AIFO_TOOLEEXEC_TOKEN.
   - On Linux, add --add-host=host.docker.internal:host-gateway.
   - Apply same AppArmor profile as sidecars if available.
6. During run, when the agent invokes a tool:
   - The shim (e.g., cargo) connects to the proxy, sending argv/env/cwd and the resolved “tool name”.
   - The proxy maps the tool to a sidecar and runs docker exec with correct user, cwd=/workspace, env, stdio.
   - Exit code is returned; the agent experiences local-like tooling.

6.2 Mapping tools to sidecars
- rust: cargo, rustc
- node/typescript: node, npm, npx, tsc, ts-node
- python: python, python3, pip, pip3
- c-cpp: gcc, g++, clang, clang++, make, cmake, ninja, pkg-config
- go: go, gofmt

6.3 Tool resolution notes
- TypeScript: Prefer project-local ./node_modules/.bin/tsc; else npx tsc; else (optional) global typescript in sidecar if bootstrap enabled.
- Python: Respect /workspace/.venv if present (shim can set VIRTUAL_ENV and PATH accordingly when calling into sidecar).
- C/C++: Allow CC/CXX to be respected from the agent’s env; otherwise default gcc/g++ or clang/clang++.

7) Sidecar images and caching

7.1 Defaults (overridable via --toolchain-image)
- rust: rust:<ver>-slim (default pinned baseline e.g., rust:1.80-slim)
- node/typescript: node:<ver>-bookworm-slim (default node:20-bookworm-slim)
- python: python:<ver>-slim (default python:3.12-slim)
- c-cpp: aifo-cpp-toolchain:<tag> (FROM debian:bookworm-slim; packages: build-essential clang cmake ninja-build pkg-config ccache; optional gdb/valgrind)
- go: golang:<ver>-bookworm (default golang:1.22-bookworm)

7.2 Mounts and caches
- Common:
  - Workspace: -v "$PWD:/workspace"
- Rust:
  - -v aifo-cargo-registry:/usr/local/cargo/registry
  - -v aifo-cargo-git:/usr/local/cargo/git
- Node/TypeScript:
  - -v aifo-npm-cache:/home/coder/.npm
  - Keep node_modules inside /workspace for agent visibility
- Python:
  - -v aifo-pip-cache:/home/coder/.cache/pip
  - Encourage project .venv in /workspace
- C/C++:
  - -v aifo-ccache:/home/coder/.cache/ccache
  - Set CCACHE_DIR; optionally set CC/CXX to ccache-wrapped compilers
- Go:
  - -v aifo-go:/go
  - Set GOPATH=/go, GOMODCACHE=/go/pkg/mod, GOCACHE=/go/build-cache

7.3 UID/GID consistency
- On Unix hosts, use --user "$(id -u):$(id -g)" for sidecars to avoid permission issues on the shared workspace and volumes.
- On Windows hosts, omit --user (Docker Desktop manages UID/GID differently).

8) Cross-platform transport and connectivity

8.1 Transport variable
- AIFO_TOOLEEXEC_URL:
  - tcp://host.docker.internal:<port> (default, works on macOS/Windows; on Linux when host-gateway added)
  - Optional Linux-only: unix:///run/aifo/toolexec.sock (requires bind-mount of a socket path into the agent)
- AIFO_TOOLEEXEC_TOKEN:
  - Random per-session token; required for proxy access.

8.2 Host addressability
- macOS/Windows: host.docker.internal resolves automatically to the host.
- Linux: add --add-host=host.docker.internal:host-gateway for both agent and sidecars when toolchains requested, so containers can reach the host proxy.

8.3 Binding strategy
- macOS/Windows: proxy binds 127.0.0.1:<random high port>.
- Linux: proxy binds 0.0.0.0:<random high port> limited by token and ephemeral lifetime; or unix socket mode when configured.

9) Security

- Do not mount Docker socket into agent or sidecars.
- Apply the same AppArmor profile to sidecars as to the agent when desired_apparmor_profile() returns Some(profile).
- Use per-session network and randomized names to prevent collisions.
- Use short proxy lifetimes; shut down on aifo-coder exit and handle signals.
- Authenticate shim requests with a random bearer token; reject if token missing/incorrect.
- Allow explicit allowlist of tool names routed per sidecar (defense-in-depth).

10) Shim design

10.1 Behavior
- Single small binary “aifo-shim” built for Linux; installed inside agent under multiple names (symlinks): cargo, rustc, node, npm, npx, tsc, ts-node, python, pip, pip3, gcc, g++, clang, clang++, make, cmake, ninja, pkg-config, go, gofmt.
- Determines invoked tool via argv[0].
- Reads AIFO_TOOLEEXEC_URL and AIFO_TOOLEEXEC_TOKEN; if missing, prints helpful guidance and exits with non-zero.
- Captures argv (including spaces/quotes), current working directory, minimal environment, and forwards to proxy.
- Streams stdout/stderr and exit code back to the agent process.

10.2 Embedding vs. bind-mount
- Embedded:
  - Bake aifo-shim into agent images at /opt/aifo/bin; create symlinks at build time.
  - Pros: zero extra mount; out-of-the-box experience.
  - Cons: requires image rebuilds for shim updates.
- Bind-mount:
  - Build aifo-shim on the host and mount into agent at /opt/aifo/bin.
  - Pros: easy iteration; no agent image rebuild.
- Mitigation:
  - Version the shim protocol; agent shim checks compatibility via env or a protocol handshake; allow host mount to override embedded shim when necessary.

11) Proxy design

11.1 Responsibilities
- Listen on AIFO_TOOLEEXEC_URL, validate token.
- Parse request:
  - tool name, argv vector, cwd path, select env passthrough.
- Map tool -> sidecar:
  - rust => aifo-tc-rust-<id>, node/typescript => aifo-tc-node-<id>, python => aifo-tc-python-<id>, c-cpp => aifo-tc-cpp-<id>, go => aifo-tc-go-<id>.
- Execute:
  - docker exec [-u uid:gid] -w /workspace -e ... <sidecar> <tool or resolved subcommand>
  - For TypeScript, resolve ./node_modules/.bin/tsc inside sidecar when present; else run npx tsc.
  - For Python, if /workspace/.venv exists, adjust PATH and VIRTUAL_ENV accordingly for the exec environment.
- Stream stdio and return exit code.

11.2 Reliability and performance
- Concurrency: handle multiple sequential requests; optionally allow limited parallelism (one per sidecar).
- Timeouts: configurable command timeout; default reasonable values with clear error messages.
- Logging: structured logs on the host for troubleshooting; low-verbosity default.

12) Integration with existing code

- src/lib.rs:
  - Reuse container_runtime_path() for docker path resolution.
  - Reuse desired_apparmor_profile()/desired_apparmor_profile_quiet() when running sidecars/agent.
  - Extend build_docker_cmd() or add a sibling function to incorporate:
    - Shim mount and PATH injection
    - --add-host=host.docker.internal:host-gateway (Linux only when toolchains enabled)
    - Toolchain-related environment variables
  - Add helpers:
    - create_session_id()
    - create_session_network(id) and remove_session_network(id)
    - start_sidecar(kind, image, uid/gid, mounts, env, apparmor)
    - stop_sidecar(name)
    - start_toolexec_proxy(bind_opts) -> (url, token, handle)
    - route_tool_to_sidecar(tool) -> kind
- src/main.rs:
  - Extend CLI (Clap) with repeatable --toolchain and --toolchain-image, and --no-toolchain-cache.
  - In main flow, when toolchains requested:
    1) parse requests -> set of kinds + versions
    2) compute images (respect overrides)
    3) create network and start sidecars
    4) start proxy, get URL/token
    5) launch agent with adjusted PATH and env
    6) on exit, cleanup: stop sidecars, remove network, stop proxy
- New crate/binary: aifo-shim
  - Tiny Rust program building static or mostly-static binaries for linux/amd64 and linux/arm64.
  - Optional builds for macOS/Windows for future local use (not needed inside Linux agent).
- Makefile/CI:
  - Build aifo-shim for linux/amd64 and linux/arm64.
  - Package or stage shim artifacts for mounting or for embedding into agent images (multi-arch).
  - Add tests for proxy routing and shim protocol.

13) Toolchain-specific details

13.1 Rust
- Image: rust:1.80-slim (default) or override.
- Volumes: cargo registry/git caches as named volumes; workspace bind mount.
- Env: CARGO_HOME=/usr/local/cargo
- Tools: cargo, rustc

13.2 Node + TypeScript
- Image: node:20-bookworm-slim (default) or override.
- Volumes: NPM cache volume; node_modules in workspace.
- Tools: node, npm, npx, tsc, ts-node
- TSC resolution: prefer project-local ./node_modules/.bin/tsc; fallback to npx tsc; optional global typescript when bootstrapped.

13.3 Python
- Image: python:3.12-slim (default) or override.
- Volumes: pip cache volume; workspace bind mount.
- Tools: python, python3, pip, pip3
- Virtualenv: if /workspace/.venv exists, adjust env for exec.

13.4 C/C++
- Image: aifo-cpp-toolchain:latest (we publish) or override.
  - Based on debian:bookworm-slim with build-essential clang cmake ninja-build pkg-config ccache.
- Volumes: ccache volume for faster rebuilds.
- Tools: gcc, g++, clang, clang++, make, cmake, ninja, pkg-config
- Env: CCACHE_DIR=/home/coder/.cache/ccache; optionally CC="ccache gcc" CXX="ccache g++".

13.5 Go
- Image: golang:1.22-bookworm (default) or override.
- Volumes: aifo-go:/go
- Env: GOPATH=/go, GOMODCACHE=/go/pkg/mod, GOCACHE=/go/build-cache
- Tools: go, gofmt

14) Cleanup and lifecycle

- Ensure best-effort cleanup on normal exit and on SIGINT/SIGTERM:
  - Stop sidecars (docker stop or rely on --rm if exec-ed containers).
  - Remove session network.
  - Stop proxy server.
- Leave named volumes (caches) intact by default; add CLI to purge if desired (e.g., aifo-coder cache clear).

15) Backward compatibility

- When no --toolchain is provided, behavior remains unchanged.
- If PATH shim is present but proxy env is not set, shim prints message guiding the user to use --toolchain or to configure URL/TOKEN; returns non-zero.
- No breaking changes to existing arguments unless explicitly documented.

16) Security considerations

- No Docker socket inside containers.
- AppArmor profile used for sidecars when available (reuse desired_apparmor_profile()).
- Token-authenticated proxy; reject unauthenticated requests.
- Network isolation via ephemeral session network; random container names.
- Limit tool allowlist per sidecar; reject unknown tool names.

17) Performance

- Named volumes for caches to speed up rebuilds: cargo, npm, pip, ccache, go.
- Optional parallelism of proxy requests; default to sequential per sidecar to avoid excessive contention.
- Low-overhead shim and proxy; avoid JSON parsing overhead when possible; use compact framing.

18) Testing plan

- Unit tests:
  - route_tool_to_sidecar() mappings.
  - Proxy request parsing and token validation.
- Integration tests (skipped if docker not available):
  - Start rust sidecar; run cargo --version via proxy; verify output.
  - Node/typescript: npx --version; tsc help with project-local compiler.
  - Python: pip --version; venv activation check.
  - C/C++: cmake --version; simple hello.c build.
  - Go: go version; simple go build.
- End-to-end:
  - Launch agent + rust sidecar; build a sample crate; confirm target/ contents.
- Platform coverage:
  - Linux CI runner with Docker.
  - macOS GitHub runner (Docker Desktop).
  - Windows GitHub runner (Docker Desktop/WSL2).
- Negative tests:
  - Missing token -> unauthorized.
  - Unknown tool name -> rejected.

19) Rollout phases

- Phase 1: Sidecars + explicit toolchain subcommand (bootstrap)
  - aifo-coder toolchain rust -- cargo build
  - Validate sidecar startup, mounts, UID/GID, caches, proxy exec.
- Phase 2: Transparent PATH shims with proxy
  - Introduce aifo-shim bind-mount; add Linux host-gateway logic; enable cross-OS proxy URL.
- Phase 3: Embed shim into agent images
  - Bake /opt/aifo/bin with symlinks; maintain protocol version; allow host override mount.
- Phase 4: Polish and extend
  - Add c-cpp official image publishing flow.
  - Robust Windows/macOS docs; advanced Linux unix-socket mode.
  - CLI quality-of-life (e.g., --with-all-caches, --purge-toolchain-caches).

20) Documentation updates

- README.md: new --toolchain usage, examples per language.
- INSTALL.md: Docker Desktop notes for macOS/Windows; host-gateway note for Linux.
- man/aifo-coder.1: update CLI flags and descriptions.
- examples/: compose-based samples and minimal projects for each toolchain.

21) Open questions

- Should we auto-bootstrap typescript globally in node sidecar when no local tsc is present? Default: no; provide CLI switch.
- Should we allow multiple versions of the same toolchain simultaneously (e.g., rust@1.70 and rust@1.80)? Default: only one version per kind per session.
- Should we support podman automatically when docker is missing? Future consideration.

22) Future work

- Optional podman support where available.
- Toolchain discovery via config files (e.g., .tool-versions style).
- Remote execution option (SSH or cloud runner) sharing the same shim protocol.
- Telemetry opt-in for toolchain usage to improve defaults.

23) Appendix: Implementation checklist (repo oriented)

- CLI (src/main.rs):
  - Add repeatable --toolchain and --toolchain-image; add --no-toolchain-cache.
  - Parse and normalize kinds; derive version/image defaults.
- Lib (src/lib.rs):
  - Add session/network/sidecar/proxy helpers.
  - Integrate AppArmor profile into docker run for sidecars.
  - Extend agent docker run to mount shim dir, inject PATH and AIFO_* vars; add host-gateway.
- New crate/binary: aifo-shim:
  - Implement protocol client (URL/TOKEN; argv/env/cwd; stdio; exit code).
  - Build linux/amd64 and linux/arm64 artifacts.
- Docker images:
  - Add aifo-cpp-toolchain Dockerfile and publishing pipeline.
  - Optionally add agent image layer installing /opt/aifo/bin shim.
- Tests:
  - Add integration tests gated on docker availability.
- CI:
  - Multi-arch builds for shim and images; smoke tests on Linux/macOS/Windows runners.
- Docs:
  - Update README/man/examples.

Rationale

This design delivers dynamic, secure, and repeatable toolchain availability without inflating agent images or compromising host security. It leverages Docker’s isolation, uses named volumes for performance, and provides a transparent developer experience via small shims and a lightweight proxy. The approach scales across languages and platforms and integrates cleanly with the existing aifo-coder codebase and AppArmor support.
