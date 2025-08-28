Question:

What would be the best way to extend the coding agent on the fly, dynamically, with a toolchain? I would like to be able to specify --toolchain rust and then a Rust toolchain should be available within the coding agent container, so that the agent is able to build code.


Short answer Launch a per-run “toolchain sidecar” container for each requested language, put thin shims on the agent’s PATH, and proxy tool invocations
(cargo, tsc, gcc, go, etc.) into the corresponding sidecar with docker exec. This keeps agent images slim, isolates toolchains cleanly, and works across
Linux, macOS, and Windows (Docker Desktop/WSL2).

Architecture overview

 • Sidecars (one per toolchain): rust, node (covers TypeScript), python, c-cpp, go.
 • Shared mounts: bind-mount the workspace into all containers; attach language-specific caches via named volumes.
 • PATH shims inside the agent: binaries named cargo, tsc, gcc, go, python, etc., that forward requests to a host-side “toolexec” proxy.
 • Toolexec proxy: a small server spawned by aifo-coder on the host. It receives a request from a shim and runs docker exec into the right sidecar, wiring
   stdio, exit code, cwd, and env.

Toolchains covered

• Rust: sidecar image rust:-slim. Map cargo, rustc.
• Caches: -v aifo-cargo-registry:/usr/local/cargo/registry, -v aifo-cargo-git:/usr/local/cargo/git
• Node + TypeScript: sidecar image node:-bookworm-slim. Map node, npm, npx, tsc, ts-node.
• For TypeScript, prefer project-local compiler (node_modules/.bin/tsc) or npx tsc. Optionally allow a one-time global npm i -g typescript in the
• For TypeScript, prefer project-local compiler (node_modules/.bin/tsc) or npx tsc. Optionally allow a one-time global npm i -g typescript in the
sidecar.
• Caches: -v aifo-npm-cache:/home/coder/.npm; keep node_modules in the workspace for agent visibility.
• Python: sidecar image python:-slim. Map python, python3, pip, pip3.
• Python: sidecar image python:-slim. Map python, python3, pip, pip3.
• Caches: -v aifo-pip-cache:/home/coder/.cache/pip
• Recommend project .venv under /workspace; shim can ensure it runs inside that venv when present.
• C/C++: sidecar image aifo-cpp-toolchain: (our minimal Debian/Ubuntu with build-essential, clang, cmake, ninja, pkg-config, ccache).
• Map gcc, g++, clang, clang++, make, cmake, ninja, pkg-config.
• Caches: optional -v aifo-ccache:/home/coder/.cache/ccache and set CCACHE_DIR, CC/CXX to ccache-wrapped compilers.
• Go: sidecar image golang:-bookworm or -alpine (bookworm for glibc). Map go, gofmt.
• Caches: -v aifo-go:/go and set GOPATH=/go, GOMODCACHE=/go/pkg/mod, GOCACHE=/go/build-cache.

Cross-OS connectivity

• Shim-to-proxy transport:
• Use a single URL env var AIFO_TOOLEEXEC_URL, e.g. tcp://host.docker.internal:48xxx?token=… (cross-platform), or unix:///run/aifo/toolexec.sock on pure
Linux if desired.
• Reaching the host from containers:
• macOS and Windows: host.docker.internal works out of the box.
• Linux: add --add-host=host.docker.internal:host-gateway when toolchains are enabled (supported in modern Docker). This makes the proxy reachable at
that name.
• Proxy bind address:
• macOS/Windows: binding to 127.0.0.1 is fine; host.docker.internal forwards to it.
• Linux: bind to 0.0.0.0 on a random high port but protect with a random per-session bearer token; only containers on the local bridge can reach it, and
token gates usage. Optionally allow unix socket mode on Linux for tighter scoping.

Security

• No Docker socket in the agent or toolchain containers.
• Sidecars get the same AppArmor profile as the agent when available.
• Random session ID, random auth token for the proxy; short-lived network and sidecars; cleanup on exit.

CLI UX

• Repeatable flag: --toolchain <kind[@ver]>
• kinds: rust, node, typescript, python, c, cpp, c-cpp, go
• versions optional: rust@1.80, node@20, python@3.12, go@1.22
• Image override: --toolchain-image rust=rust:1.80-slim, cpp=aifo-cpp-toolchain:latest, etc.
• Optional: --no-toolchain-cache to skip named volumes; --toolchain-bootstrap typescript=global to preinstall TypeScript globally in the node sidecar.

Runtime sequence (when toolchains requested)

