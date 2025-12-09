# Toolchain sidecars and proxy

This document explains how the toolchain sidecars (rust, node, python, c/cpp, go) are used together
with the tool-exec proxy and shims.

- Toolchains run in dedicated containers (sidecars) with `/workspace` mounted.
- Agents talk to a **tool-exec proxy** which routes tools (cargo, pnpm, python, gcc, go, etc.)
  into the correct sidecar.
- PATH shims inside the agent container make tools “just work” as if they were installed locally.

Use this guide as the primary user-facing documentation for all toolchains. For low-level
implementation details, see `docs/README-contributing.md` and the `src/toolchain/*` modules.

---

## Common behavior for all toolchains

### How toolchains are attached

You attach toolchains to any agent command via the `--toolchain` or `--toolchain-spec` flags:

```bash
# Attach Rust toolchain to Aider
aifo-coder --toolchain rust aider -- cargo --version

# Attach Node + Python toolchains
aifo-coder --toolchain node --toolchain python aider -- npx --version

# Attach specific versions (where supported)
aifo-coder --toolchain-spec rust@1.80 --toolchain-spec node@22 aider -- cargo --help
```

- `--toolchain <kind>` (repeatable): attaches one or more toolchains.
- `--toolchain-spec <kind@ver>`: picks a versioned toolchain image when supported.
- The launcher:
  - Starts sidecar containers (one per requested kind).
  - Starts the tool-exec proxy.
  - Exports `AIFO_TOOLEEXEC_URL` and `AIFO_TOOLEEXEC_TOKEN` into the agent container.
  - Injects PATH shims that route tools into sidecars.

Toolchains share a per-session network (`aifo-net-<id>`) so agents and sidecars can talk only
to each other, not directly to each other’s inner services.

### Workspace mount and home layout

All toolchains:

- Mount the current project directory as:
  - `$PWD` → `/workspace` (read/write, working directory).
- Standard home:
  - `HOME=/home/coder`
  - `GNUPGHOME=/home/coder/.gnupg`
- Caches:
  - Consolidated under `XDG_CACHE_HOME=/home/coder/.cache` where possible.

### Caches and named volumes

Each toolchain uses Docker named volumes for caches (see per-toolchain sections):

- Rust: cargo registry/git caches.
- Node: pnpm/npm/yarn/deno cache + pnpm store.
- Python: pip cache.
- C/C++: ccache (and optionally sccache).
- Go: go build/module cache.

You can purge all toolchain caches via:

```bash
aifo-coder toolchain-cache-clear
```

or equivalently:

```bash
make toolchain-cache-clear
```

### Proxy and routing (high level)

The tool-exec proxy:

- Receives tool execution requests from shims in the agent container.
- Routes tools to the first sidecar that can handle them, using allowlists:
  - Rust: `cargo`, `rustc`, `rustup`, `cargo-nextest`, etc.
  - Node: `node`, `npm`, `npx`, `pnpm`, `yarn`, `deno`, `tsc`, `ts-node`, `bun` (via node).
  - Python: `python`, `python3`, `pip`, `pip3`.
  - Go: `go`, `gofmt`.
  - C/C++: `gcc`, `g++`, `clang`, `clang++`, `cc`, `c++`, `cmake`, `make`, `ninja`, `pkg-config`.
- Dev tools are routed with a preference order (roughly: `c-cpp`, rust, go, node, python).

Protocol details and error semantics (401/403/409/426/504) are documented in
`docs/README-toolexec.md`.

---

## AIFO Rust Toolchain

The Rust toolchain sidecar provides a consistent Rust environment with:

- `cargo`, `rustc`, `rustup`, `cargo-nextest`, `clippy`, `rustfmt`.
- Optional `sccache` for faster builds.
- Stable caches and named volumes for registries and git checkouts.

### When to use

Use the Rust toolchain when:

- You want to compile or test Rust code (`cargo build`, `cargo test`, `cargo nextest`).
- You want to avoid polluting your host toolchain with project-specific dependencies.
- You want builds and tests to run in a predictable container environment.

### Image and overrides

Default image:

- `aifo-coder-toolchain-rust:<tag>` where `<tag>` is `RUST_TOOLCHAIN_TAG` or `latest`.

Override via environment (from `docs/README-contributing.md`):

- `AIFO_RUST_TOOLCHAIN_IMAGE` – full image reference.
- `AIFO_RUST_TOOLCHAIN_VERSION` – logical version to map to an image tag.
- `AIFO_RUST_TOOLCHAIN_USE_OFFICIAL=1` – prefer official `rust:<ver>` images where supported.

### Environment and caches

Key environment variables:

- `CARGO_HOME=/home/coder/.cargo`
- `RUSTUP_HOME=/usr/local/rustup` (default)
- `RUST_BACKTRACE=1` (default; can be overridden)
- `SCCACHE_DIR=/home/coder/.cache/sccache` (when sccache is enabled)
- `CC=gcc`, `CXX=g++` (linker defaults)

Cache volumes (see `docs/README-contributing.md`):

- `aifo-cargo-registry:/home/coder/.cargo/registry`
- `aifo-cargo-git:/home/coder/.cargo/git`
- Consolidated cache under `XDG_CACHE_HOME=/home/coder/.cache` when configured.

Ownership initialization:

- `init_rust_named_volumes_if_needed(...)` runs a short helper container that:
  - Ensures the target dir exists (registry/git).
  - Chowns it to the invoking UID:GID.
  - Writes `.aifo-init-done` to avoid repeated work.

### Official Rust image mode

When `AIFO_RUST_TOOLCHAIN_USE_OFFICIAL=1` or using an official `rust:<ver>` image:

- `CARGO_HOME`/`RUSTUP_HOME` are aligned to the image defaults (`/usr/local/cargo`, `/usr/local/rustup`).
- The launcher avoids forcing `RUSTUP_TOOLCHAIN` to reduce channel sync chatter.
- A `BootstrapGuard` (`AIFO_RUST_OFFICIAL_BOOTSTRAP=1`) coordinates bootstrap for:
  - `cargo-nextest` installation.
  - `clippy`/`rustfmt` components.
  - Optional `sccache` presence checks.

### sccache integration

When `AIFO_RUST_SCCACHE=1`:

- The Rust sidecar configures `RUSTC_WRAPPER=sccache`.
- `SCCACHE_DIR=/home/coder/.cache/sccache`.
- You are responsible for ensuring `sccache` is installed in the image; the launcher emits warnings
  when requested but not present.

### Example usage

Basic Rust version:

```bash
aifo-coder --toolchain rust aider -- cargo --version
aifo-coder --toolchain rust aider -- rustc --version
```

Run tests with nextest:

```bash
aifo-coder --toolchain rust aider -- cargo nextest run
```

Force official `rust:1.80`:

```bash
AIFO_RUST_TOOLCHAIN_USE_OFFICIAL=1 \
AIFO_RUST_TOOLCHAIN_VERSION=1.80 \
aifo-coder --toolchain rust aider -- cargo --version
```

### Troubleshooting (Rust)

Common issues:

- **Slow first run due to rustup sync**:
  - Prefer official images or set `AIFO_RUST_TOOLCHAIN_USE_OFFICIAL=1` to use rust:<ver>.
- **Permissions on cargo volumes**:
  - The helper chown logic should fix this; if it fails, check Docker logs and rerun with `--verbose`.
- **Missing `cargo-nextest`**:
  - The bootstrap path installs it automatically into `/usr/local/cargo/bin` when needed.

---

## Node toolchain: pnpm, shared store, and per-OS node_modules

The Node sidecar is designed to support pnpm with:

- A **shared, repo-local store** under `<repo>/.pnpm-store` that is reused across:
  - macOS hosts
  - Linux toolchain sidecars
  - CI jobs
- A **per-OS `node_modules` overlay** mounted at `/workspace/node_modules` so native artifacts
  are always built for the correct platform and never shared across OSes.

The Node sidecar is designed to support pnpm with:

- A **shared, repo-local store** under `<repo>/.pnpm-store` that is reused across:
  - macOS hosts
  - Linux toolchain sidecars
  - CI jobs
- A **per-OS `node_modules` overlay** mounted at `/workspace/node_modules` so native artifacts
  are always built for the correct platform and never shared across OSes.

### Directory layout and env vars

On host and in the Node toolchain sidecar:

- Repository root:
  - `pnpm-lock.yaml` – canonical dependency graph (pnpm-only repo).
  - `.pnpm-store/` – pnpm content-addressable store (ignored by git).
  - `node_modules/` – OS-specific dependency tree.

- pnpm configuration:
  - `.npmrc`:
    - `store-dir=.pnpm-store`
    - `virtual-store-dir=node_modules/.pnpm`
    - `prefer-frozen-lockfile=true`
  - Environment (sidecar):
    - `PNPM_STORE_PATH=/workspace/.pnpm-store` (overridable via host `PNPM_STORE_PATH`)
    - `PNPM_HOME=/home/coder/.local/share/pnpm`
    - `XDG_CACHE_HOME=/home/coder/.cache`
    - `NPM_CONFIG_CACHE=/home/coder/.cache/npm`
    - `YARN_CACHE_FOLDER=/home/coder/.cache/yarn`
    - `DENO_DIR=/home/coder/.cache/deno`
    - `AIFO_NODE_OVERLAY_SENTINEL=/workspace/node_modules/.aifo-node-overlay`

### Volumes and mounts

The Node toolchain sidecar mounts:

- `aifo-node-cache:/home/coder/.cache` – consolidated Node cache volume (npm/yarn/pnpm/deno).
- `/workspace` – bind mount of the current project.
- `/workspace/.pnpm-store:/workspace/.pnpm-store` – bind mount of the repo-local pnpm store.
- `aifo-node-modules:/workspace/node_modules` – **dedicated volume** for per-OS `node_modules`.

The launcher initializes `aifo-node-cache` ownership using `init_node_cache_volume_if_needed`
so that cache files are owned by the invoking UID/GID in both host and sidecar contexts.

### Overlay guard and sentinel

To ensure correctness and prevent host/sidecar cross-contamination, the Node sidecar enforces
an overlay guard:

- Sentinel:
  - Path: `/workspace/node_modules/.aifo-node-overlay` (container-only).
  - Created by the sidecar when bootstrap logic runs successfully.
- Guard logic:
  - Runs inside the sidecar (shell script) and checks:
    - That `/workspace/node_modules` is a directory.
    - That when the sentinel exists, the device/inode of `/workspace` and `/workspace/node_modules`
      differ (detects host bind mounts).
  - On misconfiguration:
    - Emits an error explaining that `/workspace/node_modules` must be a dedicated volume or tmpfs,
      not a bind mount of host `node_modules`.
    - Returns an error to the Rust launcher, which aborts the toolchain run/session.

This ensures that **Linux node_modules overlays are always container-local** and never share
build artifacts with a macOS `node_modules` tree (or vice versa).

### Lockfile hash and automatic installs

The Node sidecar bootstraps the overlay via `pnpm install` when needed:

- `ensure_node_overlay_and_install` (Rust helper invoking `docker exec sh -lc '…'`):
  - Ensures `/workspace/node_modules` exists.
  - Computes a stable hash of `/workspace/pnpm-lock.yaml` when `sha256sum` is available:
    - Stores the hash in `/workspace/node_modules/.aifo-pnpm-lock.hash`.
  - Runs `pnpm install --frozen-lockfile` inside the sidecar when:
    - The overlay is empty, or
    - The computed lockfile hash differs from the stored hash.
  - Writes/refreshes the stored hash and the sentinel file on success (best-effort).

Call sites:

- `toolchain_run`:
  - After starting a fresh Node sidecar, calls:
    - `node_overlay_state_and_guard` (preflight + overlay validation).
    - `ensure_node_overlay_and_install` (initial install / update).
- `toolchain_start_session`:
  - After starting a Node sidecar for a multi-toolchain session, performs the same sequence.

The combination of **guard + hash-based install + sentinel** ensures:

- Per-OS `node_modules` overlays are always volume-backed and isolated.
- The sidecar automatically keeps dependencies up-to-date with the lockfile without needing
  manual `pnpm install` inside the container.

### Host workflows and CI

Host-side (developer machine):

- Use `make node-install` to:
  - Create `.pnpm-store` with safe permissions.
  - Warn when `package-lock.json` or `yarn.lock` are present (pnpm-only repo).
  - Run `pnpm install --frozen-lockfile` with `PNPM_STORE_PATH="$PWD/.pnpm-store"`.

CI:

- `pnpm-node-ci` workflow:
  - Restores `.pnpm-store` from cache keyed by OS + `pnpm-lock.yaml` hash.
  - Runs `pnpm fetch` followed by:
    - `pnpm install --offline --frozen-lockfile || pnpm install --frozen-lockfile`.
  - Invokes:
    - `./aifo-coder toolchain node -- pnpm test`
    which exercises the overlay guard and lockfile bootstrap logic described above.

### Troubleshooting Node + pnpm

Common issues and their remediation:

- **Overlay misconfiguration**:
  - Symptom: error mentioning
    `/workspace/node_modules must be a dedicated container volume or tmpfs`.
  - Fix:
    - Ensure the Node toolchain image is started via `aifo-coder` so it mounts
      `aifo-node-modules:/workspace/node_modules`.
    - Avoid bind-mounting host `node_modules` into the sidecar; only mount the workspace root.

- **Store permission mismatch**:
  - Symptom: pnpm install errors mentioning permission denied under `.pnpm-store`.
  - Fix:
    - On the host:
      - `chown -R $(id -u):$(id -g) .pnpm-store`
    - In CI:
      - Ensure that the CI user ID matches the container UID/GID or adjust ownership in a
        pre-step before `pnpm fetch/install`.

- **Stale dependencies after lockfile change**:
  - The sidecar automatically detects lockfile hash changes and re-runs
    `pnpm install --frozen-lockfile` inside `/workspace/node_modules`.
  - If issues persist, clear the overlay volume and rerun the sidecar:
    - `docker volume rm aifo-node-modules` (only if you are sure nothing else depends on it).
