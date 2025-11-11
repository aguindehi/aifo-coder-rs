Title: Toolchain Sidecars, Streaming Tool Exec, and Transparent Shims for aifo-coder (v4)

Status: Accepted (implementation-complete)
Authors: AIFO
Created: 2025-09-10
Target platforms: Linux, macOS, Windows (Docker Desktop / WSL2 / Colima)
Scope: Runtime toolchain attachment for coding agents (Rust, Node/TypeScript, Python, C/C++, Go) with streaming proxy protocol v2

1) Summary

aifo-coder can dynamically extend a running agent container with language/toolchain capabilities on demand via repeatable --toolchain flags. Tooling runs inside dedicated sidecar containers that share the project workspace and language-specific caches with the agent. Inside the agent, small shim binaries (cargo, node, npm, npx, tsc, ts-node, python, pip, gcc, cc, g++, c++, clang, clang++, make, cmake, ninja, pkg-config, go, gofmt, etc.) live on PATH and transparently forward tool invocations to a host-side proxy, which executes the command inside the appropriate sidecar via docker exec.

This v4 spec upgrades the tool-exec protocol to support streaming (protocol v2, HTTP/1.1 chunked with trailer X-Exit-Code) so users see live command output. It also introduces dynamic routing of common development tools (make, cmake, ninja, pkg-config, gcc, g++, clang, clang++, cc, c++) to the first running sidecar that provides them (preferring c-cpp, then rust, go, node, python), reducing the need to start multiple sidecars when not required.

2) Goals

- Dynamically attach toolchains per run: --toolchain rust, --toolchain node, --toolchain typescript, --toolchain python, --toolchain c-cpp, --toolchain go.
- Keep agent images lean; do not bake heavy toolchains into every agent.
- Avoid mounting the Docker socket into containers; keep the proxy on the host with a bearer token.
- Seamless tool usage inside the agent via PATH shims; protocol v2 streams output live.
- Cross-platform operation (Linux, macOS, Windows Docker Desktop/WSL2; Linux with Colima).
- Robust caching via named Docker volumes and optional host cache mounts.
- Image overrides and version pinning per toolchain; align Rust toolchain image with v7 requirements.
- Dynamic dev-tool routing to avoid unnecessary sidecars; include cc/c++ shims.

3) Non-goals

- Remote multi-node build orchestration beyond Docker on the same host.
- Replacing native host tools for non-container workflows.

4) Terminology

- Agent: The coding agent container (e.g., aider, crush, codex).
- Sidecar: A long-lived container for a toolchain (rust, node, python, c-cpp, go).
- Shim: Tiny client program in the agent PATH, forwarding tool invocations to the host proxy.
- Proxy: A host-side server spawned by aifo-coder; executes docker exec into sidecars.

5) User Experience and CLI

5.1 Flags
- --toolchain <kind[@version]> (repeatable)
  - kind ∈ {rust, node, typescript, python, c, cpp, c-cpp, go}
  - version optional, e.g., rust@1.80, node@20, python@3.12, go@1.22
- --toolchain-image <kind=image> (repeatable)
  - Override default image, e.g., rust=aifo-coder-toolchain-rust:1.80 or python=python:3.12-slim
- --no-toolchain-cache
  - Disable named cache volumes; workspace still shared.
- Optional: --toolchain-bootstrap <kind=mode>
  - typescript=global to preinstall TypeScript globally in the node sidecar (off by default).

5.2 Examples
- aifo-coder aider --toolchain rust -- cargo build --release
- aifo-coder crush --toolchain node --toolchain typescript -- npx vitest
- aifo-coder aider --toolchain python -- python -m pytest
- aifo-coder aider --toolchain c-cpp -- make -j
- aifo-coder aider --toolchain go -- go test ./...
- With dev-tool routing: aifo-coder aider --toolchain rust -- make  (routes to rust sidecar if c-cpp isn’t running)

6) Architecture

6.1 High-level sequence (when toolchains requested)
1. Generate session ID (random, short) and create user-defined Docker network: aifo-net-<sid>.
2. Start sidecar containers for each requested toolchain:
   - docker run -d --rm --name aifo-tc-<kind>-<sid> --network aifo-net-<sid> [mounts] [env] [--user uid:gid] [apparmor] <image> sleep infinity
