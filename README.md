
# 🚀  Welcome to the Migros AI Foundaton Coder  🚀

🔒 Secure by Design | 🌍 Cross-Platform | 🦀 Powered by Rust | 🧠 Developed by AIFO

## ✨ Features:
- Linux: Coding agents run securely inside Docker containers with AppArmor.
- macOS: Transparent VM with Docker ensures isolated and secure agent execution.

## ⚙️  Secure Coding Agents
- Environment with Secure Containerization Enabled
- Platform with Adaptive Security for Linux & macOS

## 🔧 Building a safer future for coding automation in Migros Group
- Container isolation on Linux & macOS
- Agents run inside a container, not on your host runtimes
- AppArmor Support (via Docker or Colima)
- No privileged Docker mode; no host Docker socket is mounted
- Minimal attack surface area
- Only the current project folder and essential per‑tool config/state paths are mounted
- Nothing else from your home directory is exposed by default
- Principle of least privilege
- No additional host devices, sockets or secrets are mounted

## Prerequisites and installation

Requirements:
- Docker installed and running
- GNU Make (recommended for the provided Makefile targets)
- Optional: Rust stable toolchain (only needed if you build the CLI locally)

Quick install:
```bash
make build
./aifo-coder --help
```

> For Powershell you can use `./aifo-coder.ps1`

Optional:
```bash
make build-fat
make build-slim
make build-launcher
./scripts/build-images.sh
```

Notes:
- By default, images are minimized by dropping apt and procps in final stages. To keep them, build with KEEP_APT=1 (see “Image build options and package dropping” below).
- The aifo-coder wrapper will auto-build the Rust launcher with cargo when possible; if cargo is missing, it can build via Docker.

## macOS cross-build (osxcross)

You can build the macOS launcher on Linux CI using osxcross with an Apple SDK injected via masked CI
secrets. This produces macOS artifacts without requiring a macOS host.

- Overview:
  - Cross image stage: macos-cross-rust-builder (Dockerfile).
  - CI jobs: build-macos-cross-rust-builder (Kaniko), build-launcher-macos (arm64), optional build-launcher-macos-x86_64.
  - Artifacts: dist/aifo-coder-macos-arm64 and dist/aifo-coder-macos-x86_64 (optional).
  - Security: SDK is never committed or artifacted; it’s decoded from APPLE_SDK_BASE64 (masked + protected) only in CI.

- Documentation:
  - Prerequisites: docs/ci/macos-cross-prereqs.md
  - Validation: docs/ci/macos-cross-validation.md

- Local cross-build (developer convenience; Linux host):
  - Place the Apple SDK tarball under ci/osx/MacOSX13.3.sdk.tar.xz (do not commit it).
  - Build the cross image:
    make build-macos-cross-rust-builder
  - Build both macOS launchers:
    make build-launcher-macos-cross
  - Or only arm64:
    make build-launcher-macos-cross-arm64
  - Validate with file(1):
    make validate-macos-artifact
  - Optional x86_64:
    make build-launcher-macos-cross-x86_64
    make validate-macos-artifact-x86_64

- CI summary:
  - build-macos-cross-rust-builder:
    - Runs on tags; allowed on default-branch manual runs and schedules; tags image as :$CI_COMMIT_TAG or :ci.
  - build-launcher-macos:
    - Runs on tags; builds aarch64-apple-darwin and validates with file(1); best-effort otool -hv check.
  - build-launcher-macos-x86_64 (optional):
    - Runs on tags; builds x86_64-apple-darwin and validates with file(1); best-effort otool -hv check.
  - publish-release:
    - Attaches Linux and macOS artifacts; exposes links aifo-coder, aifo-coder-macos-arm64, aifo-coder-macos-x86_64.

- Pinning osxcross (optional stability):
  - Dockerfile supports build-arg OSXCROSS_REF to pin osxcross to a specific commit:
    docker build --target macos-cross-rust-builder --build-arg OSXCROSS_REF=<commit> .

## CLI usage and arguments

Synopsis:
```bash
./aifo-coder {codex|crush|aider|openhands|opencode|plandex|toolchain|toolchain-cache-clear|doctor|images|cache-clear|fork} [global-flags] [-- [AGENT-OPTIONS]]
```

> For Powershell you can use `./aifo-coder.ps1`

Global flags:
- --image <ref>                   Override full image reference for all agents
- --flavor <full|slim>            Select image flavor; default is full
- --verbose                       Increase logging verbosity
- --dry-run                       Print the docker run command without executing it
- --invalidate-registry-cache     Invalidate on-disk registry probe cache and re-probe
- -h, --help                      Show help
- --toolchain <kind>              Attach toolchains (repeatable): rust, node, typescript, python, c-cpp, go
- --toolchain-spec <kind@ver>     Attach toolchains with optional version (repeatable), e.g. rust@1.80, node@20, python@3.12
- --toolchain-image <k=img>       Override toolchain image (repeatable), e.g. c-cpp=aifo-coder-toolchain-cpp:latest
- --no-toolchain-cache            Disable named cache volumes for toolchain sidecars
- --toolchain-unix-socket         Linux: use unix:/// socket transport for the proxy
- --toolchain-bootstrap <opt>     Bootstrap actions (repeatable), e.g. typescript=global
- --non-interactive               Disable interactive LLM prompt (same as AIFO_CODER_SUPPRESS_LLM_WARNING=1)

> **Node: pnpm-only guard.** Repository tooling is designed for pnpm. Avoid `npm install`/`yarn install`
> directly in this repo; use `make node-install` or run `pnpm install --frozen-lockfile` in the repo
> root. For CI or local preflight, you can run `make node-guard` to check for accidental npm/yarn use.

Subcommands:
- codex [args...]                Run OpenAI Codex CLI inside container
- crush [args...]                Run Charmbracelet Crush inside container
- aider [args...]                Run Aider inside container
- openhands [args...]            Run OpenHands inside container
- opencode [args...]             Run OpenCode inside container
- plandex [args...]              Run Plandex inside container
- toolchain <kind> -- [args...]  Run a command inside a language toolchain sidecar (Phase 1)
- toolchain-cache-clear          Purge all toolchain cache volumes (cargo, npm, pip, ccache, go)
- doctor                         Run environment diagnostics (Docker/AppArmor/UID mapping)
- images                         Print effective image references (honoring flavor/registry)
- cache-clear                    Clear the on-disk registry probe cache (alias: cache-invalidate)
- fork list [--json] [--all-repos]  List fork sessions under the current repo or workspace
- fork clean [--session <sid> | --older-than <days> | --all] [--dry-run] [--yes] [--keep-dirty | --force] [--json]  Clean fork sessions safely

Tips:
- Two registries: mirror registry (MR) is used only for Dockerfile base pulls (build-time) via REGISTRY_PREFIX; internal registry (IR) is used for tagging/push and runtime image prefixing via AIFO_CODER_INTERNAL_REGISTRY_PREFIX or REGISTRY in Makefile/scripts. The obsolete AIFO_CODER_REGISTRY_PREFIX is ignored.
- To select slim images via environment, set AIFO_CODER_IMAGE_FLAVOR=slim.
- Overrides supported: AIFO_CODER_IMAGE (full ref), AIFO_CODER_IMAGE_PREFIX/TAG/FLAVOR. For runtime registry prefixing of our images, use AIFO_CODER_INTERNAL_REGISTRY_PREFIX.
- Fallback: if images are not yet published, use --image to provide an explicit image ref.

