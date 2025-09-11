Title: Toolchain Sidecars, Streaming Tool Exec, Dynamic Dev-Tool Routing, and Transparent Shims for aifo-coder (v5)

Status: Accepted (implementation-complete)
Authors: AIFO
Created: 2025-09-10
Target platforms: Linux, macOS, Windows (Docker Desktop / WSL2 / Colima)
Scope: Runtime toolchain attachment for coding agents (Rust, Node/TypeScript, Python, C/C++, Go) with streaming proxy protocol v2 and dynamic routing

1) Summary

aifo-coder dynamically extends a running agent container with language/toolchain capabilities on demand via repeatable --toolchain flags. Tooling runs inside dedicated sidecar containers that share the project workspace and language-specific caches with the agent. Inside the agent, small shim programs (cargo, node, npm, npx, tsc, ts-node, python, pip, gcc, cc, g++, c++, clang, clang++, make, cmake, ninja, pkg-config, go, gofmt, notifications-cmd, etc.) live on PATH and transparently forward tool invocations to a host-side proxy, which executes the command inside the appropriate sidecar via docker exec.

This v5 specification consolidates v4 and incorporates complete, precise details for:
- Streaming tool execution (protocol v2) with HTTP/1.1 chunked transfer and exit code trailers.
- Dynamic routing of common development tools (make, cmake, ninja, pkg-config, gcc, g++, clang, clang++, cc, c++) to the first running sidecar that provides them (prefer c-cpp, then rust, go, node, python).
- Linux unix-socket transport for the proxy, including mounting semantics.
- Rust toolchain v7 alignment (PATH, caches, CC/CXX), allowlists, security, and error semantics.

2) Goals

- Dynamically attach toolchains via --toolchain flags (repeatable).
- Keep agent images lean; heavy toolchains live in sidecars.
- No Docker socket in containers; proxy runs on the host with token auth.
- Seamless agent tool usage via PATH shims; streaming output by default (v2).
- Work across Linux, macOS (Docker Desktop/Colima), Windows (Docker Desktop/WSL2).
- Robust caching (named volumes and host mounts where safe).
- Image overrides and version pinning per toolchain; Rust aligns with v7.
- Dynamic dev-tool routing to eliminate unnecessary sidecars.

3) Non-goals

- Remote multi-node orchestration beyond a single-host Docker environment.
- Replacing native host tools for non-container workflows.

4) Terminology

- Agent: The coding agent container (e.g., aider, crush, codex).
- Sidecar: Long-lived toolchain container (rust, node, python, c-cpp, go).
- Shim: Tiny client program on PATH that forwards tool invocations to the host proxy.
- Proxy: Host-side server spawned by aifo-coder; executes docker exec into sidecars.

5) User Experience and CLI

5.1 Flags
- --toolchain <kind[@version]> (repeatable)
  - kind ∈ {rust, node, typescript, python, c, cpp, c-cpp, go}
  - version optional, e.g., rust@1.80, node@20, python@3.12, go@1.22
- --toolchain-image <kind=image> (repeatable)
  - Override default image, e.g., rust=aifo-rust-toolchain:1.80 or python=python:3.12-slim
- --no-toolchain-cache
  - Disable named cache volumes; workspace still shared.
- Optional: --toolchain-bootstrap <kind=mode>
  - typescript=global to preinstall TypeScript globally in the node sidecar (off by default).
- Optional (Linux): --toolchain-unix-socket
  - Use unix:/// transport instead of TCP for the proxy.

5.2 Examples
- aifo-coder aider --toolchain rust -- cargo build --release
- aifo-coder crush --toolchain node --toolchain typescript -- npx vitest
- aifo-coder aider --toolchain python -- python -m pytest
- aifo-coder aider --toolchain c-cpp -- make -j
- aifo-coder aider --toolchain go -- go test ./...
- Dev-tool routing: aifo-coder aider --toolchain rust -- make  (routes to rust sidecar if c-cpp isn’t running)

6) Architecture

6.1 High-level flow (when toolchains requested)
1. Generate session ID (short random) and create user-defined Docker network: aifo-net-<sid>.
2. Start sidecar containers:
   - docker run -d --rm --name aifo-tc-<kind>-<sid> --network aifo-net-<sid> [mounts] [env] [--user uid:gid] [apparmor] <image> sleep infinity