3. Start host-side “toolexec proxy”:
   - Listens via AIFO_TOOLEEXEC_URL:
     - macOS/Windows: tcp://host.docker.internal:<port>
     - Linux (TCP): tcp://0.0.0.0:<port> with host-gateway add-host for containers to reach host
     - Linux (unix): unix:///run/aifo/toolexec.sock (AIFO_TOOLEEXEC_USE_UNIX=1)
   - Generates a random bearer token AIFO_TOOLEEXEC_TOKEN for auth.
4. Prepare shim directory and ensure it is available inside the agent:
   - Agent images embed aifo-shim at /opt/aifo/bin, with symlinks for tools (including cc/c++).
   - PATH includes /opt/aifo/bin in agent stages.
5. Launch agent container:
   - Join the same network, share the same workspace bind mount.
   - Inject env AIFO_TOOLEEXEC_URL and AIFO_TOOLEEXEC_TOKEN.
   - On Linux TCP, add --add-host=host.docker.internal:host-gateway when toolchains enabled.
   - Apply same AppArmor profile as sidecars when available.
6. During run, when the agent invokes a tool:
   - The shim connects to the proxy, sending argv/env/cwd and the resolved tool name.
   - The proxy maps the tool to a sidecar and runs docker exec with correct user, cwd=/workspace, env, stdio.
   - Protocol v2 streams stdout/stderr in real time; exit code is returned via an HTTP trailer.

6.2 Tool mapping and dynamic fallback
- Family mapping:
  - rust: cargo, rustc
  - node/typescript: node, npm, npx, tsc, ts-node
  - python: python, python3, pip, pip3
  - c-cpp: gcc, g++, clang, clang++, make, cmake, ninja, pkg-config, cc, c++
  - go: go, gofmt
- Dev-tool routing (make, cmake, ninja, pkg-config, gcc, g++, clang, clang++, cc, c++):
  - Preferred order: c-cpp, rust, go, node, python
  - Select the first running sidecar that reports command -v <tool> success (cached per session)
  - If none running provides it, return a clear error suggesting an appropriate toolchain

6.3 Tool resolution notes
- TypeScript: Prefer project-local ./node_modules/.bin/tsc; else npx tsc; else global tsc when bootstrapped.
- Python: Respect /workspace/.venv if present (proxy sets VIRTUAL_ENV and PATH accordingly when executing).
- Rust: Set CARGO_HOME=/home/coder/.cargo; do not override PATH via -e at runtime; export CC=gcc and CXX=g++ for build scripts.
- C/C++: Allow CC/CXX to be respected from the agent’s env; otherwise default to gcc/g++ or clang/clang++.

7) Sidecar images and caching

7.1 Defaults (overridable via --toolchain-image)
- rust: aifo-coder-toolchain-rust:<version|latest> (preferred); optionally official rust:<version>-bookworm when AIFO_RUST_TOOLCHAIN_USE_OFFICIAL=1 (bootstrap on exec).
- node/typescript: node:<ver>-bookworm-slim (default node:20-bookworm-slim)
- python: python:<ver>-slim (default python:3.12-slim)
- c-cpp: aifo-coder-toolchain-cpp:latest (debian:bookworm-slim; build-essential clang cmake ninja pkg-config ccache; cc/c++ hardlinks)
- go: golang:<ver>-bookworm (default golang:1.22-bookworm)

7.2 Mounts and caches
- Workspace: -v "$PWD:/workspace"
- Rust:
  - Host-preferred per-path mounts when available:
    - $HOME/.cargo/registry -> /home/coder/.cargo/registry
    - $HOME/.cargo/git -> /home/coder/.cargo/git
  - Fallback to named volumes:
    - aifo-cargo-registry:/home/coder/.cargo/registry
    - aifo-cargo-git:/home/coder/.cargo/git
  - Back-compat legacy mounts: /usr/local/cargo/{registry,git}
  - Ownership init for named volumes (one-shot chown uid:gid) via helper container; stamp file .aifo-init-done
- Node/TypeScript:
  - aifo-npm-cache:/home/coder/.npm
  - node_modules stays in /workspace
