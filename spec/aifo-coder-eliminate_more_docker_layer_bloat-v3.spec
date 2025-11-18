# AIFO Coder – Eliminate More Docker Layer Bloat (v3)
# Date: 2025-11-16

Purpose
- Verify and consolidate the v1/v2 plan against the current repository and CI constraints.
- Eliminate unnecessary Docker layers across agent and toolchain images.
- Drop the optional BuildKit variant entirely to avoid complexity.
- Keep shared Dockerfiles Kaniko-safe and preserve runtime behavior and environment semantics.

Non-goals (explicit)
- Do not change runtime behavior or environment mappings (PATH export at runtime remains).
- Do not gate Playwright installs in Aider (keep installed).
- Do not make grcov optional in the Rust toolchain (keep installed and stripped).
- Do not remove GPG runtime prep or entrypoint behavior.

Validated repository state (as of this spec)
- CI uses Kaniko via a reusable GitLab component to build images from shared Dockerfiles.
- Kaniko does not support BuildKit-only extensions (COPY --link, RUN --mount).
- Current Dockerfiles still use BuildKit-only RUN --mount (cache/secret) in multiple places:
  - Dockerfile: rust-builder (cache+secret), shim-builder (cache+secret).
  - toolchains/node/Dockerfile: RUN --mount=type=secret for enterprise CA injection.
  - toolchains/cpp/Dockerfile: RUN --mount=type=cache for apt caches and a secret for CA.
- base and base-slim duplicate wrapper generation (sh/bash/dash, aifo-entrypoint) via nearly identical RUN blocks.
- Derived agent stages repeat ENV PATH="/opt/aifo/bin:${PATH}" even though docker.rs injects PATH at runtime.
- aider/aider-slim add two PATH ENV lines (venv + /opt/aifo/bin).
- plandex/plandex-slim: copy binary then chmod in a separate RUN; binary is not stripped.
- docker.rs sets PATH at container runtime per agent, ensuring shims-first semantics. Adding ENV PATH in images is redundant at runtime but reduces duplicate ENV instructions downstream.

Key constraints and Kaniko-only strategy
- Shared Dockerfiles must be Kaniko-compatible:
  - Use standard COPY (no COPY --link, no COPY --chmod).
  - Avoid RUN --mount (cache/secret) and other BuildKit-only flags.
- Preserve semantics:
  - Enterprise CA handling must not persist artifacts in final images. Without secrets, CA prep is best-effort and should no-op cleanly.
  - GPG runtime prep and entrypoint generation remain identical.
  - PATH export at runtime continues; consolidating ENV PATH in base layers must not change observable behavior.

Detected gaps and inconsistencies (and resolutions)
1) BuildKit-only RUN --mount present in shared Dockerfiles (Kaniko-incompatible)
   - Resolution: Remove RUN --mount everywhere in shared Dockerfiles; rewrite those steps as plain RUN.
     - CA injection becomes best-effort: check for a known CA file path and update system trust only if present; always remove any temporary CA files in the same RUN.
     - apt caching mounts are removed; keep apt usage as-is.

2) Shim duplication across base/base-slim
   - Resolution: Introduce a shim-common stage after shim-builder that:
     - Copies /workspace/out/aifo-shim into /opt/aifo/bin.
     - Generates /opt/aifo/bin/sh, bash, dash wrappers and /usr/local/bin/aifo-entrypoint in one RUN.
     - Base and base-slim stages then reuse via standard COPY from shim-common, removing duplicate RUN content.

3) PATH consolidation in base/base-slim vs runtime PATH export
   - Resolution: Add ENV PATH="/opt/aifo/bin:${PATH}" once in base and base-slim.
     - Remove redundant ENV PATH from derived agent stages (codex/crush/openhands/opencode/plandex and slim variants).
     - In aider/aider-slim, keep ENV PATH="/opt/venv/bin:${PATH}" only; inherit /opt/aifo/bin from base.

