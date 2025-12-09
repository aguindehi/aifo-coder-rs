# Toolchain sidecars and proxy

This document explains how the toolchain sidecars (rust, node, python, c/cpp, go) are used together
with the tool-exec proxy and shims.

## AIFO Rust Toolchain

Details for the Rust toolchain sidecar are documented alongside the Node toolchain; see
`docs/README-contributing.md` for cache layout and image override environment variables.

## Node toolchain: pnpm, shared store, and per-OS node_modules

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