- Python:
  - aifo-pip-cache:/home/coder/.cache/pip
- C/C++:
  - aifo-ccache:/home/coder/.cache/ccache; set CCACHE_DIR
- Go:
  - aifo-go:/go; set GOPATH, GOMODCACHE, GOCACHE

7.3 UID/GID consistency
- On Unix hosts, use --user "$(id -u):$(id -g)" for sidecars to avoid permission issues on shared mounts.
- On Windows hosts, omit --user (Docker Desktop manages UID/GID differently).

8) Transport and connectivity

8.1 Variables
- AIFO_TOOLEEXEC_URL:
  - tcp://host.docker.internal:<port> on macOS/Windows and Linux (with host-gateway)
  - unix:///run/aifo/toolexec.sock on Linux (AIFO_TOOLEEXEC_USE_UNIX=1)
- AIFO_TOOLEEXEC_TOKEN:
  - Random per-session token; required for proxy access.
- AIFO_TOOLEEXEC_TIMEOUT_SECS:
  - Per-request timeout for command execution and client read; default 60.

8.2 Host addressability
- macOS/Windows: host.docker.internal resolves automatically to the host.
- Linux: add --add-host=host.docker.internal:host-gateway for agent and sidecars in TCP mode.

8.3 Binding strategy
- macOS/Windows: proxy binds 127.0.0.1:<random high port>.
- Linux (TCP): proxy binds 0.0.0.0:<random high port> limited by token and ephemeral lifetime.
- Linux (unix): proxy binds at /run/aifo/aifo-<sid>/toolexec.sock; agent shims use curl --unix-socket.

9) Security

- Do not mount Docker socket into agents or sidecars.
- Apply the same AppArmor profile to sidecars as to the agent when desired_apparmor_profile() returns Some(profile).
- Per-session network and randomized names reduce collision risk.
- Token-authenticated proxy; reject unauthenticated requests.
- Sidecar-specific allowlists enforce permitted commands; dynamic routing respects allowlists.

10) Shim design

10.1 Behavior
- Agent images embed aifo-shim at /opt/aifo/bin with symlinks for:
  - cargo, rustc, node, npm, npx, tsc, ts-node, python, pip, pip3, gcc, g++, cc, c++, clang, clang++, make, cmake, ninja, pkg-config, go, gofmt, notifications-cmd
- Determines invoked tool via argv[0].
- Reads AIFO_TOOLEEXEC_URL and AIFO_TOOLEEXEC_TOKEN; if missing, prints guidance and exits 86.
- Protocol v2: streams output via curl --no-buffer; writes headers+trailers to a temp file; parses X-Exit-Code and exits with that code.
- Supports unix:// URLs on Linux via curl --unix-socket path with URL rewrite to http://localhost/exec.

10.2 Protocols
- v2 (recommended): streaming via HTTP/1.1 chunked; exit code via trailer X-Exit-Code; stderr merged into stdout by wrapping exec with sh -lc '<cmd> 2>&1'.
- v1 (legacy): buffered response with Content-Length; X-Exit-Code header; in verbose mode prefix/suffix a newline to avoid UI wrap artifacts.

10.3 Embedding vs. bind-mount
- Agent images embed the shim for out-of-box operation; no extra mount required.
- A helper (toolchain_write_shims) can write a host shim set for development and mount via AIFO_SHIM_DIR.

11) Proxy design

11.1 Responsibilities
- Listen on AIFO_TOOLEEXEC_URL, validate token (Authorization or Proxy-Authorization headers).
- Parse request: tool name, argv, cwd (form-encoded); require X-Aifo-Proto: 1 or 2 when Authorization is valid else 426 Upgrade Required.
- Dynamic dev-tool routing:
  - For make, cmake, ninja, pkg-config, gcc, g++, clang, clang++, cc, c++:
    - Preferred kinds: c-cpp, rust, go, node, python
    - Select first running sidecar where command -v <tool> succeeds (per-session cache)