Examples (dry-run previews):
```bash
./aifo-coder --dry-run openhands -- --help
./aifo-coder --dry-run opencode  -- --help
./aifo-coder --dry-run plandex   -- --help
```

PATH policy:
- openhands, opencode, plandex: shims-first (/opt/aifo/bin first)
- codex, crush: node-first
- aider: adds /opt/venv/bin before system paths

## Telemetry (OpenTelemetry) (optional)

aifo-coder includes optional OpenTelemetry-based tracing and metrics behind Cargo features and
environment variables. Telemetry is enabled by default in feature-enabled builds and never required
for normal use (disable via `AIFO_CODER_OTEL=0|false|no|off`).

- Build-time features:
  - `otel`: enables tracing and installs the OpenTelemetry layer (no fmt logs by default).
  - `otel-otlp`: extends `otel` with OTLP HTTP exporter support (no gRPC).
- Runtime enablement (when built with `otel`):
  - Default: telemetry enabled; disable with `AIFO_CODER_OTEL=0|false|no|off`.
  - `OTEL_EXPORTER_OTLP_ENDPOINT` (non-empty) selects the OTLP endpoint (HTTP/HTTPS, e.g., `https://localhost:4318`).
  - `AIFO_CODER_TRACING_FMT=1` opts into a fmt logging layer on stderr (honors `RUST_LOG`,
    default filter `warn`).
  - `AIFO_CODER_OTEL_METRICS` controls metrics instruments/exporter (default enabled).
  - CLI `--verbose` sets `AIFO_CODER_OTEL_VERBOSE=1` to print concise initialization info.
- Privacy:
  - By default, sensitive values (paths/args) are recorded as counts and salted hashes.
  - Setting `AIFO_CODER_OTEL_PII=1` allows raw values for debugging; do not use this in production.

Examples:

```bash
# Build the launcher with telemetry features (uses CARGO_FLAGS, default: --features otel-otlp)
make build-launcher

# Disable telemetry (baseline)
AIFO_CODER_OTEL=0 ./aifo-coder --help

# Build with a baked-in default OTLP endpoint (local build; CI uses protected variables)
AIFO_OTEL_ENDPOINT=https://localhost:4318 \
AIFO_OTEL_TRANSPORT=http \
make build-launcher

# At runtime, override baked-in defaults with OTEL_EXPORTER_OTLP_ENDPOINT
OTEL_EXPORTER_OTLP_ENDPOINT=https://other-collector:4318 \
./aifo-coder --help

# Traces with fmt logging and RUST_LOG control
AIFO_CODER_TRACING_FMT=1 RUST_LOG=aifo_coder=info \
  ./aifo-coder --help

# Send metrics/traces via OTLP HTTP (launcher built with otel-otlp features)
OTEL_EXPORTER_OTLP_ENDPOINT=https://localhost:4318 \
  ./aifo-coder --help
```

For crate-level development without the Makefile or launcher, you can still use:

```bash
cargo build --features otel
cargo build --features otel-otlp
cargo run --features otel -- --help
```

- Exporters and sinks:
  - Transport: OTLP over HTTP/HTTPS (no gRPC).
  - Traces: provider installed; without an endpoint, no external export occurs (use the fmt layer for local visibility).
  - Metrics: with an endpoint, exports via OTLP; in debug mode (`--debug-otel-otlp` sets `AIFO_CODER_OTEL_DEBUG_OTLP=1`), exports to a development sink (stderr/file).
- Propagation:
  - The shim forwards W3C traceparent (from TRACEPARENT env) to the proxy over HTTP/Unix.
  - The proxy extracts context and creates child spans; it also injects TRACEPARENT into sidecar execs.
- Sampling and timeouts:
  - `OTEL_TRACES_SAMPLER` / `OTEL_TRACES_SAMPLER_ARG` control sampling (e.g., `parentbased_traceidratio`).
  - `OTEL_EXPORTER_OTLP_TIMEOUT` controls exporter timeouts (default 5s). BSP envs (`OTEL_BSP_*`) are respected.
- CI invariant:
  - Golden stdout: enabling/disabling telemetry must not change CLI stdout. See ci/otel-golden-stdout.sh.
- Safety/idempotence:
  - `telemetry_init()` is idempotent; if a subscriber exists, init is skipped with a concise stderr message.
  - No stdout changes or exit code changes due to telemetry; TraceContext propagator only (no Baggage).

For more details (endpoint precedence, HTTP transport, CI checks), see `docs/README-opentelemetry.md`.

Telemetry tests:
- Run unit/integration tests (no Docker): make test
- Golden stdout and smoke (no Docker): ci/otel-golden-stdout.sh

# The aifo-coder

Containerized launcher and Docker images bundling six terminal AI coding agents:

- OpenAI Codex CLI (`codex`)
- Charmbracelet Crush (`crush`)
- Aider (`aider`)
- OpenHands (`openhands`)
- OpenCode (`opencode`)
- Plandex (`plandex`)

Run these tools inside containers while keeping them feeling “native” on your machine:
- Seamless access to your working directory
- Your configs and state mounted from your host
- Your credentials forwarded via environment variables
- Correct file ownership (UID/GID mapping)
- A single host-side entrypoint: the Rust CLI `aifo-coder`
- Optional AppArmor confinement via Docker

## Why aifo-coder?

Modern coding agents are powerful, but installing and managing multiple CLIs (and their fast‑moving dependencies) can feel heavy and risky on a developer laptop. aifo‑coder bundles six terminal agents (Codex, Crush, Aider, OpenHands, OpenCode and Plandex) into reproducible container images and gives you a tiny Rust launcher that makes them feel native. You get a clean, consistent runtime every time without polluting the host.

Typical use cases:
- Try or evaluate multiple agents without touching your host Python/Node setups.
- Keep your dev machine lean while still enjoying rich agent tooling.
- Share a single, known‑good environment across teams or CI.
- Protect your host by containing agent execution to an isolated environment.

## How it works (at a glance)

- The Dockerfile builds a shared base and six per‑agent images via multi‑stage targets:
  - aifo-coder-codex:TAG, aifo-coder-crush:TAG, aifo-coder-aider:TAG,
    aifo-coder-openhands:TAG, aifo-coder-opencode:TAG, aifo-coder-plandex:TAG
- The Rust `aifo-coder` launcher runs the selected agent inside the appropriate image, mounting only what’s needed:
  - Your current working directory is mounted at `/workspace`.
  - Minimal, well‑known config/state directories are mounted into the container `$HOME=/home/coder` so agents behave like locally installed tools.
  - Common credentials are forwarded via environment variables you already export on your host.
  - Your UID/GID are mapped into the container so files created in `/workspace` are owned by you.
- A lightweight lock prevents multiple agents from running concurrently against the same workspace.

## Security, isolation & privacy by design