3. Start host-side toolexec proxy:
   - Address (AIFO_TOOLEEXEC_URL):
     - macOS/Windows: tcp://host.docker.internal:<port>
     - Linux (TCP): tcp://0.0.0.0:<port>
     - Linux (unix): unix:///run/aifo/aifo-<sid>/toolexec.sock (when enabled)
   - Generate bearer token AIFO_TOOLEEXEC_TOKEN.
4. Prepare shim directory inside the agent:
   - Agent images embed aifo-shim at /opt/aifo/bin with symlinks for all tools (including cc/c++).
   - Agent PATH includes /opt/aifo/bin.
5. Launch agent container:
   - Join the session network; bind mount workspace.
   - Inject AIFO_TOOLEEXEC_URL and AIFO_TOOLEEXEC_TOKEN.
   - Linux TCP: add --add-host=host.docker.internal:host-gateway to the agent; sidecars do not need it.
   - Linux unix: bind-mount proxy socket directory into agent at /run/aifo; set URL to unix:///run/aifo/aifo-<sid>/toolexec.sock.
   - Apply AppArmor profile where available.
6. During run, when the agent invokes a tool:
   - The shim connects to the proxy, sends argv/env/cwd + tool name.
   - The proxy maps tools to a sidecar and runs docker exec with correct user, cwd=/workspace, env, stdio.
   - Protocol v2 streams stdout/stderr live; exit code via HTTP trailer.

6.2 Tool mapping and dynamic fallback
- Family mapping:
  - rust: cargo, rustc
  - node/typescript: node, npm, npx, tsc, ts-node
  - python: python, python3, pip, pip3
  - c-cpp: gcc, g++, clang, clang++, make, cmake, ninja, pkg-config, cc, c++
  - go: go, gofmt
- Dev-tool routing (make, cmake, ninja, pkg-config, gcc, g++, clang, clang++, cc, c++):
  - Preferred order: c-cpp, rust, go, node, python
  - Select first running sidecar where command -v <tool> succeeds (per-session cache).
  - If none running provides it, return a clear error suggesting toolchains to start.

6.3 Tool resolution notes
- TypeScript: Prefer ./node_modules/.bin/tsc; else npx tsc; else global tsc when bootstrapped.
- Python: Respect /workspace/.venv (proxy sets VIRTUAL_ENV and PATH accordingly when executing).
- Rust (v7 alignment):
  - CARGO_HOME=/home/coder/.cargo; image PATH includes $CARGO_HOME/bin and /usr/local/cargo/bin plus system dirs.
  - Do not override PATH via -e at runtime; export CC=gcc and CXX=g++ for build scripts.
- C/C++: Respect CC/CXX from env when set; otherwise default compilers are available inside c-cpp and rust sidecars.

7) Sidecar images and caching

7.1 Defaults (overridable via --toolchain-image)
- rust: aifo-rust-toolchain:<version|latest> (preferred); optional official rust:<version>-bookworm if AIFO_RUST_TOOLCHAIN_USE_OFFICIAL=1 (bootstrap on exec).
- node/typescript: node:<ver>-bookworm-slim (default node:20-bookworm-slim).
- python: python:<ver>-slim (default python:3.12-slim).
- c-cpp: aifo-cpp-toolchain:latest (debian:bookworm-slim; build-essential clang cmake ninja pkg-config ccache; cc/c++ hardlinks).
- go: golang:<ver>-bookworm (default golang:1.22-bookworm).

7.2 Mounts and caches
- Workspace: -v "$PWD:/workspace"
- Rust:
  - Host-preferred mounts when present:
    - $HOME/.cargo/registry -> /home/coder/.cargo/registry
    - $HOME/.cargo/git -> /home/coder/.cargo/git
  - Fallback to named volumes:
    - aifo-cargo-registry:/home/coder/.cargo/registry
    - aifo-cargo-git:/home/coder/.cargo/git
  - Back-compat legacy mounts retained: /usr/local/cargo/{registry,git}
  - Ownership init for named volumes (one-shot chown uid:gid) via helper container; stamp .aifo-init-done