4) plandex binary copy and strip
   - Resolution: After COPY, perform chmod 0755 and strip in a single RUN: strip tolerant via "|| true".

5) toolchains/node layering and CA handling
   - Resolution: Merge corepack enable/prepare and deno install into one RUN; preserve best-effort CA add/remove logic without RUN --mount.
     - Optionally strip /usr/local/bin/deno (|| true) within the same RUN.

6) toolchains/cpp layering and verify folding
   - Resolution: Collapse symlink creation (cc/c++ hardlinks and /usr/local/bin symlinks) and HOME prep into one RUN following apt RUN.
     - Fold cmake verify and reinstall-if-missing into the KEEP_APT cleanup RUN to drop the extra verify layer and remove RUN --mount.

7) toolchains/rust consolidation
   - Resolution: Merge apt install, initial cmake verify, and git system config into a single RUN.
     - Keep cargo installs (nextest, grcov) and strip in one RUN.
     - Fold final cmake verification into the KEEP_APT cleanup RUN.

Acceptance criteria
- Layer count reduced across base/base-slim, agent stages, and toolchain images without functional regressions.
- Shared Dockerfiles build successfully under Kaniko in CI.
- All tests pass via “make check”.
- Aider images still include Playwright; Rust toolchain still includes grcov.

Consolidated phased implementation plan (Kaniko-safe only; no BuildKit variant)

Phase 0 – Readiness and audit
- Confirm CI remains Kaniko-based; BuildKit-only features are disallowed in shared Dockerfiles.
- Audit all Dockerfiles and list BuildKit-only usages to remove:
  - Dockerfile: RUN --mount in rust-builder and shim-builder.
  - toolchains/node/Dockerfile: RUN --mount=type=secret (CA).
  - toolchains/cpp/Dockerfile: RUN --mount=type=cache and secret.
- Outcome: Explicit list of lines/blocks to rewrite with plain RUN.

Phase 1 – Remove RUN --mount (cache/secret) and rewrite as plain RUN (Kaniko-safe)
- Dockerfile:
  - rust-builder: replace both RUN blocks that use --mount with plain RUN; set TLS envs and perform steps without secret mounts; ensure any CA handling is best-effort and cleaned up.
  - shim-builder: replace RUN with --mount=cache by plain RUN; keep cargo build and strip; clean up cargo caches conditionally.
- toolchains/node/Dockerfile:
  - Replace both RUN with --mount=type=secret by plain RUN; retain CA add/remove logic as best-effort (check known path; no secret mount).
  - Merge the corepack and deno install steps into a single RUN (Phase 5).
- toolchains/cpp/Dockerfile:
  - Replace RUN with --mount=type=cache by plain RUN; preserve apt install and cleanup.
  - Fold cmake verify into the KEEP_APT cleanup RUN (Phase 6).
- Validation: Build all images with Kaniko; ensure no syntax errors and logic executes cleanly when CA is absent.

Phase 2 – Introduce shim-common stage to deduplicate wrappers and entrypoint
- Add a shim-common stage (after shim-builder) that:
  - Copies /workspace/out/aifo-shim → /opt/aifo/bin/aifo-shim.
  - RUN: chmod 0755; generate /opt/aifo/bin/sh, bash, dash wrappers; symlink aifo-shim to common dev tools; generate /usr/local/bin/aifo-entrypoint.
- In base and base-slim:
  - Replace wrapper/entrypoint RUN generation with standard COPY from shim-common for:
    - /opt/aifo/bin (directory)
    - /usr/local/bin/aifo-entrypoint (file)
- Validation: Ensure wrapper and entrypoint content parity with previous implementation (diff); agent startup behavior unchanged.

Phase 3 – PATH consolidation in base layers and removal downstream
- Add ENV PATH="/opt/aifo/bin:${PATH}" in base and base-slim once (after shim-common COPY).
- Remove redundant ENV PATH lines from agent and slim stages (codex/crush/openhands/opencode/plandex).
- In aider/aider-slim:
  - Keep ENV PATH="/opt/venv/bin:${PATH}" only; remove duplicate /opt/aifo/bin PATH line (inherits from base).