- Execute docker exec with correct user, cwd=/workspace, env, stdio:
  - For TypeScript, resolve ./node_modules/.bin/tsc inside sidecar; else npx tsc.
  - For Rust, set CARGO_HOME=/home/coder/.cargo; export CC=gcc and CXX=g++.
  - For Python venv, set VIRTUAL_ENV and PATH when /workspace/.venv/bin exists.

11.2 Streaming (v2) and buffered (v1)
- v2 streaming:
  - Send early headers:
    - HTTP/1.1 200 OK
    - Content-Type: text/plain; charset=utf-8
    - Transfer-Encoding: chunked
    - Trailer: X-Exit-Code
    - Connection: close
  - Spawn docker exec wrapped in sh -lc '<cmd> 2>&1'
  - Read child stdout and write chunks: "<hexlen>\r\n<data>\r\n"
  - On process exit, write final "0\r\nX-Exit-Code: <code>\r\n\r\n"
  - No write timeout on the socket
- v1 buffered:
  - Spawn docker exec and collect stdout/stderr
  - Send 200 OK with Content-Length and X-Exit-Code header; then body
  - In verbose mode, prefix and suffix a newline to avoid UI interleaving artifacts

11.3 Reliability and performance
- Concurrency: handle sequential requests; cache tool availability per sidecar to minimize exec probes.
- Timeouts: configurable command timeout; default 60s with 504 on timeout.
- Logging: concise, line-safe logs on stderr (flush stdout/stderr; clear current line) including durations.

12) Integration with existing code

- src/toolchain.rs:
  - Session/network management; sidecar start/stop; run/exec previews
  - Ownership init for rust caches; fast linker flags; proxy env passthrough
  - Proxy implementation (TCP + Linux unix), v1/v2 protocols
  - Dynamic dev-tool routing helpers (container_exists, tool_available_in, per-session cache)
  - Shim writer (toolchain_write_shims)
  - AppArmor profile handling
- Dockerfile:
  - Embed Rust-built aifo-shim binary then replace with POSIX shell client using curl (v2 streaming)
  - Add symlinks for all tools including cc/c++
  - PATH includes /opt/aifo/bin in agent stages
- toolchains/rust/Dockerfile (v7 alignment):
  - CARGO_HOME=/home/coder/.cargo
  - PATH="${CARGO_HOME}/bin:/usr/local/cargo/bin:${PATH}"
  - Preinstall clippy, rustfmt, rust-src, llvm-tools-preview; cargo-nextest
  - Install system deps used by common crates
  - Optional BuildKit secret to add a corporate CA to trust MITM proxies
- toolchains/cpp/Dockerfile:
  - build-essential, clang, cmake, ninja, pkg-config, ccache, git, ca-certificates
  - Hardlink cc -> gcc and c++ -> g++ to satisfy build scripts
- Makefile:
  - Build/publish rules for toolchains; pass RUST_TAG for rust base; support corporate CA secrets
  - Registry detection and double-tagging with repository.migros.net prefix when reachable

13) Toolchain-specific details

13.1 Rust
- Preferred image: aifo-coder-toolchain-rust:<version|latest>
- Official fallback: rust:<version>-bookworm with bootstrap wrapper (install nextest and rustup components) on first exec
- Caches: host-preferred mounts for $HOME/.cargo/{registry,git}; fallback to aifo-cargo-registry/git
- Env: HOME=/home/coder; GNUPGHOME=/home/coder/.gnupg; CARGO_HOME=/home/coder/.cargo; RUST_BACKTRACE=1 if unset; CC=gcc; CXX=g++
- PATH not overridden via -e; the image sets it correctly (includes system paths)

13.2 Node + TypeScript
- Image: node:<ver>-bookworm-slim
- Caches: aifo-npm-cache:/home/coder/.npm
- Tools: node, npm, npx, tsc, ts-node
- TSC resolution: prefer local ./node_modules/.bin/tsc; fallback to npx tsc; optional global typescript via bootstrap.

13.3 Python
- Image: python:<ver>-slim
- Caches: aifo-pip-cache:/home/coder/.cache/pip
- Tools: python, python3, pip, pip3
- Virtualenv: if /workspace/.venv exists, set VIRTUAL_ENV and PATH for exec.

