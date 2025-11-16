# AIFO Coder – Eliminate More Docker Layer Bloat (v2)
# Date: 2025-11-16

Purpose
- Validate the v1 plan against the current repository state and CI constraints.
- Correct gaps and inconsistencies found during review.
- Consolidate into a single, comprehensive, Kaniko-safe implementation spec with an optional BuildKit variant.

Non-goals (explicit)
- Do not change runtime behavior or environment mappings (PATH export at runtime remains).
- Do not gate Playwright installs in Aider (keep existing behavior; WITH_PLAYWRIGHT stays default=1).
- Do not make grcov optional in the Rust toolchain (keep installed and stripped).
- Do not remove GPG runtime prep or entrypoint behavior.

Validated repository state (as of this spec)
- CI uses Kaniko (via GitLab component) for shared Dockerfiles; Kaniko does not support COPY --link nor RUN --mount cache.
- Local builds can use Buildx/BuildKit (Makefile prefers buildx when available).
- Dockerfile observations:
  - base and base-slim duplicate wrapper generation (sh/bash/dash) and aifo-entrypoint creation via nearly identical RUN blocks.
  - Derived agent stages (codex/crush/opencode/openhands and their slim variants) add ENV PATH="/opt/aifo/bin:${PATH}" redundantly.
  - aider/aider-slim add two PATH ENV lines: first for venv, then for /opt/aifo/bin.
  - plandex/plandex-slim copy the binary and chmod in two instructions and do not strip it.
- toolchains:
  - node: separate RUNs for corepack enable/prepare and deno install; no apt cache mounts (BuildKit-only feature).
  - cpp: multiple small RUN steps for symlinks, HOME prep, cmake verify; verify done in a separate final RUN.
  - rust: multiple RUNs that can be merged (apt + cmake verify + git config), plus a separate verify after optional cleanup.
- Launcher (docker.rs) exports PATH at container runtime depending on agent, already ensuring shims-first semantics. Adding an ENV PATH in images is compatible and redundant at runtime but still reduces repetitive ENV in Dockerfiles.

Key constraints and compatibility strategy
- Keep shared Dockerfiles Kaniko-compatible:
  - Use standard COPY (no COPY --link).
  - Avoid RUN --mount cache and other BuildKit-only flags.
- Provide an optional BuildKit-focused variant for local builds:
  - Either via a supplemental Dockerfile.buildkit or stage-specific overrides.
  - In that variant, use COPY --link and, where helpful, COPY --chmod and RUN --mount cache.
- Preserve current semantics:
  - Enterprise CA is injected via BuildKit secret and removed in the same RUN (no residue).
  - GPG runtime prep and entrypoint generation stay identical.
  - PATH exports at runtime remain; consolidating ENV PATH in base layers must not change observed behavior.

Detected gaps in v1 and resolutions in v2
1) Playwright/grcov toggles
   - Gap: v1 discussed gating as a general optimization but constraints forbid it.
   - Resolution: Keep existing Aider WITH_PLAYWRIGHT ARG defaulting to 1; do not add new gating. Keep grcov installed in rust toolchain and stripped.

2) Shim duplication and consistency
   - Gap: v1 suggested a “shim-common” stage but did not detail exact integration points.
   - Resolution: Add a shim-common stage right after shim-builder that:
     - Installs /opt/aifo/bin/aifo-shim (from shim-builder).
     - Generates /opt/aifo/bin/sh, bash, dash wrappers and /usr/local/bin/aifo-entrypoint in a single RUN.
     - Ensures content parity with current base/base-slim RUN blocks (verbatim script lines).
     - Then base and base-slim replace their wrapper/entrypoint RUNs with COPY from shim-common.
     - BuildKit variant: use COPY --link for these copies; shared Dockerfile: standard COPY.

3) PATH consolidation vs runtime PATH export
   - Gap: v1 says “add ENV PATH in base/base-slim” but did not reconcile with docker.rs PATH injection.
   - Validation: docker.rs exports PATH per-agent at runtime; adding ENV PATH in images is redundant but harmless.
   - Resolution: Add ENV PATH="/opt/aifo/bin:${PATH}" once in base and base-slim; remove redundant ENV PATH from derived stages.
     - Keep aider’s venv PATH ENV (ENV PATH="/opt/venv/bin:${PATH}"); remove its second ENV PATH for /opt/aifo/bin (inherits from base).
     - No behavior change because runtime export still sets PATH before agent exec.