aifo‑coder takes a “contain what matters, nothing more” approach:
- Container isolation
  - Agents run inside a container, not on your host runtimes.
  - No privileged Docker mode; no host Docker socket is mounted.
  - A sane `$HOME` inside the container (`/home/coder`) keeps agent caches/configs scoped.
  - NSS wrapper provides a passwd entry for your runtime UID so editors don’t complain about missing home accounts.
- Minimal surface area
  - Only the current project folder (`$PWD`) and essential per‑tool config/state paths are mounted:
    - `~/.codex` (Codex), `~/.local/share/crush` (Crush), `~/.aider` + common Aider config files, and `~/.gitconfig`.
    - Host `~/.gnupg` is mounted read‑only at `/home/coder/.gnupg-host` and copied into the container on start.
  - Nothing else from your home directory is exposed by default.
- Principle of least privilege
  - UID/GID mapping ensures any files written inside `/workspace` are owned by your host user—no unexpected root‑owned files.
  - No additional host devices, sockets or secrets are mounted.
- AppArmor (via Docker)
  - When supported by Docker, the launcher adds `--security-opt apparmor=<profile>`.

### AppArmor on macOS (Colima) and Docker Desktop

- macOS (Colima):
  - Build the profile from the template: make apparmor
  - Load it into the Colima VM:
```bash
colima ssh -- sudo apparmor_parser -r -W "$PWD/build/apparmor/aifo-coder"
```
  - If the custom profile is not available, the launcher will fall back to docker-default automatically.
- Docker Desktop (macOS/Windows):
  - Docker runs inside a VM; AppArmor support and profiles are managed by the VM. The launcher defaults to docker-default on these platforms.
- Native Linux:
  - If the aifo-coder profile is loaded on the host, it will be used; otherwise docker-default is used when available, or no explicit profile.

Troubleshooting:
- Check Docker AppArmor support:
```bash
docker info --format '{{json .SecurityOptions}}'
```
- List loaded profiles (Linux):
```bash
cat /sys/kernel/security/apparmor/profiles | grep -E 'aifo-coder|docker-default' || true
```

---

## Requirements

- Docker installed and running
- GNU Make for the provided Makefile targets
- Optional: Rust stable toolchain (only needed if you build the CLI locally via Makefile)

If you need to access a private base image:
- Base image used: `repository.migros.net/node:22-bookworm-slim`
- If you cannot access this, replace the `FROM` line in the Dockerfile with an accessible equivalent.

No Rust or Make installed on your host? Use the Docker-based dev helper:
- Make the script executable once:
  chmod +x scripts/dev.sh
- Run tests via Docker:
  ./scripts/dev.sh test
- Generate a CycloneDX SBOM via Docker:
  ./scripts/dev.sh sbom

---

## Quick start

- Build both slim and fat images:
```bash
make build
```

- If make is not installed on your host, use the Docker-only helper script:
```bash
./scripts/build-images.sh
```

- Build only slim variants (smaller images, fewer tools):
```bash
make build-slim
```

- Build only fat (full) variants:
```bash
make build-fat
```

- Build the Rust launcher locally (optional if you already have the binary):
```bash
make build-launcher
```

- Run the launcher:
```bash
./aifo-coder --help
```

- Launch an agent:
```bash
./aifo-coder codex --profile o3 --sandbox read-only --ask-for-approval on-failure
```

> For Powershell you can use `./aifo-coder.ps1`


All trailing arguments after the agent subcommand are passed through to the agent unchanged.

### Toolchains (Phases 2–4)

For transparent PATH shims, the toolexec proxy (TCP and Linux unix sockets), per-language caches, the C/C++ sidecar image, and optional smokes, see:
- docs/TOOLCHAINS.md

Dev‑tool routing:
- For make, cmake, ninja, pkg-config, gcc, g++, clang, clang++, cc, c++, the proxy selects the first running sidecar that provides the tool, in this order: c-cpp, rust, go, node, python. This avoids starting unnecessary sidecars when a tool is already available in another running sidecar (for example, make inside rust).

Linux note:
- On Linux you can use a unix:/// transport for the tool‑exec proxy to reduce TCP surface and simplify networking. See INSTALL.md for unix sockets and the TCP host‑gateway note.

## Fork mode

Run multiple containerized agent panes side by side on cloned workspaces to explore different approaches in parallel, then merge back when done.

When to use:
- Compare alternative fixes/implementations safely without touching your base working tree.
- Split tasks across panes while keeping isolation and reproducibility.
- Preserve clones for later inspection or merging even if orchestration fails (default).

Usage and flags:
- Basic:
  - aifo-coder --fork 2 aider --
  - aifo-coder --fork 3 --fork-session-name aifo-exp aider --
- Include dirty working tree via snapshot (no hooks/signing; temporary index + commit-tree):
  - aifo-coder --fork 2 --fork-include-dirty aider --
- Independent object stores (slower, more disk):
  - aifo-coder --fork 2 --fork-dissociate aider --
- Layouts (tmux): tiled (default), even-h, even-v:
  - aifo-coder --fork 3 --fork-layout even-h aider --
- Keep clones on orchestration failure (default: keep; can disable):
  - aifo-coder --fork 2 --fork-keep-on-failure=false aider --

Paths and naming:
- Clones live under: <repo-root>/.aifo-coder/forks/<sid>/pane-1..N
- Branch names: fork/<base|detached>/<sid>-<i> (i starts at 1)

Per-pane state:
- Each pane mounts its own state directory to avoid concurrent writes:
  - Default base: ~/.aifo-coder/state/<sid>/pane-<i>/{.aider,.codex,.crush}
  - Override base with AIFO_CODER_FORK_STATE_BASE

Post-session merging guidance:
- Fetch/merge from a pane:
  - git -C "<root>" remote add "fork-<sid>-1" "<pane-1-dir>"
  - git -C "<root>" fetch "fork-<sid>-1" "fork/<base>/<sid>-1"
  - git -C "<root>" merge --no-ff "fork/<base>/<sid>-1"
- Cherry-pick:
  - git -C "<root>" cherry-pick <sha1> [...]
- Rebase:
  - git -C "<root>" checkout -b "tmp/fork-<sid>-1" "fork/<base>/<sid>-1"
  - git -C "<root>" rebase "<base-branch>"
- Format-patch/am:
  - git -C "<pane-1-dir>" format-patch -o "<out-dir>" "<base-ref>"
  - git -C "<root>" am "<out-dir>/*.patch"
- Push a branch to a remote and open PR/MR.

Performance notes:
- N>8 panes will stress I/O and memory; consider fewer panes or --fork-dissociate to avoid shared object GC interactions.

Orchestrators:
- Linux/macOS/WSL: tmux session with N panes (required).
- Windows: Windows Terminal (wt.exe) preferred; falls back to PowerShell windows or Git Bash/mintty.

Tip: Maintenance commands help manage sessions:
- aifo-coder fork list [--json] [--all-repos]
- aifo-coder fork clean [--session <sid> | --older-than <days> | --all] [--dry-run] [--yes] [--keep-dirty | --force] [--json]

## Makefile targets

A quick reference of all Makefile targets.

