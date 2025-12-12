# pnpm support plan (aifo-coder-pnpm_plan-v2)

## Verification of v1 plan
- Goals, constraints, and desired behaviors remain correct; per-OS `node_modules` plus shared store is
  still the target architecture.
- No conflicts with repository conventions (line length, tooling) were found.
- Missing pieces detected: detailed overlay validation, permission strategy, pnpm enforcement,
  CI/cache integration, telemetry/troubleshooting guidance, and phased rollout milestones.

## Consistency review and gap analysis
1. **Configuration propagation** — v1 omitted how `.npmrc` is consumed by containers/CI; env defaults
   must be explicitly set.
2. **Overlay validation** — detection of shared `node_modules` lacked concrete checks (inode/device or
   sentinel); remediation steps not specified.
3. **Lockfile enforcement** — npm/yarn users could still alter `node_modules`; no guardrails existed.
4. **Permissions / UID alignment** — shared `.pnpm-store` on bind mounts needs ownership guidance for
   rootless containers; absent previously.
5. **CI caching** — no plan for cache keys, invalidation, or `pnpm fetch`.
6. **Observability & troubleshooting** — no structured logging, cleanup, or runbook.
7. **Rollout plan** — steps were unordered with no validation gates.

## Corrections & clarifications
- Document directory layout, required `.npmrc` options, env vars (`PNPM_STORE_PATH`, etc.).
- Define overlay guard (device/inode comparison plus sentinel file) and remediation (mount override).
- Enforce pnpm usage via helper scripts/preflight; detect `package-lock.json`/`yarn.lock` edits.
- Outline permission expectations and auto-fix guidance.
- Provide CI cache instructions keyed by OS + `pnpm-lock.yaml` hash.
- Add telemetry hooks for store/overlay failures.
- Present a phased implementation plan with success criteria.

## Final specification

### Objectives
- Ensure platform-correct `node_modules` for macOS host and Linux container while deduping downloads via
  pnpm’s content-addressable store.
- Make pnpm the canonical package manager across host, container, and CI workflows.
- Provide deterministic automation, validation, and documentation.

### Functional requirements
1. `.pnpm-store` lives at `<repo>/.pnpm-store`, shared read/write across host/container/CI.
2. macOS host keeps local `node_modules`; container overlays `/workspace/node_modules` with tmpfs or
   named volume so artifacts stay per-OS.
3. Tooling (`aifo-coder` launcher, Makefile targets, CI jobs) always runs `pnpm install/fetch` using repo
   `.npmrc` and `--frozen-lockfile`.
4. Overlay guard aborts if container sees host `node_modules` (missing sentinel or identical device/inode).
5. Lockfile integrity enforced; npm/yarn installs emit warnings or errors.
6. CI uses pnpm with cache reuse and runs overlay/store checks.

### Non-functional requirements
- Keep committed lines ≤100 chars when practical.
- Work offline once artifacts cached.
- Avoid requiring root; when root modifies store, ownership reset to invoking user.
- Emit actionable errors with remediation tips.

### Directory & environment layout
- `.npmrc` entries:
  - `store-dir=.pnpm-store`
  - `virtual-store-dir=node_modules/.pnpm`
  - `prefer-frozen-lockfile=true`
  - Optional: `auto-install-peers=false`
- Tooling sets:
  - `PNPM_STORE_PATH=$REPO/.pnpm-store`
  - `PNPM_HOME` (if pnpm installed locally)
  - `AIFO_NODE_OVERLAY_SENTINEL=/workspace/node_modules/.aifo-node-overlay`
- Host installs drop sentinel file (same path) to verify overlay detection logic.

### Failure handling
- Missing store → create with `0o775`, log action, advise rerun of `pnpm install`.
- Overlay missing → detect via `stat` device/inode or absent sentinel; abort with steps to mount volume.
- Lockfile drift → fail fast if npm/yarn touched `node_modules`; suggest `pnpm install`.
- Store corruption → document `pnpm store prune` and `rm -rf .pnpm-store && pnpm fetch`.
- Permission mismatch → guide `chown -R $(id -u):$(id -g) .pnpm-store`; optionally auto-fix when possible.

### Observability & docs
- Extend README/onboarding with workflow diagrams, troubleshooting, and offline/cleanup instructions.
- Capture overlay/store failures via existing logging/telemetry hooks (include sentinel path and advice).
- Add FAQ covering permissions, overlay errors, store cleanup, offline use.

## Phased implementation plan

### Phase 0 – Preparation
- Audit existing Node workflows, scripts, and CI lanes.
- Pick pnpm baseline (>=9.x) and document installation.
- Decide UID/GID strategy for shared store (e.g., match host user IDs in containers).

### Phase 1 – Repository configuration
- Add `.npmrc` with required settings; ensure `.pnpm-store` ignored in VCS.
- Update documentation (README, onboarding) to declare pnpm canonical.
- Add guard script or pre-commit check warning on `npm install` / `yarn install`.

### Phase 2 – Host workflow enablement
- Implement `make node-install` (runs preflight + `pnpm install --frozen-lockfile`).
- Update `aifo-coder` launcher/tooling commands to invoke helper before Node tasks.
- Ensure host sentinel creation + permission checks.

### Phase 3 – Container/runtime integration
- Modify orchestrator to:
  - Mount repo at `/workspace`
  - Mount dedicated volume/tmpfs at `/workspace/node_modules`
  - Share `.pnpm-store` bind mount
- Add preflight binary/script verifying overlay (sentinel + device/inode) before running installs.
- Auto-run `pnpm install` in container when overlay empty or lock hash changed.

### Phase 4 – CI & caching
- Configure CI jobs to run `pnpm fetch` then `pnpm install --offline --frozen-lockfile`.
- Cache `.pnpm-store` keyed by `pnpm-lock.yaml` + OS; add prune step for stale caches.
- Introduce telemetry/logging around store reuse, overlay failures.

### Phase 5 – Documentation & enablement
- Publish updated docs, troubleshooting guide, and FAQ.
- Provide migration steps for contributors (npm → pnpm).
- Offer sample commands for cleaning store, resetting overlays, running offline.

### Phase 6 – Validation & rollout completion
- Run macOS + Linux integration tests covering overlay guard, permission reset, CI cache hits.
- Verify CI pipelines pass with pnpm-only workflow.
- Collect feedback, address issues, and make pnpm mandatory by disabling npm/yarn scripts.

## Validation & test plan
- Unit tests for preflight utility (sentinel detection, error messages).
- Integration tests simulating macOS host + Linux container workflows.
- CI gate verifying `pnpm install` both host/container paths, ensuring no cross-OS artifacts.

## Acceptance criteria
- pnpm enforced repo-wide; `pnpm-lock.yaml` authoritative source of truth.
- `.pnpm-store` shared successfully; per-OS `node_modules` confirmed via guard.
- Automation prevents misconfiguration and supplies remediation instructions.
- CI uses pnpm caches and reports zero overlay violations.
- Documentation updated; contributors can follow workflow without additional support.
