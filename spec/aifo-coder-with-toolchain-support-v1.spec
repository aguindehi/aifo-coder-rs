What would be the best way to extend the coding agent on the fly, dynamically, with a toolchain. I would like to be able to specify --toolchain rust a
> nd then a Rust toolchain should be available within the coding agent container, so that the agent is able to build code.

Best overall approach: run a “toolchain sidecar” and transparently forward toolchain commands from the agent to the sidecar

Why this is best

 • Robust: the sidecar image (rust/node/python) already includes the right compilers, linkers, system libs, and updates independently of the agent.
 • Zero bloat to the agent: you don’t need to bake toolchains into every agent image.
 • Secure: you don’t have to mount the host Docker socket into the agent.
 • Reusable: works the same for Rust, Node, Python, etc., and across architectures.

How it works at runtime

 • aifo-coder launches:
    • the agent container (as today), and
    • one or more toolchain “sidecar” containers (e.g., rust:1.80-slim), all on a short-lived user-defined network.
 • All containers share:
 • All containers share:
    • the project workspace bind mount (e.g., $PWD:/workspace), so build artifacts are visible to the agent,
    • language caches via named volumes (for speed).
 • aifo-coder mounts a tiny “shim bin” directory into the agent at the front of PATH. That directory contains wrapper binaries named cargo, rustc, npm, node,
   python, pip, etc.
 • The shims do not run Docker; instead, they send an exec request over a Unix domain socket (mounted into the agent at /run/aifo/toolexec.sock).
 • The aifo-coder launcher listens on that socket and performs docker exec in the matching sidecar (e.g., aifo-tc-rust-) with the right cwd, env, and mounts.
   Exit codes and stdio are proxied back, so from the agent’s point of view, “cargo build” just works.

CLI UX proposal

 • --toolchain rust (repeatable; accepts rust, node, python)
 • Optional versions: --toolchain rust@1.80, node@20, python@3.12
 • Optional sidecar image override: --toolchain-image rust=rust:1.80-slim
 • Optional: --no-toolchain-cache if you don’t want named volumes for caches

What aifo-coder would do when --toolchain rust is used (sketch)




 • Create a session id and network aifo-net-.
 • Start sidecar container:
    • docker run -d --rm --name aifo-tc-rust- --network aifo-net-
    • -v "$PWD:/workspace"
    • -v "$PWD:/workspace"
    • -v aifo-cargo-registry:/usr/local/cargo/registry
    • -v aifo-cargo-git:/usr/local/cargo/git
    • --user "$(id -u):$(id -g)" when appropriate
    • image: rust:1.80-slim (configurable)
    • command: sleep infinity
 • Prepare the shim bin dir on the host (build once, tiny static helper binaries).
 • Start an aifo-coder “toolexec” listener bound to $XDG_RUNTIME_DIR/aifo-coder//toolexec.sock and mount that dir into the agent at /run/aifo.
 • Launch the agent container with:
    • the same workspace mount,
    • the same workspace mount,
    • -v $RUNTIME_DIR:/run/aifo
    • -v $SHIM_BIN:/opt/aifo/bin and PATH=/opt/aifo/bin:$PATH
    • same AppArmor profile as today
 • On agent invoking “cargo build”:
    • /opt/aifo/bin/cargo connects to /run/aifo/toolexec.sock with a JSON payload like: { "sidecar": "rust", "argv": ["cargo","build","--release"], "cwd":
      "/workspace", "env": {...} }
    • The launcher runs: docker exec -w /workspace -e ... aifo-tc-rust- cargo build --release
    • Streams stdio and exit code back to the agent.

Security considerations

 • No Docker socket inside agent (or sidecar) containers.
 • AppArmor: run the sidecar with the same profile as the agent.
 • The toolexec socket path is unique per session and only mounted into the containers you launch.