| Target                              | Category   | Description                                                                                   |
|-------------------------------------|------------|-----------------------------------------------------------------------------------------------|
| build                               | Build      | Build both slim and fat images (all agents)                                                   |
| build-coder                         | Build      | Build slim + fat images and rust-builder (all agents)                                         |
| build-fat                           | Build      | Build all fat images (codex, crush, aider, openhands, opencode, plandex)                      |
| build-slim                          | Build      | Build all slim images (codex-slim, crush-slim, aider-slim,<br> openhands-slim, opencode-slim, plandex-slim) |
| build-codex                         | Build      | Build only the Codex image (`${IMAGE_PREFIX}-codex:${TAG}`)                                   |
| build-crush                         | Build      | Build only the Crush image (`${IMAGE_PREFIX}-crush:${TAG}`)                                   |
| build-aider                         | Build      | Build only the Aider image (`${IMAGE_PREFIX}-aider:${TAG}`)                                   |
| build-openhands                     | Build      | Build only the OpenHands image (`${IMAGE_PREFIX}-openhands:${TAG}`)                           |
| build-opencode                      | Build      | Build only the OpenCode image (`${IMAGE_PREFIX}-opencode:${TAG}`)                             |
| build-plandex                       | Build      | Build only the Plandex image (`${IMAGE_PREFIX}-plandex:${TAG}`)                               |
| build-codex-slim                    | Build      | Build only the Codex slim image (`${IMAGE_PREFIX}-codex-slim:${TAG}`)                         |
| build-crush-slim                    | Build      | Build only the Crush slim image (`${IMAGE_PREFIX}-crush-slim:${TAG}`)                         |
| build-aider-slim                    | Build      | Build only the Aider slim image (`${IMAGE_PREFIX}-aider-slim:${TAG}`)                         |
| build-openhands-slim                | Build      | Build only the OpenHands slim image (`${IMAGE_PREFIX}-openhands-slim:${TAG}`)                 |
| build-opencode-slim                 | Build      | Build only the OpenCode slim image (`${IMAGE_PREFIX}-opencode-slim:${TAG}`)                   |
| build-plandex-slim                  | Build      | Build only the Plandex slim image (`${IMAGE_PREFIX}-plandex-slim:${TAG}`)                     |
| build-rust-builder                  | Build      | Build the Rust cross-compile builder image (`${IMAGE_PREFIX}-rust-builder:${TAG}`)            |
| build-macos-cross-rust-builder      | Build      | Build osxcross-based macOS cross image (requires `ci/osx/<SDK>`)                              |
| build-toolchain                     | Build      | Build all toolchain sidecar images (rust, node, cpp)                                          |
| build-toolchain-rust                | Build      | Build the Rust toolchain sidecar image (`$(TC_REPO_RUST):$(RUST_TOOLCHAIN_TAG)`)              |
| build-toolchain-node                | Build      | Build the Node toolchain sidecar image (`$(TC_REPO_NODE):$(NODE_TOOLCHAIN_TAG)`)              |
| build-toolchain-cpp                 | Build      | Build the C/C++ toolchain sidecar image (`$(TC_REPO_CPP):latest`)                             |
| build-launcher                      | Release    | Build the Rust host launcher (release build)                                                  |
| build-launcher-macos-cross          | Build      | Build macOS arm64 and x86_64 launchers using the macOS cross image                            |
| build-launcher-macos-cross-arm64    | Build      | Build macOS arm64 launcher using the macOS cross image                                        |
| build-launcher-macos-cross-x86_64   | Build      | Build macOS x86_64 launcher using the macOS cross image                                       |
| validate-macos-artifact             | Utility    | Validate macOS arm64 Mach-O via file(1)                                                       |
| validate-macos-artifact-x86_64      | Utility    | Validate macOS x86_64 Mach-O via file(1)                                                      |
| rebuild                             | Rebuild    | Rebuild both slim and fat images without cache                                                |
| rebuild-coder                       | Rebuild    | Rebuild slim, fat and builder images (all agents) without cache                               |
| rebuild-fat                         | Rebuild    | Rebuild all fat images without cache                                                          |
| rebuild-slim                        | Rebuild    | Rebuild all slim images without cache                                                         |
| rebuild-codex                       | Rebuild    | Rebuild only Codex, no cache                                                                  |
| rebuild-crush                       | Rebuild    | Rebuild only Crush, no cache                                                                  |
| rebuild-aider                       | Rebuild    | Rebuild only Aider, no cache                                                                  |
| rebuild-openhands                   | Rebuild    | Rebuild only OpenHands, no cache                                                              |
| rebuild-opencode                    | Rebuild    | Rebuild only OpenCode, no cache                                                               |
| rebuild-plandex                     | Rebuild    | Rebuild only Plandex, no cache                                                                |
| rebuild-codex-slim                  | Rebuild    | Rebuild only Codex slim, no cache                                                             |
| rebuild-crush-slim                  | Rebuild    | Rebuild only Crush slim, no cache                                                             |
| rebuild-aider-slim                  | Rebuild    | Rebuild only Aider slim, no cache                                                             |
| rebuild-openhands-slim              | Rebuild    | Rebuild only OpenHands slim, no cache                                                         |
| rebuild-opencode-slim               | Rebuild    | Rebuild only OpenCode slim, no cache                                                          |
| rebuild-plandex-slim                | Rebuild    | Rebuild only Plandex slim, no cache                                                           |
| rebuild-existing                    | Rebuild    | Rebuild any existing local images with `IMAGE_PREFIX` (using cache)                           |
| rebuild-existing-nocache            | Rebuild    | Rebuild any existing local images with `IMAGE_PREFIX` (no cache)                              |
| rebuild-rust-builder                | Rebuild    | Rebuild only the Rust builder image without cache                                             |
| rebuild-toolchain                   | Rebuild    | Rebuild all toolchain images without cache                                                    |
| rebuild-toolchain-rust              | Rebuild    | Rebuild only the Rust toolchain image without cache                                           |
| rebuild-toolchain-node              | Rebuild    | Rebuild only the Node toolchain image without cache                                           |
| rebuild-toolchain-cpp               | Rebuild    | Rebuild only the C/C++ toolchain image without cache                                          |
| publish                             | Publish    | Buildx multi-arch and push all images (set PLATFORMS=… and PUSH=1)                            |
| publish-toolchain-rust              | Publish    | Buildx multi-arch and push Rust toolchain (PLATFORMS=…, PUSH=1)                               |
| publish-toolchain-node              | Publish    | Buildx multi-arch and push Node toolchain (PLATFORMS=…, PUSH=1)                               |
| publish-toolchain-cpp               | Publish    | Buildx multi-arch and push C/C++ toolchain (PLATFORMS=…, PUSH=1)                              |
| clean                               | Utility    | Remove built images (ignores errors if not present)                                           |
| toolchain-cache-clear               | Utility    | Purge all toolchain cache Docker volumes (cargo, npm, pip, ccache, go)                        |
| loc                                 | Utility    | Count lines of code across key file types                                                     |
| docker-images                       | Utility    | Show the available images in the local Docker registry                                        |
| docker-enter                        | Utility    | Enter a running container via docker exec with GPG runtime prepared                           |
| hadolint                            | Utility    | Lint Dockerfiles with hadolint (advisory)                                                     |
| test                                | Utility    | Run the Rust test suite (cargo-nextest preferred; cargo test fallback)                        |
| checksums                           | Utility    | Generate dist/SHA256SUMS.txt for current artifacts                                            |
| sbom                                | Utility    | Generate CycloneDX SBOM into dist/SBOM.cdx.json (requires cargo-cyclonedx)                    |
| gpg-disable-signing                 | GPG        | Disable GPG commit signing for the current repo                                               |
| gpg-enable-signing                  | GPG        | Enable GPG commit signing for the current repo                                                |
| gpg-show-config                     | GPG        | Show effective GPG/Git signing configuration                                                  |
| gpg-disable-signing-global          | GPG        | Disable GPG commit signing globally                                                           |
| gpg-unset-signing                   | GPG        | Unset repo signing configuration                                                              |
| git-show-signatures                 | GPG        | Show commit signature status (git log %h %G? %s)                                              |
| git-commit-no-sign                  | GPG        | Make a commit without signing                                                                 |
| git-amend-no-sign                   | GPG        | Amend the last commit without signing                                                          |
| git-commit-no-sign-all              | GPG        | Commit all staged changes without signing                                                     |
| scrub-coauthors                     | History    | Remove a specific “Co‑authored‑by” line from all commit messages (uses git‑filter‑repo)       |
| apparmor                            | AppArmor   | Generate build/apparmor/${APPARMOR_PROFILE_NAME} from template (used by Docker)               |
| apparmor-load-colima                | AppArmor   | Load the generated profile into the Colima VM (macOS)                                         |
| apparmor-log-colima                 | AppArmor   | Stream AppArmor logs (Colima VM or local Linux) into build/logs/apparmor.log                  |