- Node/TypeScript: aifo-npm-cache:/home/coder/.npm
- Python: aifo-pip-cache:/home/coder/.cache/pip
- C/C++: aifo-ccache:/home/coder/.cache/ccache (CCACHE_DIR)
- Go: aifo-go:/go (GOPATH=/go, GOMODCACHE, GOCACHE)

7.3 UID/GID consistency
- Unix hosts: run sidecars as --user uid:gid to avoid permission issues on shared mounts.
- Windows: omit --user (Docker Desktop handles ownership differently).

8) Transport and connectivity

8.1 Variables
- AIFO_TOOLEEXEC_URL:
  - TCP:
    - macOS/Windows: http://host.docker.internal:<port>/exec
    - Linux: http://host.docker.internal:<port>/exec (agent and sidecars use host-gateway)
  - Unix (Linux): unix:///run/aifo/aifo-<sid>/toolexec.sock
- AIFO_TOOLEEXEC_TOKEN: Bearer token for proxy; required for access.
- AIFO_TOOLEEXEC_TIMEOUT_SECS: Per-request timeout (default 60).

8.2 Host addressability
- macOS/Windows: host.docker.internal resolves to host automatically.
- Linux (TCP): add --add-host=host.docker.internal:host-gateway to the agent when toolchains are enabled.

8.3 Binding strategy
- macOS/Windows: proxy binds 127.0.0.1:<random high port>.
- Linux (TCP): proxy binds 0.0.0.0:<random high port> (token-authenticated, short-lived).
- Linux (unix): proxy binds at /run/aifo/aifo-<sid>/toolexec.sock; agent shims use curl --unix-socket path and http://localhost/exec.

8.4 Unix socket mount (Linux)
- The proxy sets AIFO_TOOLEEXEC_UNIX_DIR to /run/aifo/aifo-<sid>.
- The agent container MUST bind-mount this host directory at /run/aifo.
- The agent MUST set AIFO_TOOLEEXEC_URL=unix:///run/aifo/aifo-<sid>/toolexec.sock.
- Sidecars DO NOT need the socket mounted (shim runs inside agent only).

9) Security

- Do not mount Docker socket into agents or sidecars.
- AppArmor profile: apply the same profile to sidecars as the agent when available.
- Per-session network and randomized names reduce collision risk.
- Token-authenticated proxy; reject unauthorized requests (401).
- Sidecar-specific allowlists constrain permissible commands; dynamic routing respects allowlists after selection.
- Notifications-cmd endpoint bypasses Authorization but enforces an exact-match guard:
  - Only executes the host say command with arguments exactly matching ~/.aider.conf.yml notifications-command. All mismatches rejected (403).

10) Shim design

10.1 Behavior
- Agent images embed aifo-shim at /opt/aifo/bin with symlinks for:
  - cargo, rustc, node, npm, npx, tsc, ts-node, python, pip, pip3, gcc, g++, cc, c++, clang, clang++, make, cmake, ninja, pkg-config, go, gofmt, notifications-cmd
- Determines invoked tool via argv[0].
- Reads AIFO_TOOLEEXEC_URL and AIFO_TOOLEEXEC_TOKEN; if missing, prints guidance and exits 86.
- Protocol v2: streams output via curl --no-buffer; writes headers+trailers to a temp file; parses X-Exit-Code and exits with that code.
- Supports unix:// URLs on Linux via curl --unix-socket path; uses http://localhost/exec URL rewrite.

10.2 Protocols
- v2 (recommended): streaming via HTTP/1.1 chunked; exit code via trailer X-Exit-Code; stderr merged into stdout by wrapping exec with sh -lc '<cmd> 2>&1' (implementation detail).
- v1 (legacy): buffered response with Content-Length and X-Exit-Code header; in verbose mode prefix/suffix a newline to avoid UI wrap artifacts.

10.3 Embedding vs. bind-mount
- Agent images embed the shim for out-of-box operation; no extra mount required.
- toolchain_write_shims can write a host shim set for development and mount via AIFO_SHIM_DIR.

11) Proxy design