Performance

 • Mount language caches as named volumes:
    • Rust: aifo-cargo-registry, aifo-cargo-git
    • Node: aifo-npm-cache, optionally mount node_modules in the workspace
    • Python: aifo-pip-cache, and a project-local .venv in /workspace
 • Parallel builds are fine: the listener can multiplex requests; or one request at a time per sidecar (configurable).

Alternative (simpler but less robust): inject a toolchain into the agent via a pre-populated bind mount

 • For Rust only (works best for pure-Rust crates):
    • Before launching agent, run a one-shot rust:1.80-slim container with -v "$PWD/.aifo/toolchains/rust:/opt/rust" and HOME=/opt/rust to run: rustup-init
      -y --profile minimal --default-toolchain stable --no-modify-path
    • Then mount the resulting .aifo/toolchains/rust into the agent and set:
       • PATH=/opt/rust/.cargo/bin:$PATH
       • CARGO_HOME=/opt/rust/.cargo
       • RUSTUP_HOME=/opt/rust/.rustup
    • Pros: agent sees cargo/rustc directly, no IPC.
    • Cons: lacks system compilers/libs for crates with native deps; brittle across glibc variants; heavier to maintain for Node/Python.

Brief implementation outline in this repo

 • Add CLI:
    • in src/main.rs: a repeatable --toolchain <name[@version]> and optional --toolchain-image overrides.
 • Orchestration:
    • in src/lib.rs: helpers to create a session id, network, sidecar run specs, start/stop lifecycle, and toolexec listener.
    • reuse desired_apparmor_profile() when spawning sidecars.
 • Shim binaries:
    • add a tiny static “aifo-shim” that connects to a Unix socket and forwards argv/env/cwd (build it as part of the release; symlink/copy it to names
      cargo, rustc, npm, node, python, pip as needed).
 • Mounts/caches:
    • compute and create named volumes; attach them to sidecars; only workspace is shared RW between agent and sidecars.
 • Cleanup:
    • on process exit, stop sidecars and agent, remove the session network (keep caches).

Example user flow

 • aifo-coder aider --toolchain rust -- cargo build --release
    • Agent starts and has “cargo” on PATH (shim).
    • The build runs in a rust:1.80-slim sidecar and writes target/ into the shared workspace.
    • The agent can run/test the built binaries immediately.

If you prefer the inject-a-toolchain mount for a first iteration, I can set up the Rust bootstrap path now, and we can add the sidecar + shim later.
Otherwise I’ll implement the sidecar + shim design directly.

--

Short answer Run the agent as-is and add a “toolchain sidecar” container that shares the workspace. Put tiny shims in front of PATH inside the agent; those
shims forward toolchain invocations (cargo, rustc, npm, node, python, pip, …) to the sidecar via the aifo-coder launcher, which performs docker exec into the
sidecar. This keeps agent images slim, avoids ABI mismatches, and needs no Docker socket in the agent.

Why this is the best fit here

 • No image bloat: you don’t rebuild agent images to add Rust/Node/Python.
 • ABI-safe: toolchain runs in its own official image (glibc/musl differences don’t break).
 • Secure: no privileged mounts into the agent; you reuse the same AppArmor profile.
 • Reusable: the same mechanism works for rust, node, python, etc.

Minimal design

 • CLI: add a repeatable flag: --toolchain <lang[@ver]>, e.g. --toolchain rust@1.80.
 • Sidecar(s): for each requested toolchain, start a container like rust:1.80-slim with:
    • -v "$PWD:/workspace"
    • language caches (named volumes) for speed
    • --user "$(id -u):$(id -g)"
    • shared user-defined network with the agent
    • shared user-defined network with the agent
    • command: sleep infinity
 • PATH shims: mount a small directory into the agent at /opt/aifo/bin and prepend PATH.
    • Provide small “shim” executables named cargo, rustc, npm, node, python, pip, …
    • Each shim connects to a Unix socket (e.g., /run/aifo/toolexec.sock) and sends argv/env/cwd.
 • Exec proxy: aifo-coder listens on that socket on the host and runs:
    • docker exec -u uid:gid -w /workspace -e … aifo-tc-rust- cargo build …
    • Stdio and exit codes are proxied back; to the agent “cargo build” looks local.