Variables used by these targets:

| Variable                | Default                         | Purpose                                                                     |
|-------------------------|---------------------------------|-----------------------------------------------------------------------------|
| IMAGE_PREFIX            | aifo-coder                      | Image name prefix for per‑agent images                                      |
| TAG                     | latest                          | Tag for images                                                              |
| REGISTRY                | (unset)                         | Registry prefix to push/pull (e.g., repository.migros.net/)                 |
| KEEP_APT                | 0                               | If 1, keep apt/procps in final images; 0 (default) drops them after install |
| USE_BUILDX              | 1                               | Use docker buildx when available                                            |
| PLATFORMS               | (unset)                         | Comma-separated platforms for buildx (e.g., linux/amd64,linux/arm64)        |
| PUSH                    | 0                               | With PLATFORMS set, push multi-arch images instead of loading               |
| CACHE_DIR               | .buildx-cache                   | Local buildx cache dir for faster rebuilds                                  |
| RUST_TOOLCHAIN_TAG      | latest                          | Tag used for Rust toolchain sidecar (TC_REPO_RUST)                          |
| NODE_TOOLCHAIN_TAG      | latest                          | Tag used for Node toolchain sidecar (TC_REPO_NODE)                          |
| RUST_BASE_TAG           | 1-bookworm                      | Base rust image tag for toolchain build                                     |
| NODE_BASE_TAG           | 22-bookworm-slim                | Base node image tag for toolchain build                                     |
| TC_REPO_RUST            | aifo-coder-toolchain-rust       | Repository/name for Rust toolchain image                                    |
| TC_REPO_NODE            | aifo-coder-toolchain-node       | Repository/name for Node toolchain image                                    |
| TC_REPO_CPP             | aifo-coder-toolchain-cpp        | Repository/name for C/C++ toolchain image                                   |
| OSX_SDK_FILENAME        | MacOSX13.3.sdk.tar.xz           | Apple SDK filename expected in ci/osx/ for osxcross build                   |
| APPARMOR_PROFILE_NAME   | aifo-coder                      | Rendered AppArmor profile name                                              |
| APP_NAME                | aifo-coder                      | App bundle name used for macOS .app                                         |
| APP_BUNDLE_ID           | ch.migros.aifo-coder            | macOS bundle identifier for the .app                                        |
| DMG_NAME                | aifo-coder-<version>            | DMG file base name (macOS)                                                  |
| APP_ICON                | (none)                          | Path to a .icns icon to include in the .app (optional)                      |

---

## Cross-compiling and Rust specifics

This repository uses native rustup toolchains for host builds. Linux→macOS cross builds are supported via an osxcross-based Docker stage and CI. No cross-rs containers or Cross.toml are used.

Recommended approach:
- Use release-for-target to build and package binaries for the current host or selected targets:
  - make release-for-target
- Build Linux artifacts from macOS quickly:
  - make release-for-linux
- Build both macOS (host) and Linux:
  - make release
- Specify multiple targets explicitly:
  - RELEASE_TARGETS='x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu' make release-for-target
- Ensure required Rust targets are installed:
  - rustup target add <triple> for each target you build

Notes about linkers:
- For host builds, no special setup is needed.
- For non-host Linux targets on macOS, you may need a linker toolchain. One option is to install osxct toolchains (by SergioBenitez) via Homebrew; another is to use a system-provided gcc. You can also point cargo to a linker via .cargo/config.toml:
  [target.x86_64-unknown-linux-gnu]
  linker = "x86_64-unknown-linux-gnu-gcc"

Summary:
- Prefer make release-for-target with rustup-installed targets.
- Use make build-launcher for a quick host-only build.

### macOS signing and notarization (optional)

There are two common paths to sign macOS artifacts:

#### CI behavior (unchanged)

CI builds and publishes macOS artifacts, but it does not perform signing or notarization:
- CI MUST NOT run `codesign`, `notarytool`, or `stapler`.
- CI MUST NOT depend on `SIGN_IDENTITY` or `NOTARY_PROFILE`.
- CI MAY still produce and publish unsigned macOS binaries (e.g. from osxcross) and/or copy them into `dist/` as:
  - `dist/aifo-coder-macos-arm64`
  - `dist/aifo-coder-macos-x86_64`

Unsigned artifacts may trigger Gatekeeper warnings on end-user machines. Signed/notarized assets are produced locally on
macOS (see below).

#### Local developer workflows (macOS)

Two supported local workflows:

1) Self-signed / non-Apple identity (internal use)
- Configure a local signing identity (login keychain), e.g.:
  - `export SIGN_IDENTITY="Migros AI Foundation Code Signer"`
  - `unset NOTARY_PROFILE`
- Run:
  - `make release-macos-binary-signed`
- Result:
  - `dist/aifo-coder-<version>-macos-*.zip` containing signed (but not notarized) per-arch binaries.
- Note:
  - Notarization is skipped automatically for non-Apple identities.

2) Apple Developer ID (public distribution)
- Configure:
  - `export SIGN_IDENTITY="Developer ID Application: <Org Name> (<TEAMID>)"`
  - `export NOTARY_PROFILE="<notarytool-profile>"`
- Run:
  - `make release-macos-binary-signed`
- Result:
  - Per-arch `.zip` assets are signed and submitted for notarization; stapling is attempted best-effort.

Existing DMG workflow:
- The DMG pipeline remains independent and continues to use:
  - `make release-app`
  - `make release-dmg`
  - `make release-dmg-sign`
- You can reuse the same `SIGN_IDENTITY` / `NOTARY_PROFILE` settings for both per-arch zips and DMG signing.

#### Release assets (recommended)