11.1 Responsibilities
- Listen on AIFO_TOOLEEXEC_URL, validate token (Authorization or Proxy-Authorization headers).
- Parse request: tool name, argv, cwd (form-encoded or query string); require X-Aifo-Proto: 1 or 2 once Authorization is valid; otherwise 426 Upgrade Required.
- Dynamic dev-tool routing:
  - For make, cmake, ninja, pkg-config, gcc, g++, clang, clang++, cc, c++:
    - Preferred kinds: c-cpp, rust, go, node, python
    - Select first running sidecar where command -v <tool> succeeds (cached for session)
- Execute docker exec with correct user, cwd=/workspace, env:
  - TypeScript: resolve ./node_modules/.bin/tsc inside sidecar; else npx tsc.
  - Rust: export CARGO_HOME=/home/coder/.cargo; set CC=gcc and CXX=g++.
  - Python venv: set VIRTUAL_ENV and PATH override when /workspace/.venv/bin exists (for python sidecar only).

11.2 Streaming (v2) and buffered (v1)
- v2 streaming:
  - Send early headers:
    - HTTP/1.1 200 OK
    - Content-Type: text/plain; charset=utf-8
    - Transfer-Encoding: chunked
    - Trailer: X-Exit-Code
    - Connection: close
  - Spawn docker exec wrapped in sh -lc '<cmd> 2>&1' (stderr merged).
  - Read child stdout and write chunks: "<hexlen>\r\n<data>\r\n"; flush after each.
  - On process exit, write "0\r\nX-Exit-Code: <code>\r\n\r\n"
  - No write timeout on the socket (to avoid truncation).
- v1 buffered:
  - Spawn docker exec and collect stdout+stderr.
  - Send 200 OK with Content-Length and X-Exit-Code header; then body.
  - In verbose mode, prefix and suffix a newline to avoid UI artifacts.

11.3 Reliability and performance
- Concurrency: handle sequential requests; cache tool availability per sidecar to minimize probes.
- Timeouts: configurable command timeout; default 60s with 504 on timeout.
- Logging: concise, line-safe logs on stderr (flush stdout/stderr; clear current line) including durations.

11.4 Error semantics (HTTP status + exit code)
- 200 OK: success; X-Exit-Code trailer (v2) or header (v1).
- 401 Unauthorized: token missing/invalid.
- 403 Forbidden: tool not in sidecar allowlist.
- 409 Conflict: requested dev tool not available in any running sidecar; body suggests toolchains to start.
- 426 Upgrade Required: Authorization valid but protocol unsupported/missing (require 1 or 2).
- 504 Gateway Timeout: tool execution timed out.

12) Integration with existing code

- src/toolchain.rs:
  - Session/network helpers; sidecar start/stop; run/exec previews
  - Ownership init for rust caches; proxy env passthrough; dynamic dev-tool routing with per-session availability cache
  - Proxy implementation (TCP + Linux unix sockets), protocol v1/v2 handling
  - Shim writer (toolchain_write_shims)
  - AppArmor profile usage
- Dockerfile:
  - Embed POSIX shell shim client (curl-based) implementing protocol v2; add symlinks for all tools including cc/c++
  - PATH includes /opt/aifo/bin in agent stages
- toolchains/rust/Dockerfile (v7 alignment):
  - CARGO_HOME=/home/coder/.cargo; PATH="$CARGO_HOME/bin:/usr/local/cargo/bin:${PATH}"
  - Preinstall clippy, rustfmt, rust-src, llvm-tools-preview; cargo-nextest
  - Install system deps used by common crates
  - Optional BuildKit secret to add corporate CA to trust TLS interceptors
- toolchains/cpp/Dockerfile:
  - build-essential, clang, cmake, ninja, pkg-config, ccache, git, ca-certificates
  - Hardlink cc -> gcc and c++ -> g++
- Makefile:
  - Build/publish rules for toolchains; pass RUST_TAG for rust base; corporate CA secrets; registry detection and double-tagging
  - Recommended parity for c-cpp (registry detection, double-tag, optional buildx multi-arch with PLATFORMS and PUSH)

13) Toolchain-specific details