- Validation: docker.rs still sets shims-first PATH at runtime per agent; commands resolve correctly.

Phase 4 – Agent-stage RUN merges and plandex optimization
- plandex/plandex-slim:
  - After COPY, replace chmod-only RUN with a single RUN: chmod 0755 /usr/local/bin/plandex && strip /usr/local/bin/plandex || true.
- For stages that still create small wrappers (e.g., openhands wrapper): merge creation and chmod into the existing install RUN to avoid an extra layer.
- Validation: “plandex --version” returns successfully; openhands wrapper exists and is executable.

Phase 5 – toolchains/node consolidation (Kaniko-safe)
- Merge corepack enable/prepare and deno install into one RUN:
  - Best-effort CA handling (add/remove) within the same RUN; no RUN --mount.
  - strip /usr/local/bin/deno || true at the end.
- Keep HOME/caches prep and PATH env as-is.
- Validation: node/yarn/pnpm/deno basic commands work; cache directories exist and are writable.

Phase 6 – toolchains/cpp consolidation and verify folding (Kaniko-safe)
- Collapse:
  - cc/c++ hardlinks and /usr/local/bin symlinks (cmake/ninja/pkg-config).
  - HOME and ccache directory prep and permissions.
  - Implement these in a single RUN following the apt RUN.
- Fold cmake verify into KEEP_APT cleanup RUN:
  - If /usr/bin/cmake missing, apt-get update; install cmake+cmake-data; then verify.
- Validation: cmake --version succeeds; compilers/tools present; ccache writable.

Phase 7 – toolchains/rust consolidation and verify folding (Kaniko-safe)
- Merge into one RUN:
  - apt install of system dependencies,
  - initial cmake verification,
  - system git config (email/name/default branch) and git-lfs install (best-effort).
- Keep cargo installs (nextest, grcov) and strip in one RUN (already present).
- Fold the final cmake verification into the KEEP_APT cleanup RUN (reinstall if missing, then verify).
- Validation: rustup/cargo/rustc versions print; nextest available; cmake present after cleanup.

Risk assessment and rollback
- Removing RUN --mount: medium risk; ensure all logic uses plain RUN and remains idempotent when CA secrets are absent. Roll back by splitting RUNs if necessary; do not reintroduce BuildKit-only flags.
- shim-common stage: medium risk; parity required; rollback by restoring per-stage RUN generation.
- PATH consolidation: low risk; rollback by re-adding ENV PATH in affected stages if any breakages occur.
- RUN merges and plandex strip: low-to-medium risk; rollback by splitting RUNs or skipping strip if issues arise.

Testing and validation
- Run “make check” locally (cargo nextest).
- Integration suites:
  - make test-integration-suite
  - make test-acceptance-suite
- Toolchain tests:
  - make test-toolchain-rust
  - make test-toolchain-cpp
- Image sanity:
  - Build aider and aider-slim and verify Playwright installation remains intact.
  - Build rust toolchain and verify grcov is present and stripped.

Operational notes
- Keep lines ≤100 chars where possible; prefer 4-space indentation (see CONVENTIONS.md).
- No dead code or unused stages; avoid adding wrappers not reused.
- Prefer exhaustive matches when editing scripts; keep search/replace diffs minimal and unique.
- Kaniko-only path: do not reintroduce BuildKit-only features (COPY --link, RUN --mount).

Acceptance checklist
- Shared Dockerfiles build successfully under Kaniko with fewer layers.
- Duplicate ENV PATH and wrapper-generation RUNs removed; layer count reduced in base/base-slim and agent stages.
- Plandex binary stripped; deno strip best-effort applied.
- Aider retains Playwright; rust toolchain retains grcov without gating.
- All tests pass via “make check”.

How to run tests
- make check
