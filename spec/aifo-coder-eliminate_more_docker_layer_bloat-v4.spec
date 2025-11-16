# AIFO Coder – Eliminate More Docker Layer Bloat (v4)
# Date: 2025-11-16

Purpose
- Consolidate and validate v3 while explicitly retaining BuildKit-style RUN --mount for secrets and caches.
- Reduce unnecessary Docker layers across agent and toolchain images without changing runtime behavior.
- Keep shared Dockerfiles Kaniko-safe; rely on Kaniko’s --use-new-run support for RUN --mount.
- Preserve environment semantics, Playwright in Aider, and grcov in Rust toolchain.

Non-goals (explicit)
- Do not change runtime behavior or environment mappings (docker.rs PATH export remains).
- Do not gate Playwright installs in Aider (keep installed; no ARG gating).
- Do not make grcov optional in Rust toolchain (keep installed and stripped).
- Do not remove GPG runtime prep or entrypoint behavior.
- Do not introduce BuildKit-only COPY flags (COPY --link, COPY --chmod) in shared Dockerfiles.

Validated repository state
- CI uses Kaniko via a reusable GitLab component to build images from shared Dockerfiles.
- Kaniko is invoked with --use-new-run, enabling a subset of BuildKit RUN semantics:
  - RUN --mount=type=secret (supported)
  - RUN --mount=type=cache (supported)
- COPY --link and COPY --chmod remain unsupported in Kaniko; current Dockerfiles do not use them.
- Dockerfiles currently:
  - Use RUN --mount in multiple places (rust-builder/shim-builder, toolchains/node, toolchains/cpp).
  - Duplicate wrapper/entrypoint generation in base/base-slim.
  - Repeat ENV PATH="/opt/aifo/bin:${PATH}" in derived agent stages; aider adds two PATH lines.
  - Copy plandex binary then chmod in a separate RUN; not stripped.

Key constraints and strategy
- Keep RUN --mount=type=secret and --mount=type=cache where they speed builds (CA injection, apt cache, cargo cache).
- Kaniko compatibility:
  - Continue to avoid COPY --link and COPY --chmod in shared Dockerfiles.
  - Ensure CA injection via secrets is best-effort and cleaned up in the same RUN.
- Preserve semantics:
  - docker.rs sets PATH at container runtime; consolidating ENV PATH in base must not change behavior.
  - Keep Playwright and grcov installed; strip binaries where applicable.

Detected gaps, inconsistencies and resolutions
1) BuildKit feature coverage
   - Gap: v3 spec recommended removing RUN --mount for Kaniko; this contradicts actual CI behavior with --use-new-run.
   - Resolution: Explicitly retain RUN --mount=type=secret and --mount=type=cache where present and helpful; document reliance on --use-new-run in CI.

2) Shim duplication across base/base-slim
   - Gap: Identical RUN blocks generate sh/bash/dash wrappers and entrypoint twice.
   - Resolution: Introduce a shim-common stage that performs wrapper/entrypoint generation once, then COPY into base/base-slim via standard COPY.

3) PATH consolidation in base layers
   - Gap: Derived stages redundantly set ENV PATH="/opt/aifo/bin:${PATH}". Aider adds two PATH lines.
   - Resolution: Add ENV PATH="/opt/aifo/bin:${PATH}" once in base and base-slim; remove duplicate PATH env in derived stages.
     - In aider/aider-slim keep only ENV PATH="/opt/venv/bin:${PATH}"; inherit /opt/aifo/bin from base.

4) Plandex binary handling
   - Gap: COPY followed by chmod in separate RUN; not stripped.
   - Resolution: Keep COPY; merge chmod+strip in a single RUN (strip tolerant: || true).

5) Toolchains/node layering and CA handling
   - Gap: corepack enable/prepare and deno install split into two RUNs; both use secrets.
   - Resolution: Merge into a single RUN to reduce layers; keep best-effort CA injection/removal inside the merged RUN; strip deno if present (|| true).

6) Toolchains/cpp layering and verify folding
   - Gap: Multiple small RUNs for symlinks/home prep; verify uses RUN --mount caches separately.
   - Resolution: Collapse symlink creation and HOME/ccache prep into one RUN after base apt RUN.
     - Fold cmake verify/reinstall-if-missing into the KEEP_APT cleanup RUN to drop an extra layer.
     - Continue to use RUN --mount caches where helpful for apt.

7) Toolchains/rust consolidation
   - Gap: Separate RUNs for apt + cmake verify + git config; final verify adds layers.
   - Resolution: Merge apt install, initial cmake verify, and system git config into one RUN.
     - Keep cargo installs (nextest, grcov) in one RUN and strip.
     - Fold final cmake verification into the KEEP_APT cleanup RUN.

Acceptance criteria
- Fewer layers across base/base-slim, agent stages, and toolchain images without functional regressions.
- Shared Dockerfiles build successfully under Kaniko in CI using --use-new-run.
- All tests pass via “make check”.
- Playwright remains installed in Aider; grcov remains installed/stripped in Rust toolchain.

Holistic phased implementation plan (Kaniko-safe; keep RUN --mount; avoid COPY --link)

Phase 0 – Readiness and explicit CI dependency
- Confirm Kaniko path uses --use-new-run (documented in .gitlab-ci.yml via component and before_script).
- Audit Dockerfiles for BuildKit usage:
  - RUN --mount (secrets/caches): keep.
  - COPY --link/--chmod: avoid (unsupported by Kaniko).
- Outcome: Affirm that current RUN --mount usages are acceptable and should be preserved.