13.1 Rust
- Preferred image: aifo-rust-toolchain:<version|latest>
- Official fallback: rust:<version>-bookworm with bootstrap wrapper on first exec
- Caches: host-preferred mounts for $HOME/.cargo/{registry,git}; fallback to aifo-cargo-registry/git
- Env: HOME=/home/coder; GNUPGHOME=/home/coder/.gnupg; CARGO_HOME=/home/coder/.cargo; RUST_BACKTRACE=1 if unset; CC=gcc; CXX=g++
- PATH MUST NOT be overridden via -e; the image sets it to include $CARGO_HOME/bin, /usr/local/cargo/bin, and system dirs

13.2 Node + TypeScript
- Image: node:<ver>-bookworm-slim
- Caches: aifo-npm-cache:/home/coder/.npm
- Tools: node, npm, npx, tsc, ts-node
- TSC resolution: prefer local ./node_modules/.bin/tsc; else npx tsc; optional global typescript via bootstrap.

13.3 Python
- Image: python:<ver>-slim
- Caches: aifo-pip-cache:/home/coder/.cache/pip
- Tools: python, python3, pip, pip3
- Virtualenv activation for exec: set VIRTUAL_ENV and PATH when .venv exists.

13.4 C/C++
- Image: aifo-cpp-toolchain:latest (debian:bookworm-slim base) with build-essential, clang, cmake, ninja, pkg-config, ccache
- Caches: aifo-ccache:/home/coder/.cache/ccache; set CCACHE_DIR
- Ensure cc and c++ are present (hardlinks to gcc/g++)

13.5 Go
- Image: golang:<ver>-bookworm
- Caches: aifo-go:/go; GOPATH=/go; GOMODCACHE=/go/pkg/mod; GOCACHE=/go/build-cache
- Tools: go, gofmt

14) Cleanup and lifecycle

- On agent exit or SIGINT/SIGTERM:
  - Stop sidecars (if they exist)
  - Remove session network
  - Stop proxy and remove unix socket dir (Linux unix transport)
- Named volumes (caches) persist; Makefile target toolchain-cache-clear purges them.

15) Backward compatibility

- Without --toolchain, behavior remains unchanged.
- Shims print guidance and exit 86 when proxy env is not set.
- Protocol v1 is supported for legacy shims; v2 is recommended and default in embedded shim.
- Dynamic dev-tool routing is additive: if only c-cpp is running, tools route to c-cpp; if only rust is running and tool present, route to rust.

16) Security considerations

- No Docker socket inside containers; only host proxy talks to Docker CLI.
- Apply AppArmor profile to sidecars when available.
- Token-authenticated proxy; reject unauthorized requests.
- Network isolation via per-session network; random container names.
- Sidecar allowlists constrain permissible commands; dynamic routing respects allowlists.
- Notifications endpoint:
  - Authorization bypass is deliberate to allow host notifications; execution strictly limited to exact-match say command; mismatches rejected.

17) Performance

- Streaming output (v2) provides immediate feedback and avoids large buffers.
- Caches speed up rebuilds (cargo, npm, pip, ccache, go).
- Tool availability checks cached per session to reduce exec probes.
- Compact framing and form-encoding; minimal overhead in proxy.

18) Testing plan

- Unit tests:
  - Protocol header parsing and token validation
  - preferred_kinds_for_tool selection
- Integration tests:
  - Rust sidecar: cargo --version via proxy; v2 streaming and trailer parsing
  - Dev-tool routing: with rust only, /run make routes to rust; with c-cpp only, routes to c-cpp; with both, prefers c-cpp
  - Node/typescript: npx --version; tsc local resolution
  - Python: pip --version; venv activation check
  - C/C++: cmake --version; cc/c++ presence
  - Go: go version; basic go build
- Unix socket (Linux):
  - Agent + unix:// transport; ensure shim uses curl --unix-socket; large output streams correctly
- End-to-end:
  - Build sample Rust crate; confirm artifacts; verify no truncation in output with v2 streaming
- Negative tests:
  - Missing token -> 401 Unauthorized
  - Unsupported/missing protocol (with auth) -> 426 Upgrade Required
  - Unknown tool name -> 403 Forbidden (allowlist)
  - Tool not available in any running sidecar -> 409 Conflict with guidance