How to implement here (brief)




 • src/main.rs
    • Add CLI: --toolchain  (repeatable) and optional --toolchain-image rust=rust:1.80-slim.
    • In the normal run path, if toolchains requested:
       1 Create a session id and user-defined docker network (aifo-net-).
       2 Start sidecars (see below).
       3 Create a runtime dir (e.g., $XDG_RUNTIME_DIR/aifo-coder/) and a shim dir (build-time artifact).
       4 Spawn a toolexec listener thread (Unix socket file in the runtime dir).
       5 Launch the agent with:
          • same workspace mount
          • -v runtime_dir:/run/aifo
          • -v shim_dir:/opt/aifo/bin
          • PATH=/opt/aifo/bin:$PATH and AIFO_TOOLEEXEC_SOCK=/run/aifo/toolexec.sock
       6 On exit, stop sidecars and delete the network.
 • src/lib.rs
    • Add helpers:
       • create_session_id(), create/remove_network()
       • start_toolchain_sidecar(lang, image, uid, gid, mounts, network, apparmor?)
       • stop_sidecar(name)
       • toolexec_server(sock, routing_map) that:
          • validates request
          • maps binary name to sidecar (e.g., cargo -> rust)
          • runs docker exec with proper user, cwd=/workspace, env passthrough
          • runs docker exec with proper user, cwd=/workspace, env passthrough
          • forwards stdout/stderr/exit status
    • Reuse desired_apparmor_profile() for sidecars: pass --security-opt apparmor=… when enabled.
 • Shim (new tiny helper binary)
    • New crate aifo-shim (or a small module compiled to a static binary).
    • Behavior: read AIFO_TOOLEEXEC_SOCK, capture argv/env/cwd, connect to the socket, forward, then mirror streams and exit status.
    • For each exposed tool, either symlink the same shim under the tool’s name or generate multiple copies.
    • Build it in release and mount from host into agent as /opt/aifo/bin/.
 • Makefile
    • Build the shim along with the launcher.
    • Install or package the shim into dist; at runtime the launcher will know where to mount it from.

Toolchain-specific mounts (examples)

 • Rust sidecar
    • Workspace: -v "$PWD:/workspace"
    • Caches: -v aifo-cargo-registry:/usr/local/cargo/registry, -v aifo-cargo-git:/usr/local/cargo/git
    • USER: --user "$(id -u):$(id -g)"
    • Image: rust:1.80-slim (override via --toolchain-image)
 • Node sidecar
    • Workspace: -v "$PWD:/workspace"
    • Caches: -v aifo-npm-cache:/home/coder/.npm
    • Optionally leave node_modules inside workspace for agent visibility
 • Python sidecar
    • Workspace: -v "$PWD:/workspace"
    • Caches: -v aifo-pip-cache:/home/coder/.cache/pip
    • Encourage a project .venv inside /workspace for isolation

Incremental rollout

 • Phase 1 (quick): support --toolchain rust and a new subcommand to run tools explicitly without shims:
    • aifo-coder toolchain rust -- cargo build
    • Lets you validate sidecar startup, mounts, UID/GID, caches.
 • Phase 2: add shims + socket for transparent PATH interception so the agent can call cargo directly.

Notes and caveats

 • Windows: start with Linux/macOS (Unix socket). For Windows, fall back to a TCP localhost listener.
 • Images: keep sidecar images configurable to pin versions or swap distros as needed.
 • Cleanup: handle SIGINT/SIGTERM so sidecars and network are torn down.

If you want, I can draft:

 • The new CLI flags and sidecar lifecycle helpers in src/main.rs/src/lib.rs.
 • A minimal aifo-shim crate (50–100 lines).
 • Makefile updates to build and mount the shim.