Phase 1 – Introduce shim-common stage to deduplicate wrappers/entrypoint
- Add shim-common after shim-builder:
  - COPY /workspace/out/aifo-shim → /opt/aifo/bin/aifo-shim.
  - RUN: chmod 0755 aifo-shim; generate /opt/aifo/bin/sh (as in base), derive bash/dash via sed; create tool symlinks; generate /usr/local/bin/aifo-entrypoint.
- In base and base-slim:
  - Replace current wrapper/entrypoint RUN blocks with COPY from shim-common:
    - /opt/aifo/bin (directory) and /usr/local/bin/aifo-entrypoint (file).
- Validation: Byte-for-byte parity of generated wrapper scripts and entrypoint with prior implementation.

Phase 2 – PATH consolidation in base/base-slim; cleanup downstream
- In base and base-slim:
  - Add ENV PATH="/opt/aifo/bin:${PATH}" after shim-common COPY.
- In derived agent stages:
  - Remove redundant ENV PATH="/opt/aifo/bin:${PATH}" lines (codex/crush/openhands/opencode/plandex and slim variants).
- In aider/aider-slim:
  - Keep only ENV PATH="/opt/venv/bin:${PATH}" (inherit /opt/aifo/bin from base).
- Validation: docker.rs still sets shims-first PATH at runtime; commands resolve correctly.

Phase 3 – Agent-stage RUN merges and CA handling
- Merge small adjacent RUNs where safe:
  - For wrapper creation or chmod of small files (e.g., openhands wrapper), perform creation+chmod within a single RUN.
- Keep enterprise CA injection/removal within the same RUN blocks; continue to use RUN --mount=type=secret for CA.
- Validation: Build images; verify agent startup and wrapper behavior.

Phase 4 – Plandex binary optimization
- In plandex/plandex-slim:
  - After COPY /out/plandex → /usr/local/bin/plandex, replace chmod-only RUN with:
    - RUN chmod 0755 /usr/local/bin/plandex && strip /usr/local/bin/plandex || true
- Validation: plandex --version succeeds; binary executable.

Phase 5 – toolchains/node consolidation (retain RUN --mount secrets)
- Merge corepack enable/prepare and deno install into one RUN:
  - Keep RUN --mount=type=secret,id=migros_root_ca to inject CA best-effort; set SSL_CERT_FILE, CURL_CA_BUNDLE, NODE_EXTRA_CA_CERTS/NODE_OPTIONS where applicable; remove CA afterwards.
  - Strip /usr/local/bin/deno || true at end.
- Keep HOME/caches prep and PATH env as-is; apt caches may be retained via RUN --mount=type=cache on apt steps if added later.
- Validation: node/yarn/pnpm/deno commands work; caches exist and are writable.

Phase 6 – toolchains/cpp consolidation (retain RUN --mount caches)
- Collapse symlink creation (cc/c++ hardlinks; /usr/local/bin symlinks) and HOME/ccache prep into one RUN following apt RUN.
- KEEP_APT cleanup RUN:
  - Fold cmake verify and reinstall-if-missing into the same RUN (and optionally retain apt cache mounts).
- Validation: cmake --version; compilers/tools present; ccache writable.

Phase 7 – toolchains/rust consolidation and verify folding
- Merge into one RUN:
  - apt install system deps; initial cmake verify; system git config (email/name/default branch); git-lfs install (best-effort).
- Keep cargo installs (nextest, grcov) and strip together; purge cargo caches.
- KEEP_APT cleanup RUN:
  - Fold final cmake verification; reinstall cmake/cmake-data if missing; verify; remove apt artifacts.
- Validation: rustup/cargo/rustc versions print; nextest present; cmake present after cleanup.

Phase 8 – Documentation and guardrails
- Add comments in Dockerfiles where RUN --mount is used explaining reliance on Kaniko --use-new-run and best-effort CA handling.
- Note in README/CONTRIBUTING that local builds should prefer buildx/BuildKit; classic docker build without BuildKit may fail on RUN --mount.

Risk assessment and rollback
- RUN --mount reliance: low-to-medium risk if CI component changes; rollback by rewriting to plain RUN with best-effort CA and removing mounts (only if CI stops supporting --use-new-run).
- shim-common introduction: medium risk; rollback by restoring per-stage wrapper generation RUN blocks.
- PATH consolidation: low risk; rollback by re-adding per-stage ENV PATH lines if needed.
- RUN merges and strip: low-to-medium risk; rollback by splitting RUNs or skipping strip.

Testing and validation
- Run “make check” locally (cargo nextest).
- Integration/acceptance:
  - make test-integration-suite
  - make test-acceptance-suite
- Toolchain tests:
  - make test-toolchain-rust
  - make test-toolchain-cpp
- Image sanity:
  - Build aider/aider-slim; verify Playwright install intact.
  - Build rust toolchain; verify grcov present and stripped.

Operational notes
- Keep lines ≤100 chars; prefer 4-space indentation (see CONVENTIONS.md).
- No dead code or unused stages; wrapper content must match previous behavior.
- Prefer minimal, unique diffs in scripts; ensure best-effort CA removal in same RUN.

Acceptance checklist
- Shared Dockerfiles build successfully under Kaniko (with --use-new-run).
- Duplicate ENV PATH and wrapper-generation RUNs removed; layer count reduced in base/base-slim and agents.
- Plandex binary stripped; deno strip best-effort applied.
- Aider retains Playwright; rust toolchain retains grcov; no behavior changes.
- All tests pass via “make check”.

How to run tests
- make check