4) Node toolchain RUN merges and cache mounts
   - Gap: v1 proposed merging corepack and deno install and adding apt cache mounts; Kaniko cannot use RUN --mount cache.
   - Resolution: In shared toolchains/node/Dockerfile:
     - Merge the corepack enable/prepare RUN with the deno install RUN, preserving enterprise CA add/remove.
     - No RUN --mount cache in shared Dockerfile.
     - Provide BuildKit variant using RUN --mount=type=cache for apt on the initial apt-get RUN to speed local rebuilds.

5) C/CPP toolchain RUN consolidation and cmake verify
   - Gap: v1 suggested folding symlinks and HOME prep and integrating cmake verify into cleanup.
   - Resolution: Collapse symlink creation (cc/c++ hardlinks and cmake/ninja/pkg-config symlinks) and HOME prep into a single RUN after the apt RUN.
     - Fold the “verify cmake, reinstall if missing” into the existing KEEP_APT cleanup RUN to eliminate an extra layer.
     - Preserve current semantics and error handling.

6) Rust toolchain RUN consolidation
   - Gap: v1 suggested merging apt + cmake verify + git config and folding final verify into cleanup; provide precise steps.
   - Resolution: Merge apt install + initial cmake verify + git system config into a single RUN.
     - Keep grcov and cargo-nextest install in one RUN (already present).
     - Fold the final “verify cmake after optional cleanup” into the KEEP_APT cleanup RUN.
     - Maintain CA handling and stripping of binaries.

7) Plandex binary copy and strip
   - Gap: v1 called for chmod+strip in one RUN; Dockerfile currently only chmods.
   - Resolution: After COPY of /usr/local/bin/plandex, perform chmod 0755 and strip in a single RUN (strip tolerant via || true).
     - BuildKit variant: COPY --link from builder.

8) Optional BuildKit variant
   - Gap: v1 said “optional Dockerfile.buildkit” but did not list concrete COPY --link hotspots.
   - Resolution: Provide a concise list of COPY --link candidates for that variant:
     - shim-common → base/base-slim paths
     - aider-builder venv → aider/aider-slim
     - plandex-builder binary → plandex/plandex-slim
     - Add COPY --chmod where helpful (e.g., /usr/local/bin/openhands) and RUN --mount caches for apt where it improves rebuilds.

Acceptance criteria (v2)
- Fewer total layers in agent and toolchain images without functional regressions.
- Shared Dockerfiles remain CI-safe (Kaniko compatible).
- Optional BuildKit variant demonstrates additional local storage/rebuild improvements (COPY --link, cache mounts).
- All tests pass via “make check”; toolchain tests continue to pass; Aider images still include Playwright; Rust toolchain still includes grcov.

Consolidated phased implementation plan

Phase 0 – Readiness and CI compatibility
- Confirm local usage of Buildx/BuildKit (Makefile already prefers buildx).
- Confirm CI uses Kaniko; shared Dockerfiles must remain free of COPY --link and RUN --mount cache.
- Outcome: Prepare to keep shared Dockerfiles Kaniko-safe; create an optional Dockerfile.buildkit after Phases 1–7.

Phase 1 – PATH consolidation in base layers and cleanup downstream
- In Dockerfile:
  - base and base-slim: add ENV PATH="/opt/aifo/bin:${PATH}" once after wrapper/entrypoint integration (Phase 2).
  - Remove ENV PATH="/opt/aifo/bin:${PATH}" from derived stages: codex, crush, openhands, opencode, plandex and their slim variants.
  - aider/aider-slim: keep ENV PATH="/opt/venv/bin:${PATH}" only; remove the second /opt/aifo/bin PATH line (inherited from base).
- Validation: runtime PATH from docker.rs still sets agent-specific PATH before exec; commands resolve correctly.

Phase 2 – Introduce shim-common to deduplicate wrapper/entrypoint layers
- Add a new stage in Dockerfile after shim-builder:
  - COPY from shim-builder /workspace/out/aifo-shim to /opt/aifo/bin/aifo-shim.
  - RUN: chmod 0755 aifo-shim; generate /opt/aifo/bin/sh (with existing logic), derive bash/dash scripts, symlink aifo-shim to common dev tools, and generate /usr/local/bin/aifo-entrypoint (preserve exact content).
- Update base and base-slim stages:
  - Replace their wrapper/entrypoint RUN blocks with:
    - install -d /opt/aifo/bin (if not already present),
    - COPY from shim-common for /opt/aifo/bin and /usr/local/bin/aifo-entrypoint.