After producing artifacts, the recommended release assets are:

- Linux:
  - `aifo-coder-linux-x86_64.tar.gz` (contains the Linux binary + README.md + NOTICE + LICENSE)
- macOS (per-arch, preferred for CLI users):
  - `dist/aifo-coder-<version>-macos-arm64.zip`
  - `dist/aifo-coder-<version>-macos-x86_64.zip` (if produced)
  - Optional raw signed binaries:
    - `dist/aifo-coder-macos-arm64`
    - `dist/aifo-coder-macos-x86_64`
- macOS (GUI users):
  - Signed DMG via `make release-dmg-sign` (recommended for drag-and-drop install)

Notes:
- CI macOS artifacts are unsigned; Gatekeeper prompts may appear on end-user machines.
- Signed/notarized macOS artifacts are produced locally on macOS.

#### Creating per-arch signed macOS zip assets (local, macOS)

These targets package already-built per-arch macOS launcher binaries into `dist/`:

- Normalize existing `target/*-apple-darwin/release/$(BIN_NAME)` into canonical dist names:
  - `make release-macos-binaries-normalize-local`
- Sign those dist binaries in place:
  - `make release-macos-binaries-sign`
- Create per-arch zip files including required docs:
  - `make release-macos-binaries-zips`
- Optional notarization (Developer ID + NOTARY_PROFILE required):
  - `make release-macos-binaries-zips-notarize`
- One-shot helper (build host arch + normalize + sign + zip + optional notarize):
  - `make release-macos-binary-signed`

Outputs (versioned to avoid collisions):
- `dist/aifo-coder-<version>-macos-arm64.zip`
- `dist/aifo-coder-<version>-macos-x86_64.zip` (if produced)

Tip (tagged releases):
- To produce zip filenames that match a Git tag without renaming, set:
  - `MACOS_ZIP_VERSION=<tag>`
  - Example: `make release-macos-binary-signed MACOS_ZIP_VERSION=v1.2.3`

Verification (recommended):
- `make verify-macos-signed`

Publish signed zips to the GitLab Release (tag pipelines):
- CI does not sign/notarize. Instead, upload the locally produced zips to the GitLab Generic Package
  Registry for the tag, then run the manual CI job that attaches links to the Release.
- Expected filenames (must match the tag name, to avoid collisions):
  - `aifo-coder-<tag>-macos-arm64.zip`
  - `aifo-coder-<tag>-macos-x86_64.zip`
- Steps:
  1) Build/sign/notarize locally on macOS (produces versioned zips in dist/):
     - `make release-macos-binary-signed`
  2) Rename zips to use the tag (example for tag v1.2.3):
     - `mv dist/aifo-coder-<version>-macos-arm64.zip dist/aifo-coder-v1.2.3-macos-arm64.zip`
     - `mv dist/aifo-coder-<version>-macos-x86_64.zip dist/aifo-coder-v1.2.3-macos-x86_64.zip`
  3) Upload the renamed zips to the Generic Package Registry for the tag:
     - `curl --header "PRIVATE-TOKEN: <token>" --upload-file dist/aifo-coder-v1.2.3-macos-arm64.zip \
         "<CI_API_V4_URL>/projects/<id>/packages/generic/<project>/v1.2.3/aifo-coder-v1.2.3-macos-arm64.zip"`
     - `curl --header "PRIVATE-TOKEN: <token>" --upload-file dist/aifo-coder-v1.2.3-macos-x86_64.zip \
         "<CI_API_V4_URL>/projects/<id>/packages/generic/<project>/v1.2.3/aifo-coder-v1.2.3-macos-x86_64.zip"`
  4) In the tag pipeline, run the manual job:
     - `publish-macos-signed-zips`

These targets do not invoke Cargo builds directly (except `release-macos-binary-signed`, which calls
`make build-launcher` first).

1) Apple Developer identity (Apple Distribution / Developer ID Application):
- Produces artifacts eligible for notarization.
- The Makefile target release-dmg-sign will detect an Apple identity and use hardened runtime flags and timestamps automatically.
- Notarization requires Xcode CLT and a stored notary profile.

2) Self‑signed Code Signing certificate (no Apple account):
- Useful for internal testing or distribution within a trusted environment.
- Not notarizable; Gatekeeper prompts may appear on other machines unless the certificate is trusted on those hosts.

Self‑signed certificate via Keychain Access (login keychain):
- Open Keychain Access.
- Ensure “login” is the active keychain.
- Menu: Keychain Access → Certificate Assistant → Create a Certificate…
  - Name: choose a clear name (e.g., Migros AI Foundation Code Signer)
  - Identity Type: Self Signed Root
  - Certificate Type: Code Signing (ensures Extended Key Usage includes Code Signing)
  - Key Size: 2048 (or 4096)
  - Location: login keychain
- Finish, then verify a private key exists under the certificate.
- Optional: In the certificate’s Trust settings, set Code Signing to “Always Trust” for smoother codesign usage.

Build and sign with your chosen identity name:
```bash
make release-app
make release-dmg-sign SIGN_IDENTITY="Migros AI Foundation Code Signer"
```

Notes for self‑signed usage:
- The release-dmg-sign target will:
  - Clear extended attributes on the app if needed.
  - Sign the inner executable and the .app bundle with basic flags for non‑Apple identities.
  - Rebuild the DMG from the signed app and sign the DMG.
  - Skip notarization automatically if the identity is not an Apple Developer identity.
- If prompted for key access, allow codesign to use the private key.
- If your login keychain is locked, you may need to unlock it first:
```bash
security unlock-keychain -p "<your-password>" login.keychain-db
```
- If you previously signed artifacts or see quarantine issues:
```bash
xattr -cr "dist/aifo-coder.app" "dist/aifo-coder.dmg"
```

Apple notarization workflow (requires Apple Developer identity and profile):
```bash
# Store credentials once (example)
xcrun notarytool store-credentials AC_NOTARY --apple-id "<apple-id>" --team-id "<team-id>" --password "<app-specific-password>"

# Create a signed DMG (Makefile will sign app and DMG, then rebuild DMG)
make release-dmg-sign NOTARY_PROFILE="AC_NOTARY"

# If needed, staple tickets (usually release-dmg-sign already staples)
xcrun stapler staple "dist/aifo-coder.dmg"
xcrun stapler staple "dist/aifo-coder.app"
```

Tip:
- The DMG includes an /Applications symlink for drag‑and‑drop install; you can further customize a background image.

---

## What the images contain

- Node-based global CLIs:
  - `@openai/codex` (Codex)
  - `@charmland/crush` (Crush)
  - `opencode-ai` installed globally via npm (OpenCode)
- Python-based CLIs via `uv`:
  - `aider` installed into `/opt/venv` (PEP 668‑safe)
  - `openhands` installed into `/opt/venv-openhands` via `uv` + pip; wrapper at `/usr/local/bin/openhands` (executes the venv console script `openhands`)
- Go-based CLI:
  - `plandex` built from source and installed to `/usr/local/bin`