13.4 C/C++
- Image: aifo-coder-toolchain-cpp:latest (debian:bookworm-slim) with build-essential, clang, cmake, ninja, pkg-config, ccache
- Caches: aifo-ccache:/home/coder/.cache/ccache; export CCACHE_DIR
- Ensure cc and c++ are present (hardlinks to gcc/g++)

13.5 Go
- Image: golang:<ver>-bookworm
- Caches: aifo-go:/go
- Env: GOPATH=/go, GOMODCACHE=/go/pkg/mod, GOCACHE=/go/build-cache
- Tools: go, gofmt

14) Cleanup and lifecycle

- Cleanup on agent exit or SIGINT/SIGTERM:
  - Stop sidecars (if they exist)
  - Remove session network
  - Stop proxy and remove unix socket dir (Linux unix transport)
- Named volumes (caches) persist by default; Makefile target toolchain-cache-clear purges them.

15) Backward compatibility

- Without --toolchain, behavior remains unchanged.
- Shims print guidance and exit 86 when proxy env is not set.
- Protocol v1 is supported for legacy shims; v2 is recommended and default in embedded shim.
- Dynamic dev-tool routing is additive: if only c-cpp is running, tools route to c-cpp; if only rust is running and tool present, route to rust.

16) Security considerations

- No Docker socket inside containers; only host proxy talks to Docker CLI.
- Apply AppArmor profile for sidecars when available.
- Token-authenticated proxy; reject unauthorized requests.
- Network isolation via per-session network; random container names.
- Sidecar allowlists constrain permissible commands; dynamic routing respects allowlists after selection.

17) Performance

- Streaming output (v2) provides immediate user feedback; no large buffers.
- Named volumes for caches accelerate rebuilds (cargo, npm, pip, ccache, go).
- Tool availability checks cached per session; minimal docker inspect/exec probes.
- Avoid heavyweight parsing in proxy; use compact framing and simple form-encoding.

18) Testing plan

- Unit tests:
  - Routing (preferred_kinds_for_tool, dynamic sidecar selection)
  - Header parsing and token validation (Authorization, protocol negotiation)
- Integration tests:
  - Rust sidecar: cargo --version via proxy; v2 streaming and trailer parsing
  - Dev-tool routing: with rust only, /run make works; with c-cpp only, /run make works; with both, prefer c-cpp
  - Node/typescript: npx --version; tsc resolution behavior
  - Python: pip --version; venv activation check
  - C/C++: cmake --version; cc/c++ presence
  - Go: go version; simple go build
- End-to-end:
  - Agent + rust sidecar; cargo build of a sample crate; confirm artifacts and no truncation in output
- Platform coverage:
  - Linux CI runner with Docker; include unix socket test lane
  - macOS GitHub runner (Docker Desktop/Colima)
  - Windows GitHub runner (Docker Desktop/WSL2)
- Negative tests:
  - Missing/invalid token -> 401 Unauthorized
  - Unsupported/missing protocol (with auth) -> 426 Upgrade Required with message
  - Unknown tool name -> 403 Forbidden (allowlist enforcement)

19) Rollout phases

- Phase 1: Sidecars + explicit toolchain subcommand (bootstrap)
- Phase 2: Transparent PATH shims with proxy; host-gateway logic for Linux TCP
- Phase 3: Embed shim into agent images; add cc/c++ shims; dynamic dev-tool routing
- Phase 4: Streaming protocol v2 (chunked + trailers) with Linux unix socket support; polished verbose logging
- Phase 5: Complete docs and tests; Makefile integrations for rust toolchain base and CA secrets

20) Documentation updates

- README.md: --toolchain usage, examples, dev-tool routing description.
- INSTALL.md: Docker Desktop notes; Linux host-gateway note; unix socket setup.
- man/aifo-coder.1: updated CLI flags and descriptions.
- docs/TOOLEEXEC_PROTOCOL.md: protocol v1/v2 details, curl flags, unix mode, client behavior.
- examples/: samples and minimal projects for each toolchain.

21) Open questions

- Persist tool availability cache across sessions?
- Podman support when docker is missing?
- Structured logging and log level control for proxy/shim?

22) Future work

