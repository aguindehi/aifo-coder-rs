# pnpm support plan (aifo-coder-pnpm_plan-v1)

## Problem and goals
- Shared bind mounts between macOS (darwin/arm64) hosts and Linux (linux/arm64) containers currently
  reuse one `node_modules`, breaking native dependencies due to ABI mismatches.
- We must keep per-OS correctness while deduplicating downloads and JS/package payload storage.
- Solution must integrate with existing aifo-coder orchestration and require minimal manual steps.

## Constraints and observations
- Any `node_modules` tree containing native builds is tied to its build OS/arch.
- pnpm offers a content-addressable store (`.pnpm-store`) that deduplicates package payloads and lets
  multiple installs link into the shared blobs.
- Only native build artifacts (node-gyp outputs, postinstall binaries) must remain per-OS duplicates.
- Bind-mounted repos enable sharing `.pnpm-store` between host and container; containers can overlay
  `node_modules` with tmpfs or named volumes to keep per-OS trees isolated.

## Desired behavior
1. Host (macOS) and container (Linux) each materialize a platform-correct `node_modules`.
2. Both environments reuse the shared `.pnpm-store`, avoiding redundant downloads.
3. Tooling configures pnpm paths automatically and fails fast if `node_modules` would be shared.
4. Documentation explains the workflow, troubleshooting, and cleanup.

## Verification and validation
- Structural checks:
  - `.pnpm-store` lives inside the repo (e.g., `<repo>/.pnpm-store`) and is writable by host and container.
  - Container startup overlays `/workspace/node_modules` with an empty dir or volume before installs.
  - `pnpm-lock.yaml` remains authoritative for dependency resolution.
- Runtime checks:
  - `pnpm install` on macOS reuses stored artifacts when available and builds darwin-specific binaries.
  - `pnpm install` in Linux container reuses the store without re-downloading and builds linux artifacts
    without touching host `node_modules`.
  - CI runs `pnpm install` per OS target, ensuring reproducibility.
- Failure handling:
  - Missing or corrupt `.pnpm-store` triggers a clean re-fetch.
  - Overlay misconfiguration is detected (device/inode mismatch) and reported with remediation steps.
  - Permission mismatches surface early with actionable errors.

## Implementation plan

### 1. Repository configuration
- Add or extend `.npmrc` with:
  - `store-dir=.pnpm-store`
  - `virtual-store-dir=node_modules/.pnpm`
  - Additional pnpm flags as needed (e.g., `auto-install-peers=false`).
- Update `.gitignore` to exclude `.pnpm-store`.
- Document pnpm as the required package manager for Node workloads.

### 2. Host tooling
- Ensure host workflows (`make node-install`, CLI helpers) call `pnpm install` at repo root.
- Optionally add a helper target that seeds `.pnpm-store` via `pnpm fetch` for offline use.
- Validate store availability before running installs; prompt users when lockfile changes.

### 3. Container integration
- Modify toolchain launchers so that:
  - Repo mounts at `/workspace`.
  - `/workspace/.pnpm-store` remains shared (no overlay).
  - `/workspace/node_modules` is mounted as tmpfs or a named volume per container.
- Container bootstrap runs `pnpm install` when `node_modules` overlay is empty or the lockfile hash
  differs from a stored sentinel.

### 4. Automation and checks
- Introduce a lightweight preflight (Rust CLI or shell) that verifies:
  - `.pnpm-store` exists or gets created with correct permissions.
  - `node_modules` overlay is active (e.g., via sentinel file, device check, or mount marker).
- Invoke this check in both host and container entrypoints, aborting with clear guidance if it fails.

### 5. Documentation
- Update README/onboarding to cover:
  - Rationale for pnpm (multi-OS correctness, dedupe benefits).
  - Shared store + per-OS `node_modules` pattern.
  - Commands to seed or clean the store and to troubleshoot overlay or permission issues.

### 6. Future enhancements
- Cache `.pnpm-store` via `pnpm fetch` in CI artifacts.
- Add `pnpm store prune` or similar cleanup hooks to `make clean`.
- Emit telemetry/logs for store usage and overlay misconfigurations.

## Acceptance criteria
- pnpm is the canonical Node package manager for this repo.
- Host and container `node_modules` trees are platform-correct and isolated.
- `.pnpm-store` eliminates redundant downloads for pure JS packages across OSes.
- Tooling surfaces misconfiguration with actionable errors.
- Documentation fully describes setup, operation, and troubleshooting.