- `dumb-init`, `git`, `ripgrep`, `curl`, `emacs-nox`, `vim`, `nano`, `mg`, `nvi`
- GnuPG (`gnupg`, `pinentry-curses`) and NSS wrapper (`libnss-wrapper`)
- Default working directory: `/workspace`
- Entrypoint `/usr/local/bin/aifo-entrypoint`:
  - Ensures `$HOME` and `$GNUPGHOME` exist (0700), prepares `$XDG_RUNTIME_DIR`
  - Copies keys from `/home/coder/.gnupg-host` (read‑only mount) into `GNUPGHOME`
  - Configures pinentry to `pinentry-curses` and launches `gpg-agent`

### Host notifications command (notifications-cmd)

- Available inside agent containers as notifications-cmd.
- When invoked, it asks the host listener to run say with the provided arguments, but only if the full command equals the notifications-command configured in ~/.aider.conf.yml.
- If the configured command is missing or does not match, execution is rejected with a clear reason. This feature requires toolchains to be enabled so the internal proxy is running.

Windows note:
- The notifications-command parser requires the first token (the executable) to be an absolute
  Unix-style path that starts with "/". Pure Windows paths like "C:\Program Files\..." are rejected.
  This strictness is by design for v2.
- On Windows, use one of the following approaches:
  - Run the host under WSL2 and point to a Linux absolute path (e.g., /usr/bin/notify-send).
  - Use a POSIX layer that exposes Unix-like paths (e.g., MSYS/Cygwin paths such as /usr/bin/… or
    /cygdrive/c/…).
  - Alternatively, run the host on Linux or macOS.
- The allowlist defaults to the basename "say". You can extend it via
  AIFO_NOTIFICATIONS_ALLOWLIST (comma-separated basenames), but the configured path must still be
  absolute and Unix-style.

### Slim image variants

For smaller footprints, use the -slim variants of each image:

- aifo-coder-codex-slim:TAG
- aifo-coder-crush-slim:TAG
- aifo-coder-aider-slim:TAG
- aifo-coder-openhands-slim:TAG
- aifo-coder-opencode-slim:TAG
- aifo-coder-plandex-slim:TAG

Differences from the full images:
- Based on the same Debian Bookworm base
- Heavy editors (emacs-nox, vim, nano) and ripgrep are omitted; lightweight editors mg and nvi are included
- Otherwise identical behavior and entrypoint

Editors installed:
- Full images: emacs-nox, vim, nano, mg, nvi
- Slim images: mg, nvi

How to use:
- Build slim only: make build-slim
- Build fat only: make build-fat
- Build both: make build
- Run via explicit image: ./aifo-coder --image aifo-coder-codex-slim:latest codex --version
- Or pass a CLI flag or set an environment variable for automatic selection:
  - ./aifo-coder --flavor slim codex --version
  - export AIFO_CODER_IMAGE_FLAVOR=slim

## Image build options and package dropping

### Version pins

By default, agent versions are installed at latest. To pin reproducible releases, set these variables when building or publishing:
- CODEX_VERSION: npm @openai/codex (default: latest)
- CRUSH_VERSION: npm @charmland/crush (default: latest)
- AIDER_VERSION: pip aider-chat (default: latest)
- OPENHANDS_VERSION: pip openhands-ai (default: latest)
- OPENCODE_VERSION: npm opencode-ai (default: latest)
- PLANDEX_GIT_REF: git ref for Plandex CLI (default: main)

Examples:
- make publish-openhands PUSH=1 REGISTRY=... OPENHANDS_VERSION=0.3.1
- make publish-opencode PUSH=1 REGISTRY=... OPENCODE_VERSION=0.6.0
- make publish-aider PUSH=1 REGISTRY=... AIDER_VERSION=0.52.0 WITH_PLAYWRIGHT=1
- make publish-codex PUSH=1 REGISTRY=... CODEX_VERSION=1.2.3
- make publish-crush PUSH=1 REGISTRY=... CRUSH_VERSION=0.18.4
- make publish-plandex PUSH=1 REGISTRY=... PLANDEX_GIT_REF=v0.9.0

Notes:
- Crush fallback to GitHub binary is only attempted when CRUSH_VERSION is pinned to a concrete version; with the default latest, we rely on npm.

During image builds, the final runtime stages drop apt and procps by default to minimize attack surface. You can opt out by setting KEEP_APT=1.

Default removal sequence (applied when KEEP_APT=0):
```bash
# Remove apt and clean up
apt-get remove --purge -y apt apt-get
apt-get autoremove -y
apt-get clean
rm -rf /var/lib/apt/lists/*
```
Additionally, procps is removed when present.

How to keep apt/procps:
```bash
make KEEP_APT=1 build
make KEEP_APT=1 build-slim
```

These options propagate as Docker build-args so you can also pass them directly when invoking docker build manually.

---

## Runtime launching

> For Powershell you can use `./aifo-coder.ps1`

Use the Rust launcher:

```bash
./aifo-coder {codex|crush|aider} [agent-args...]
```

Examples:

- Run Codex with a profile and safe sandbox:
```bash
./aifo-coder codex --profile o3 --sandbox read-only --ask-for-approval on-failure
```

- Run Crush with debug:
```bash
./aifo-coder crush --debug
```

- Run Aider with a specific model:
```bash
./aifo-coder aider --model o3-mini --yes
```

Override the image used by the launcher (use a specific per‑agent image):
```bash
./aifo-coder --image myrepo/aifo-coder-codex:dev codex --version
```

---

## How the launcher works

When you run `aifo-coder ...` it will:

1. Acquire a lock to ensure only one agent runs at a time:
   - If inside a Git repository: prefer `<repo-root>/.aifo-coder.lock`; fallback to `$XDG_RUNTIME_DIR/aifo-coder.<hash(repo_root)>.lock`; legacy fallback `/tmp/aifo-coder.lock`.
   - If not inside a Git repository: legacy candidates `~/.aifo-coder.lock`, `$XDG_RUNTIME_DIR/aifo-coder.lock`, `/tmp/aifo-coder.lock`, and `./.aifo-coder.lock`.
2. Locate Docker.
3. Build a `docker run` command with:
   - `--rm` removal after exit
   - Interactive TTY (`-it`) if connected to a terminal; otherwise `-i`
   - Bind mount your current directory to `/workspace` and set `-w /workspace`
   - Map your UID:GID (`--user UID:GID`) so files written in `/workspace` are owned by you
   - Set a sane home and Codex home:
     - `HOME=/home/coder`
     - `CODEX_HOME=/home/coder/.codex`
   - Prepare GnuPG runtime:
     - `GNUPGHOME=/home/coder/.gnupg`
     - `XDG_RUNTIME_DIR=/tmp/runtime-<uid>`
     - Mount host `~/.gnupg` read‑only at `/home/coder/.gnupg-host` for key import
   - Bind mount persistent config/state:
     - `~/.local/share/crush` → `/home/coder/.local/share/crush`
     - `~/.codex` → `/home/coder/.codex`
     - `~/.aider` → `/home/coder/.aider`
     - `~/.aider.conf.yml` → `/home/coder/.aider.conf.yml`
     - `~/.aider.model.metadata.json` → `/home/coder/.aider.model.metadata.json`
     - `~/.aider.model.settings.yml` → `/home/coder/.aider.model.settings.yml`
     - `~/.gitconfig` → `/home/coder/.gitconfig`
   - Timezone passthrough (if present):
     - `/etc/localtime` and `/etc/timezone` mounted read‑only
   - AppArmor (optional): adds `--security-opt apparmor=<profile>` if supported by Docker
   - Per‑agent image selection:
     - Defaults to `AIFO_CODER_IMAGE` if set; otherwise `IMAGE_PREFIX-<agent>:TAG` (e.g., `aifo-coder-codex:latest`)
     - Runtime prefixing for our images: when `AIFO_CODER_INTERNAL_REGISTRY_PREFIX` is non-empty, it is prepended to our agent images (aifo-coder-*). Official upstream defaults (e.g., python/golang/rust) remain unprefixed at runtime.
     - Build-time base pulls use a mirror registry (MR) via Docker `--build-arg REGISTRY_PREFIX=...` set by Makefile/CI; the launcher does not use MR at runtime.
