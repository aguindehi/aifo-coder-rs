# AIFO‑Coder Security Architecture

This document describes the security architecture of **aifo‑coder** in detail. It explains how
security concerns influenced the design and implementation, how the launcher, containers, and
toolchain sidecars interact, and which controls exist to limit the impact of coding agents and
their dependencies.

It should be read together with:

- `README.md` (high‑level product overview and usage)
- `docs/README-coding-agent-security-issues.md` (threat/risk analysis)
- `docs/INSTALL.md`, `docs/README-contributing.md`, `docs/TOOLCHAINS.md` (operational details)

The focus here is on **architecture**: which components exist, how they are wired, and which trust
and isolation assumptions they rely on.

---

## 1. High‑level Goals and Principles

AIFO‑Coder is designed around a few core security principles:

1. **Containment by default**
   - Coding agents and toolchains run inside containers or sidecars, not directly on the host.
   - The host launcher remains small and auditable; most “complex” behavior is inside images.

2. **Least privilege**
   - Only the current project directory and a small set of per‑tool config/state paths are mounted.
   - No privileged Docker usage, no host Docker socket mounts.
   - No additional host devices or sockets are exposed by default.

3. **Predictable runtime surface**
   - PATH, environment variables, user/UID mapping, and mounts are normalized and documented.
   - Images drop apt/procps by default to reduce post‑install attack surface.

4. **Separation of concerns**
   - Coding agents (Codex, Crush, Aider, OpenHands, OpenCode, Plandex).
   - Toolchain sidecars (Rust, Node/TS, Python, C/C++, Go).
   - Tool‑exec proxy and shim for controlled command execution.
   - Fork orchestration and session management isolated from the core launcher.

5. **Secure‑by‑default UX**
   - Safety defaults (read‑only sandbox, approval workflows in agents where supported).
   - Minimal automatic “smart” behavior that might unexpectedly broaden access or egress.

---

## 2. Component Model

### 2.1 Host Launcher (`aifo-coder`)

The Rust CLI (`aifo-coder`) is the **trusted entrypoint**. It:

- Parses CLI flags via Clap (subcommands for each agent and `toolchain`, `fork`, `support`, etc.).
- Derives effective images and registry prefixes from environment variables and flags.
- Resolves the container runtime (`docker`) and constructs `docker run` invocations with:
  - `--rm` (cleanup after exit)
  - `--user UID:GID` (host user mapping)
  - `-v $PWD:/workspace -w /workspace` (project directory mount)
  - Controlled mounts for config and state (see below)
  - Optional AppArmor profile
- Handles:
  - Locking (prevent concurrent agents against the same workspace).
  - Toolchain sidecar session orchestration and proxy bootstrap.
  - Fork session layout and pane orchestration (via tmux or platform equivalent).
  - Registry probe/override logic for image references.

**Security boundaries**:

- The launcher **never runs as root intentionally**; it assumes a normal, non‑privileged user.
- It **does not mount** host Docker socket into containers; containers cannot start other
  containers by default.
- Environment variables forwarded into containers are restricted to an explicit allowlist
  (relevant keys like OpenAI/Gemini/Azure credentials, Git author details, EDITOR, etc.).

### 2.2 Agent Containers

Each coding agent runs inside a dedicated image:

- `aifo-coder-codex[:TAG]`
- `aifo-coder-crush[:TAG]`
- `aifo-coder-aider[:TAG]`
- `aifo-coder-openhands[:TAG]`
- `aifo-coder-opencode[:TAG]`
- `aifo-coder-plandex[:TAG]`

Slim variants are provided (`*-slim`) with reduced tool footprint.

**Common properties**:

- Base: Debian Bookworm slim‑style images.
- Entrypoint: `/usr/local/bin/aifo-entrypoint` that:
  - Normalizes `$HOME=/home/coder`, `$GNUPGHOME=/home/coder/.gnupg`.
  - Prepares `/tmp/runtime-<uid>` and pins `XDG_RUNTIME_DIR`.
  - Copies GPG keys from `/home/coder/.gnupg-host` (read‑only host mount) into GNUPGHOME.