19) Rollout phases (updated)

- Phase 1: Sidecars + explicit toolchain subcommand (bootstrap)
- Phase 2: Transparent PATH shims with proxy; Linux host-gateway logic for TCP
- Phase 3: Embed shim into agent images; cc/c++ shims; dynamic dev-tool routing
- Phase 4: Streaming protocol v2 (chunked + trailers) with Linux unix socket support; polished verbose logging; line-safe output
- Phase 5: Documentation and tests; Makefile integrations (RUST_TAG, CA secrets, registry detection)

20) Documentation updates

- README.md: --toolchain usage; dev-tool routing summary; troubleshooting for streaming/unix sockets.
- INSTALL.md: Docker Desktop notes; Linux host-gateway note; unix socket setup.
- man/aifo-coder.1: CLI flags and descriptions (including --toolchain-unix-socket).
- docs/TOOLEEXEC_PROTOCOL.md: v1/v2 details; curl flags; unix mode; client behavior; errors.
- examples/: samples for each toolchain.

21) Open questions

- Persist tool availability cache across sessions?
- Podman support where docker is absent?
- Log level configuration for proxy/shim?

22) Future work

- Optional podman support where available.
- Toolchain discovery via config (e.g., .tool-versions style).
- Remote execution option (SSH/cloud) sharing the v2 streaming protocol.
- Optional telemetry (opt-in) for improving defaults.

23) Implementation checklist (repository)

- CLI:
  - Repeatable --toolchain and --toolchain-image; --no-toolchain-cache; --toolchain-unix-socket (Linux)
- Lib (src/toolchain.rs):
  - Session/network helpers; AppArmor; ownership init; linker flags; proxy env passthrough
  - Dynamic dev-tool routing with availability probe and per-session cache
  - Protocol v2 streaming (chunked + trailers) and v1 buffered; TCP and Linux unix sockets (/run/aifo)
  - Line-safe verbose logs; command duration metrics
- Shim:
  - POSIX shell client (curl) with v2 streaming; unix socket support (--unix-socket)
  - Symlinks for cc/c++ and other tools at /opt/aifo/bin
- Docker images:
  - Rust toolchain (aifo-rust-toolchain) per v7; CA injection via BuildKit secret
  - C/C++ toolchain (aifo-cpp-toolchain) with cc/c++ hardlinks
  - Agent images: embed shim; slim/fat variants; PATH includes /opt/aifo/bin
- Makefile:
  - Build/publish toolchains; pass RUST_TAG; corporate CA secret
  - Registry detection and double-tagging (parity for c-cpp recommended)
- Docs:
  - Protocol spec; toolchain usage; unix sockets; dev-tool routing; error semantics

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
  - AIFO_TOOLEEXEC_UNIX_DIR (host socket dir for unix transport)
- Diagnostics:
  - AIFO_TOOLCHAIN_VERBOSE, RUST_BACKTRACE (default 1 for rust)

Appendix B: Tool allowlists (final)
- rust:
  - cargo, rustc, make, cmake, ninja, pkg-config, gcc, g++, clang, clang++, cc, c++
- node:
  - node, npm, npx, tsc, ts-node, and dev tools above if present
- python:
  - python, python3, pip, pip3, and dev tools above if present
- c-cpp:
  - gcc, g++, cc, c++, clang, clang++, make, cmake, ninja, pkg-config
- go:
  - go, gofmt, and dev tools above if present

Appendix C: Naming and paths
- Sidecars: aifo-tc-<kind>-<sid>
- Network: aifo-net-<sid>
- Linux unix socket: /run/aifo/aifo-<sid>/toolexec.sock

Rationale

This v5 design consolidates v4 and clarifies all operational details to provide an excellent toolchain experience. It finalizes the streaming-based tool execution protocol with dynamic dev-tool routing, robust unix socket handling on Linux, and precise error semantics. The Rust toolchain aligns with v7 requirements, agent images embed a cross-platform shim including cc/c++, and the solution ensures consistent, secure behavior across platforms. The approach leverages Docker isolation, named volumes for performance, and a lightweight proxy protocol that plugs cleanly into the codebase, delivering a fast, reproducible, and user-friendly toolchain experience.