4. Execute the agent and return its exit code.

---

## Environment variables

Forwarded from host to container (only if set in your shell):

| Variable                   | Forwarded | Notes                                                                                 |
|---------------------------|-----------|---------------------------------------------------------------------------------------|
| OPENAI_API_KEY            | Yes       | Generic & Codex                                                                       |
| OPENAI_ORG                | Yes       | Generic & Codex                                                                       |
| OPENAI_BASE_URL           | Yes       | Generic & Codex                                                                       |
| CODEX_OSS_BASE_URL        | Yes       | Codex OSS                                                                             |
| CODEX_OSS_PORT            | Yes       | Codex OSS                                                                             |
| CODEX_HOME                | Yes       | Also set in container to `/home/coder/.codex`; forwarded value may influence behavior |
| GEMINI_API_KEY            | Yes       | Google / Vertex / Gemini                                                              |
| VERTEXAI_PROJECT          | Yes       | Google / Vertex / Gemini                                                              |
| VERTEXAI_LOCATION         | Yes       | Google / Vertex / Gemini                                                              |
| AZURE_OPENAI_API_ENDPOINT | Yes       | Azure                                                                                 |
| AZURE_OPENAI_API_KEY      | Yes       | Azure                                                                                 |
| AZURE_OPENAI_API_VERSION  | Yes       | Azure                                                                                 |
| AZURE_OPENAI_ENDPOINT     | Yes       | Azure                                                                                 |
| AZURE_API_KEY             | Yes       | Azure                                                                                 |
| AZURE_API_BASE            | Yes       | Azure                                                                                 |
| AZURE_API_VERSION         | Yes       | Azure                                                                                 |
| GIT_AUTHOR_NAME           | Yes       | Optional override                                                                     |
| GIT_AUTHOR_EMAIL          | Yes       | Optional override                                                                     |
| GIT_COMMITTER_NAME        | Yes       | Optional override                                                                     |
| GIT_COMMITTER_EMAIL       | Yes       | Optional override                                                                     |
| GIT_SIGNING_KEY           | Yes       | Select a specific GPG key for signing                                                 |
| TZ                        | Yes       | Timezone passthrough                                                                  |
| EDITOR                    | Yes       | Editor preference                                                                     |
| VISUAL                    | Yes       | Editor preference                                                                     |

Always set inside the container:

| Variable         | Value                       | Notes                          |
|------------------|-----------------------------|--------------------------------|
| HOME             | /home/coder                 | Canonical container home       |
| USER             | coder                       | Runtime user                   |
| CODEX_HOME       | /home/coder/.codex          | Ensures consistent Codex home  |
| GNUPGHOME        | /home/coder/.gnupg          | GPG runtime location           |
| XDG_RUNTIME_DIR  | /tmp/runtime-<uid>          | Computed by the launcher       |

Launcher control variables (read by the Rust launcher):

| Variable                  | Default/Behavior                                              |
|---------------------------|---------------------------------------------------------------|
| AIFO_CODER_IMAGE          | If set, overrides the full image reference for all agents     |
| AIFO_CODER_IMAGE_PREFIX   | Default: `aifo-coder`                                         |
| AIFO_CODER_IMAGE_TAG      | Default: `latest`                                             |
| AIFO_CODER_CONTAINER_NAME | If set, assigns the container name                            |
| AIFO_CODER_HOSTNAME       | If set, assigns the container hostname                        |
| AIFO_CODER_APPARMOR_PROFILE | Override AppArmor profile; defaults: docker-default on Docker-in-VM (macOS/Windows), aifo-coder on native Linux |
| AIFO_CODER_INTERNAL_REGISTRY_PREFIX | If set (non-empty), prepend this prefix to our images at runtime; normalized to a single trailing “/”. Empty/unset means no prefix. |
| AIFO_CODER_IMAGE_FLAVOR     | Optional: set to `slim` to select `-slim` image variants instead of default full images |

---

## Configuration & persistence

The launcher mounts common config/state from your host to make the tools behave as if installed locally:

- Crush state (cache, logs, etc.):
  - Host: `~/.local/share/crush` → Container: `/home/coder/.local/share/crush`
- Codex:
  - Host: `~/.codex` → Container: `/home/coder/.codex`
  - Inside container `CODEX_HOME` is set to `/home/coder/.codex`
- Aider:
  - State & Cache: `~/.aider` → `/home/coder/.aider`
  - Root-level config files:
    - `~/.aider.conf.yml` → `/home/coder/.aider.conf.yml`
    - `~/.aider.model.metadata.json` → `/home/coder/.aider.model.metadata.json`
    - `~/.aider.model.settings.yml` → `/home/coder/.aider.model.settings.yml`
- Git:
  - `~/.gitconfig` → `/home/coder/.gitconfig`
- Timezone:
  - `/etc/localtime` and `/etc/timezone` mounted read‑only when present

Crush example config:
```bash
./aifo-coder crush --config /workspace/examples/sandbox/crush/crush.json
```

> For Powershell you can use `./aifo-coder.ps1`

---

## Rust implementation notes

- CLI parsing is powered by Clap; subcommands are `codex`, `crush`, `aider`, `openhands`, `opencode`, `plandex`. Trailing arguments are passed through to the agent unchanged.
- TTY detection uses `atty` to select `-it` vs `-i` for interactive runs.
- The launcher uses Docker; ensure it is installed and available in PATH.
- The default image selection can be overridden via `AIFO_CODER_IMAGE`, or computed from `AIFO_CODER_IMAGE_PREFIX` and `AIFO_CODER_IMAGE_TAG`.
- A lock file is used to avoid concurrent runs against the same workspace; candidate locations include `$HOME`, `$XDG_RUNTIME_DIR`, and `/tmp`.
- Arguments are shell-escaped conservatively before passing to the container.

---

## License and copyright

Licensed under the Apache License, Version 2.0.

You may not use this project except in compliance with the License.
You may obtain a copy of the License at:

  http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, this software is
distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND,
either express or implied. See the License for the specific language governing
permissions and limitations under the License.

Copyright (c) 2025, Amir Guindehi <amir.guindehi@mgb.ch>, Head of the Migros AI Foundation.

---

## Acknowledgements

- OpenAI Codex CLI: https://github.com/openai/codex
- Charmbracelet Crush: https://github.com/charmbracelet/crush
- Aider: https://github.com/Aider-AI/aider