- BuildKit variant:
  - Use COPY --link for both copies; optionally COPY --chmod=0755 for entrypoint.
- Risk: Script parity must be exact; verify by diffing generated content versus prior.

Phase 3 – Merge adjacent RUNs in agent stages where safe
- Ensure CA add/remove and cleanup steps remain in the same RUN sequences as before.
- Typical merges:
  - In stages that still have a dedicated chmod after COPY for small files (e.g., openhands wrapper), prefer generating and chmod within a single RUN (already mostly done).
- Remove redundant ENV PATH in derived stages as per Phase 1.

Phase 4 – plandex/plandex-slim binary optimization
- Replace separate COPY + chmod with COPY then a single RUN: chmod 0755 /usr/local/bin/plandex && strip /usr/local/bin/plandex || true.
- BuildKit variant: use COPY --link from plandex-builder.

Phase 5 – toolchains/node consolidation
- Merge the corepack enable/prepare RUN with the deno install RUN into one RUN while preserving enterprise CA injection/removal logic.
- Do not add RUN --mount cache in the shared Dockerfile.
- BuildKit variant: add apt cache mounts to the initial apt RUN and consider strip /usr/local/bin/deno || true.
- Validate: node/yarn/pnpm/deno basic commands; HOME/caches exist and are writable.

Phase 6 – toolchains/cpp consolidation and verify folding
- Collapse:
  - cc/c++ hardlinks and /usr/local/bin symlinks (cmake/ninja/pkg-config) and HOME/ccache prep into one RUN following the apt RUN.
- Fold the final cmake “verify and reinstall if missing” into the existing KEEP_APT cleanup RUN to drop the extra layer.
- Validate: cmake --version, compilers and tools present; ccache directory writable.

Phase 7 – toolchains/rust consolidation and verify folding
- Merge early steps into one RUN: apt install (system deps) + initial cmake verify + system git config (email/name/default branch + LFS install).
- Keep the cargo installs (nextest, grcov) and strip in the existing RUN.
- Fold the final cmake verification into the KEEP_APT cleanup RUN (reinstall if missing, then verify).
- Validate: rustup/cargo/rustc version prints; nextest present; cmake present after cleanup.

Phase 8 – Optional BuildKit variant (local optimization)
- Add Dockerfile.buildkit:
  - Mirrors the shared Dockerfile but:
    - Uses COPY --link for: shim-common → base/base-slim, aider-builder venv → aider/aider-slim, plandex-builder → plandex/plandex-slim.
    - Uses COPY --chmod where appropriate (e.g., entrypoint, openhands).
    - Adds RUN --mount cache for apt in heavy apt-get RUNs (toolchains and early base installs).
- Keep .gitlab-ci.yml unchanged (still uses shared Dockerfiles). Document local use of Dockerfile.buildkit in README/CONTRIBUTING if desired.

Risk assessment and rollback
- PATH consolidation: low risk; rollback by re-adding ENV PATH in affected stages.
- shim-common: medium risk; if differences break startup, rollback by restoring per-stage wrapper RUNs.
- RUN merges: low-to-medium; ensure CA removal remains intact; split RUNs again if regression occurs.
- COPY --link: builder-dependent; only used in optional Dockerfile.buildkit.

Testing and validation
- Run make check locally; CI runs lint + tests with cargo nextest.
- Integration suites:
  - make test-integration-suite
  - make test-acceptance-suite
- Toolchain tests:
  - make test-toolchain-rust
  - make test-toolchain-cpp
- Image sanity:
  - Build at least aider and aider-slim and verify Playwright install remains intact.
  - Build rust toolchain and verify grcov present and stripped.

Operational notes
- Keep lines ≤100 chars where possible; use 4-space indentation (see CONVENTIONS.md).
- No dead code or unused stages.
- Prefer exhaustive matches when editing scripts or spec lines; keep search/replace blocks minimal and unique.

Acceptance checklist (what must be true at the end)
- Shared Dockerfiles build successfully under Kaniko.
- Layer count reduced in base/base-slim and agent stages by removing duplicate ENV and RUNs.
- Plandex binary is stripped; optional deno strip applied in BuildKit variant.
- Aider images still include Playwright; rust toolchain includes grcov; no regression in behavior.
- Optional BuildKit variant exists and demonstrates additional local benefits.