1 Create session ID and user-defined docker network aifo-net-.
2 Start requested sidecars:
• docker run -d --rm --name aifo-tc-rust- --network aifo-net- -v "$PWD:/workspace" [cache volumes] [--user uid:gid] [apparmor]  sleep infinity
• Repeat per toolchain with appropriate mounts and env defaults.
3 Start aifo-coder toolexec proxy on the host:
• Choose a random port P and token T; export AIFO_TOOLEEXEC_URL to agent.
• On Linux add --add-host=host.docker.internal:host-gateway to the agent run.
4 Launch the agent container with:
• -v workspace mount, timezones, git/gnupg mounts as today
• -v shim dir at /opt/aifo/bin and prepend PATH=/opt/aifo/bin:$PATH
• AIFO_TOOLEEXEC_URL=tcp://host.docker.internal:P and AIFO_TOOLEEXEC_TOKEN=T
5 Shims inside the agent forward commands to the proxy, which runs docker exec into the correct sidecar, setting:
• -w /workspace, -u uid:gid (on Unix hosts), and a curated env (EDITOR, TERM, AIFO_* API, etc.)
• For TypeScript: resolve to node sidecar; invoke node_modules/.bin/tsc when present, else npx tsc.

Embedding the shim in agent images

• Yes, we can bake the shim into each agent image at /opt/aifo/bin and create symlinks for the common tools (cargo, rustc, node, npm, npx, tsc, python, pip,
                                                                                                             gcc, g++, clang, cmake, ninja, pkg-config, go, gofmt).
• Pros:
• No extra bind mount for the shim.
• Always-on experience out of the box.
• Cons:
• Requires rebuilding agent images when the shim changes.
• Mitigation:
• Keep a versioned shim protocol; agent shim reads AIFO_TOOLEEXEC_URL/TOKEN set by the launcher. If absent, shim can no-op or emit a helpful message.
• Allow host override by optionally bind-mounting a newer shim over /opt/aifo/bin when requested.

Per-toolchain defaults and caches

• Rust:
• Image default: rust:1.80-slim (configurable)
• Env defaults: CARGO_HOME=/usr/local/cargo
• Node/TypeScript:
• Image default: node:20-bookworm-slim
• Env defaults: NPM_CONFIG_CACHE=/home/coder/.npm
• tsc resolution order: ./node_modules/.bin/tsc, then npx tsc, then optional global install in sidecar if enabled
• Python:
• Image default: python:3.12-slim
• Env defaults: PIP_CACHE_DIR=/home/coder/.cache/pip
• Auto-activate /workspace/.venv if present
• C/C++:
• Image default: aifo-cpp-toolchain:latest we define (FROM debian:bookworm-slim; apt-get install -y build-essential clang cmake ninja-build pkg-config
                                                      ccache)
• Env defaults: CCACHE_DIR=/home/coder/.cache/ccache, CC="ccache gcc", CXX="ccache g++" when ccache volume is attached
• Go:
• Image default: golang:1.22-bookworm
• Env defaults: GOPATH=/go, GOMODCACHE=/go/pkg/mod, GOCACHE=/go/build-cache

Windows, macOS, Linux specifics

• macOS (Docker Desktop) and Windows (Docker Desktop/WSL2):
• host.docker.internal available; proxy binds 127.0.0.1:P, containers connect to host.docker.internal:P.
• Linux:
• Add --add-host=host.docker.internal:host-gateway for agent and sidecars.
• Proxy binds 0.0.0.0:P (localhost-only isn’t reachable from container on Linux). Guard with token; use high random port; lifetime limited to the
session.
• Optionally support unix:///run/aifo/toolexec.sock by mounting a socket path on Linux systems that support it (advanced mode).
• UID/GID:
• On Unix hosts, run sidecars with --user uid:gid to avoid permission problems; on Windows host leave default.

Minimal changes needed (brief)

• CLI (src/main.rs):
• Add repeatable --toolchain and optional --toolchain-image.
• Orchestration (new helpers in src/lib.rs or a new module):
• plan_toolchains(), create_session_network(), start_sidecar(lang, image, mounts, env, user, apparmor), stop_sidecars(), run_toolexec_proxy()
• Resolve image defaults per toolchain and attach the right volumes.
• Docker run for agent (keep build_docker_cmd mostly unchanged):
• When toolchains requested, add:
• PATH=/opt/aifo/bin:$PATH and AIFO_TOOLEEXEC_URL/TOKEN env vars
• --add-host=host.docker.internal:host-gateway on Linux
• a volume for the shim dir unless shim is baked into the image
• Shim (new tiny binary):
• Reads its own argv[0] to decide which tool was called; connects to AIFO_TOOLEEXEC_URL; forwards argv/env/cwd; streams stdio; exits with remote code.
• Single binary installed once and symlinked for tool names; also baked into agent images for zero-mount UX.

Example usage

• aifo-coder aider --toolchain rust -- cargo build --release
• aifo-coder crush --toolchain node --toolchain typescript -- npx vitest
• aifo-coder aider --toolchain c-cpp -- cmake -S . -B build && cmake --build build -j
• aifo-coder aider --toolchain go -- go test ./...

Rollout plan

• Phase 1: Implement sidecars + explicit toolchain subcommand for validation
• aifo-coder toolchain rust -- cargo build
• Phase 2: Add shim + proxy for transparent PATH interception
• Phase 3: Bake shim into agent images and add aifo-cpp-toolchain image target to the Dockerfile/Makefile

If you want, I can start with Phase 1 for rust/node/python/go and add the c-cpp toolchain image, then wire up the shim and cross-OS proxy in Phase 2. Shall I proceed with these next steps?$