- Installed tools:
  - Node CLIs (`@openai/codex`, `@charmland/crush`, `opencode-ai`).
  - Python venv(s) for Aider and OpenHands.
  - Go‑built `plandex` binary.
  - Minimal editors and utilities; full images include Emacs/Vim/Nano/Ripgrep; slim variants drop heavy editors and some diagnostics.

**Host mounts**:

The launcher mounts only the minimum required host paths:

- Workspace:
  - `$PWD` → `/workspace` (read‑write).
- Config/state:
  - `~/.codex` → `/home/coder/.codex`
  - `~/.local/share/crush` → `/home/coder/.local/share/crush`
  - `~/.aider` → `/home/coder/.aider`
  - `~/.aider.conf.yml`, `~/.aider.model.metadata.json`,
    `~/.aider.model.settings.yml` → same paths under `/home/coder`
  - `~/.gitconfig` → `/home/coder/.gitconfig`
- GnuPG:
  - `~/.gnupg` → `/home/coder/.gnupg-host` (read‑only)
- Timezone (best‑effort):
  - `/etc/localtime` and `/etc/timezone` as read‑only binds if present.

**Security posture**:

- No home‑directory wildcards: only a small set of well‑known application directories are mounted.
- No direct host secret stores (browser, OS keychains) are exposed via mounts.
- GPG keys are imported into container home; host `.gnupg` remains read‑only and separate.

### 2.3 Toolchain Sidecars

Toolchain sidecars are separate containers providing development tools:

- Rust sidecar: `aifo-coder-toolchain-rust`
- Node sidecar: `aifo-coder-toolchain-node`
- C/C++ sidecar: `aifo-coder-toolchain-cpp`
- Python, Go images (similar patterns, see `docs/README-contributing.md`)

They are attached with:

```bash
aifo-coder --toolchain rust aider -- cargo --version
```

or similar, and are orchestrated by the launcher:

- Sidecar containers are launched alongside the agent container.
- A **tool‑exec proxy** runs inside a sidecar, exposing a small shim API over HTTP (TCP or
  unix:// sockets).
- Inside the agent container, PATH shims redirect compilation/test commands to the proxy.

**Key design points**:

- Sidecars mount the same workspace (`/workspace`) and limited cache volumes under `/home/coder`.
- Caches are consolidated under `/home/coder/.cache` and named volumes:
  - `aifo-rust-cache`, `aifo-node-cache`, `aifo-python-cache`, `aifo-go-cache`, `aifo-ccpp-cache`.
- Cache initialization (`init_rust_named_volume` etc.) runs a one‑shot helper container to
  chown volumes, dropping a stamp file to avoid repeated work.

**Security impact**:

- Toolchains execute inside their own containers, reducing direct exposure of host binaries and
  credentials.
- Per‑toolchain caches live in dedicated volumes; they can be purged with
  `aifo-coder toolchain-cache-clear` to limit long‑lived artefacts or potential persistence.

### 2.4 Tool‑Exec Proxy and Shim

The **tool‑exec proxy** mediates execution of developer tools (compilers, package managers,
linters) through a consistent, controllable path:

- Shim (local script `aifo-shim`) uses curl to POST to the proxy:
  - Protocol versions:
    - v1: buffered response with `Content-Length` and `X-Exit-Code` header.
    - v2: streaming response (chunked) with `X-Exit-Code` in trailers.
  - Auth: `Authorization: Bearer <token>` semantics; proxy validates tokens.
  - Version negotiation: `X-Aifo-Proto: 1` or `2`; invalid/missing → `426 Upgrade Required`.

- Proxy routes tools to the appropriate sidecar:
  - Rust: `cargo`, `rustc`.
  - Node/TS: `node`, `npm`, `npx`, `tsc`, `ts-node`, `yarn`, `pnpm`, `deno`.
  - Python: `python`, `python3`, `pip`, `pip3`.
  - Go: `go`, `gofmt`.
  - C/C++: `gcc`, `g++`, `clang`, `clang++`, `cc`, `c++`, `make`, `cmake`, `ninja`, `pkg-config`.
  - Dynamic dev‑tool routing: selects the **first running sidecar** that provides a tool in
    order: `c-cpp`, `rust`, `go`, `node`, `python`.

**Security properties** (from `docs/README-toolexec.md`):

- Authentication and allowlists:
  - Sidecars define allowlists of permitted tools.
  - Proxy returns `403 Forbidden` if a tool is not allowed, `409 Conflict` if a tool is not
    available in any sidecar.
  - Tokens prevent unauthorized exec from other containers or processes.

- Error semantics and timeouts:
  - `401 Unauthorized` for missing/invalid auth tokens.
  - `426 Upgrade Required` when Authorization succeeds but protocol header is invalid.
  - `504 Gateway Timeout` on execution timeout.
  - Behavior aligns with explicit semantics to avoid silent failures.

- Streaming considerations:
  - v2 streaming uses chunked encoding to stream stdout/stderr back to the shim in real time.
  - No write timeout is set on streaming to avoid truncation mid‑body; process timeouts are
    handled at proxy layer instead.

This architecture centralizes tool execution decisions, enabling policy enforcement, logging, and
a clear seam for future security controls (e.g., per‑command allow lists, audit trails).

### 2.5 Fork Orchestrator

The **fork mode** orchestrator spawns multiple agent panes against cloned workspaces:

- Workspaces:
  - Base repo root is detected.
  - Per‑session directories: `.aifo-coder/forks/<sid>/pane-1..N`.
- Branching:
  - Pane branches: `fork/<base|detached>/<sid>-<i>`.
- State:
  - Each pane gets its own state directory:
    - `~/.aifo-coder/state/<sid>/pane-<i>/{.aider,.codex,.crush}`.
  - This prevents concurrent writes to the same state directories from multiple panes.

Security implications:

- Forks isolate experimental changes from the base repo until the user explicitly merges them.
- Clones and branches are clearly namespaced; accidental pushes from fork branches to critical
  remotes are easier to detect.
- Clean‑up tooling (`fork clean`) supports safe removal of old sessions without touching
  unrelated directories.

---

## 3. Isolation and Privilege Model

### 3.1 Container Isolation

On all platforms (Linux, macOS, Windows via Docker Desktop/Colima):

- Agents and toolchains run in containers.
- No privileged mode is used; no `--privileged` or host PID/NET namespaces.
- Device mounts beyond defaults are avoided; no host Docker socket is mounted.

This prevents:

- Direct manipulation of host processes.
- Arbitrary nested container creation on the host.
- Access to host kernel namespaces beyond Docker’s default.

### 3.2 AppArmor Integration

On AppArmor‑capable environments:

- A dedicated `aifo-coder` AppArmor profile can be built and loaded:
  - `make apparmor` generates the profile from templates.
  - `make apparmor-load-colima` loads it into the Colima VM on macOS.
- The launcher selects the profile as follows:
  - On Docker‑in‑VM (macOS/Windows): default `docker-default`, or `aifo-coder` when present.
  - On native Linux: `aifo-coder` when loaded, else `docker-default` when available.

Benefits:

- Restricts what containers launched by aifo‑coder can do (filesystem paths, capabilities).
- Further reduces the chance that compromised agents can interact with host security controls or
  sensitive directories.

### 3.3 UID/GID Mapping

The launcher maps the host user into containers:

- Uses `--user UID:GID` on `docker run`.
- Ensures:
  - Files written in `/workspace` are owned by the host user, not root.
  - Container processes cannot trivially change ownership of files they do not own.

Security and usability:

- Avoids root‑owned files in the project after runs.
- Keeps container processes restricted to the host user’s privileges (no implicit escalation).

### 3.4 Filesystem Scope

The launcher **whitelists** specific host paths:

- Always mounts:
  - `$PWD` → `/workspace` (and uses `-w /workspace`).
- Optionally mounts:
  - Tool‑specific config under `~/.aider`, `~/.codex`, `~/.local/share/crush`.
  - `~/.gitconfig`.
  - `~/.gnupg` in a **read‑only** fashion.

What is intentionally **not** mounted:

- Arbitrary parts of `$HOME` (documents, downloads, etc.).
- Browser and OS credential stores.
- Docker authentication files beyond what the user may already expose via environment.

---

## 4. Network and Registry Controls

### 4.1 Runtime Registry Routing

The launcher derives image references by combining:

- Image prefix: `AIFO_CODER_IMAGE_PREFIX` (default `aifo-coder`).
- Image tag: `AIFO_CODER_IMAGE_TAG` (default `latest` or `release-<version>` depending on layer).
- Internal registry prefix: `AIFO_CODER_INTERNAL_REGISTRY_PREFIX` (IR).
- Mirror registry prefix: `AIFO_CODER_MIRROR_REGISTRY_PREFIX` (MR) for third‑party images.

Behavior:

- For our images (`aifo-coder-*`):
  - IR prefix is applied when set; MR is ignored at runtime for these.
- For third‑party images (e.g., `node:22`):
  - MR prefix can be used when IR is unset to pull from an internal mirror.
- The old `AIFO_CODER_REGISTRY_PREFIX` is ignored in favor of the more explicit IR/MR scheme.

Security posture:

- Clear separation of **internal registry** vs **mirror registry**:
  - IR is where aifo‑coder images live; MR is for mirroring upstream bases.
- Runtime registry override allows enterprises to keep all images in private registries and
  avoid contacting `docker.io` from developer machines.

### 4.2 Auto‑Login Behavior

When pulling from protected registries:

- On permission‑denied pulls, the launcher can prompt the user to run `docker login` for the
  resolved registry and retry once.
- This is disabled via `AIFO_CODER_AUTO_LOGIN=0` for stricter environments.

The user remains in control of credential provisioning to registries; aifo‑coder only orchestrates
a single login prompt when needed.

---

## 5. Toolchains, Caches, and Environment Normalization

### 5.1 Toolchain Image Overrides

From `docs/README-contributing.md`, toolchain images can be overridden via environment:

- Rust:
  - `AIFO_RUST_TOOLCHAIN_IMAGE`, `AIFO_RUST_TOOLCHAIN_VERSION`,
    `AIFO_RUST_TOOLCHAIN_USE_OFFICIAL=1` for official `rust:<ver>` images.
- Node:
  - `AIFO_NODE_TOOLCHAIN_IMAGE`, `AIFO_NODE_TOOLCHAIN_VERSION`.
- Python:
  - `AIFO_PYTHON_TOOLCHAIN_IMAGE`, `AIFO_PYTHON_TOOLCHAIN_VERSION`.
- Go:
  - `AIFO_GO_TOOLCHAIN_IMAGE`, `AIFO_GO_TOOLCHAIN_VERSION`.
- C/C++:
  - `AIFO_CCPP_TOOLCHAIN_IMAGE`, `AIFO_CCPP_TOOLCHAIN_VERSION`.

Security implications:

- Enterprises can pin toolchain images to vetted references and avoid unreviewed tags.
- Official vs custom images are selected in a predictable way.

### 5.2 Cache Layout

Common scheme:

- `XDG_CACHE_HOME=/home/coder/.cache` for most toolchains.
- Per‑tool volumes:
  - Node: `aifo-node-cache:/home/coder/.cache`
  - Rust: `aifo-rust-cache:/home/coder/.cache` plus dedicated cargo registry and git volumes.
  - Python: `aifo-python-cache:/home/coder/.cache`
  - Go: `aifo-go-cache:/home/coder/.cache`
  - C/C++: `aifo-ccpp-cache:/home/coder/.cache`

Security rationale:

- Caches are consolidated and clearly named, making it easy to:
  - Purge them via CLI/Makefile targets.
  - Inspect or backup them if needed.
- Minimizes accidental reuse of caches across unrelated processes.

### 5.3 PATH and Environment Controls

Examples:

- Node sidecar:
  - Ensures `PNPM_HOME` is present and on PATH.
- Rust:
  - Adds `$CARGO_HOME/bin` to PATH so installed tools resolve.
  - Integrates with `sccache` in a predictable way.
- Python:
  - Adjusts PATH when a workspace virtualenv is active to respect local venvs.
- C/C++:
  - Optionally configures `ccache`/`sccache` via environment variables.

The primary goal is to **normalize** environments in sidecars without surprising the host or
overriding host PATH semantics.

---

## 6. Notifications Command (Host Integration)

Inside agent containers, a `notifications-cmd` binary is available:

- Purpose:
  - Ask a host listener to execute a **specific, allow‑listed command**, typically for user
    notifications (e.g., `say` on macOS).
- Rules:
  - The configured notifications command must match exactly AND be an **absolute Unix‑style path**
    starting with `/`.
  - Default allowlist basename is `say`; extended via `AIFO_NOTIFICATIONS_ALLOWLIST`.
  - On Windows:
    - Pure Windows paths (`C:\…`) are rejected; use WSL2/MSYS style paths instead.

Security implications:

- Prevents arbitrary host command execution from containers.
- Restricts host commands to a clearly configured allowlist and exact path.
- Fails closed: if configuration is missing or mismatched, execution is rejected with a clear
  reason.

---

## 7. Build and CI Guardrails

### 7.1 BuildKit and Kaniko Constraints

From `docs/README-buildkit.md`:

- Dockerfiles are written to be Kaniko‑safe:
  - Avoid `COPY --link` and `COPY --chmod`.
  - Use `RUN --mount=type=secret` and `RUN --mount=type=cache` where supported.
- CI uses Kaniko with `--use-new-run`.
- Local builds prefer `docker buildx` with BuildKit enabled (`DOCKER_BUILDKIT=1`).

Security benefits:

- Secrets injected into builds via BuildKit secrets are cleaned up within the same RUN layer.
- Apple SDK and similar sensitive artefacts are not committed to git and are injected via CI
  variables/artifacts as documented in `docs/README-macos-cross-prereqs.md`.

### 7.2 Dropping Apt and Procps

Final runtime stages remove apt and procps when `KEEP_APT=0` (default):

```bash
apt-get remove --purge -y apt apt-get
apt-get autoremove -y
apt-get clean
rm -rf /var/lib/apt/lists/*
```

Security rationale:

- Reduces attack surface in runtime images:
  - Fewer tools available for lateral movement or post‑exploitation.
  - Makes runtime containers more “appliance‑like”.

---

## 8. Testing and Safe Defaults

From `docs/README-testing.md`:

- Test lanes:
  - Unit/fast tests: no Docker required.
  - Integration tests: require Docker and specific images; self‑skip if unavailable.
  - Acceptance/E2E: heavy tests, marked `#[ignore]` by default.
- Env toggles:
  - `AIFO_CODER_TEST_DISABLE_DOCKER=1` to force docker detection failure in certain CI contexts.
  - Proxy and shim behavior toggles for edge cases and error semantics.

Security posture:

- Tests are structured to validate:
  - Correct proxy error semantics (401/403/409/426/504).
  - Proper routing and mounting behavior.
  - Notifications and shim behavior under failure conditions.
- Docker‑dependent tests self‑skip when Docker is unavailable, preventing accidental attempts to
  pull images in constrained environments.

---

## 9. Putting It All Together

The AIFO‑Coder security architecture is shaped by the understanding that **coding agents are
powerful automation**, and must be treated similarly to CI/CD runners or build systems:

- **Agents and toolchains are always containerized**, with minimal host exposure.
- **Filesystem access is tightly scoped** to the current project and a small set of config/state
  directories; host secrets are not wholesale mounted.
- **Tool execution goes through a dedicated proxy and shim**, enabling routing, allowlists, and
  explicit error semantics instead of ad‑hoc shelling out.
- **Registry and image references are fully controllable** via environment variables to align
  with enterprise registries and network policies.
- **AppArmor and UID/GID mapping** provide additional defense‑in‑depth on supported hosts.
- **Fork mode and per‑pane state** support exploratory development without compromising the base
  repository.

Enterprises can layer additional controls (VPN/e‑gress policies, host firewalls, EDR, restricted
registries, hardened base images) on top of AIFO‑Coder’s design to meet their own risk posture,
knowing that the architecture aims to minimize the surprises and make trust boundaries explicit.