- Optional podman support where available.
- Toolchain discovery via config files (e.g., .tool-versions style).
- Remote execution option (SSH or cloud runner) sharing the same streaming protocol.
- Optional telemetry for improving defaults (opt-in).

23) Implementation checklist (repo oriented) — consolidated v4

- CLI:
  - Repeatable --toolchain and --toolchain-image; --no-toolchain-cache.
- Lib (src/toolchain.rs):
  - Session/network helpers, AppArmor integration
  - Rust mounts/env; ownership init; linker flags; proxy env passthrough
  - Dynamic dev-tool routing with availability probe and per-session cache
  - Protocol v2 streaming (chunked + trailers) and v1 buffered; TCP and Linux unix sockets (/run/aifo)
  - Line-safe verbose logs; command duration metrics
- Shim:
  - Agent-embedded shim at /opt/aifo/bin with cc/c++ symlinks; v2 streaming using curl; unix socket support (--unix-socket)
- Docker images:
  - Rust toolchain (aifo-coder-toolchain-rust) per v7; CA injection via BuildKit secret
  - C/C++ toolchain (aifo-coder-toolchain-cpp) with cc/c++ hardlinks
  - Agent images: embed POSIX shim client; slim/fat variants with PATH including /opt/aifo/bin
- Makefile:
  - Build/publish toolchains; pass RUST_TAG; corporate CA secret; registry detection and double-tag
- Docs:
  - Protocol spec; toolchain usage; unix sockets; dev-tool routing; security notes

Appendix A: Key environment variables
- Toolchain images:
  - AIFO_RUST_TOOLCHAIN_IMAGE, AIFO_RUST_TOOLCHAIN_VERSION, AIFO_RUST_TOOLCHAIN_USE_OFFICIAL
- Caches:
  - AIFO_TOOLCHAIN_NO_CACHE, AIFO_TOOLCHAIN_RUST_USE_DOCKER_VOLUMES, AIFO_TOOLCHAIN_RUST_USE_HOST_CONFIG
- SSH:
  - AIFO_TOOLCHAIN_SSH_FORWARD, SSH_AUTH_SOCK
- sccache:
  - AIFO_RUST_SCCACHE, AIFO_RUST_SCCACHE_DIR
- Linkers:
  - AIFO_RUST_LINKER=lld|mold
- Proxies/cargo networking:
  - HTTP_PROXY, HTTPS_PROXY, NO_PROXY; http_proxy, https_proxy, no_proxy
  - CARGO_NET_GIT_FETCH_WITH_CLI, CARGO_REGISTRIES_CRATES_IO_PROTOCOL
- Proxy:
  - AIFO_TOOLEEXEC_URL, AIFO_TOOLEEXEC_TOKEN, AIFO_TOOLEEXEC_TIMEOUT_SECS
  - AIFO_TOOLEEXEC_USE_UNIX=1 (Linux), AIFO_TOOLEEXEC_ADD_HOST=1 (Linux TCP)
- Diagnostics:
  - AIFO_TOOLCHAIN_VERBOSE, RUST_BACKTRACE (default 1 for rust)

Appendix B: Tool allowlists (final)
- rust: cargo, rustc, make, cmake, ninja, pkg-config, gcc, g++, clang, clang++, cc, c++
- node: node, npm, npx, tsc, ts-node, and dev tools above if present
- python: python, python3, pip, pip3, and dev tools above if present
- c-cpp: gcc, g++, cc, c++, clang, clang++, make, cmake, ninja, pkg-config
- go: go, gofmt, and dev tools above if present

Appendix C: Naming and paths
- Sidecars: aifo-tc-<kind>-<sid>
- Network: aifo-net-<sid>
- Linux unix socket: /run/aifo/aifo-<sid>/toolexec.sock

Rationale

This v4 design consolidates earlier proposals and completes the streaming-based tool execution protocol with robust dynamic tool routing and improved developer ergonomics. It aligns the Rust toolchain with v7 requirements, embeds a cross-platform shim including cc/c++, and ensures consistent, secure behavior across platforms. The approach leverages Docker isolation, named volumes for performance, and a lightweight proxy protocol that cleanly integrates into the existing aifo-coder codebase and security posture, offering a fast, reproducible, and user-friendly toolchain experience.
