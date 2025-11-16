# AIFO Coder – Eliminate More Docker Layer Bloat (v1)
# Date: 2025-11-16

Goal
- Reduce per-image layers and cumulative local registry footprint without changing runtime behavior.
- Keep Aider Playwright installs and Rust toolchain grcov as-is (no gating).
- Use BuildKit features like COPY --link where available, while preserving CI (Kaniko) compatibility.

Key constraints and validations
- Do not introduce ARG gating for Playwright in Aider images; Playwright must remain installed.
- Do not make grcov optional in toolchains/rust; it stays installed and stripped.
- COPY --link is a BuildKit-only extension; some builders (e.g., Kaniko) may not support it.
  - To remain CI-compatible, prefer standard COPY in shared Dockerfiles.
  - Introduce an alternate path (or file) for BuildKit-only improvements when needed.
- Maintain existing environment variable mappings and runtime behavior (PATH, GPG runtime prep, entrypoint).
- Avoid merging RUN blocks that would lose necessary cleanup/security steps (CA injection, pinentry config).

High-level approach
1) Consolidate PATH exports once in base and base-slim to remove redundant ENV lines from derived stages.
2) Deduplicate shim wrapper generation via a single “shim-common” stage; reuse via COPY (and COPY --link where supported).
3) Merge adjacent RUN blocks where safe to reduce layers, preserving current semantics.
4) Optimize binary copies with COPY --link (BuildKit) and strip in a single RUN where applicable (plandex, deno).
5) Harmonize toolchain Dockerfiles:
   - toolchains/node: join corepack enable and deno install; add apt cache mounts; preserve CA handling.
   - toolchains/cpp: collapse symlink/home prep into fewer RUNs; integrate cmake verification with cleanup.
   - toolchains/rust: merge early apt/cmake/git config; fold cmake verify into KEEP_APT cleanup.

Builder compatibility strategy
- Primary Dockerfiles remain CI-safe (Kaniko-compatible). Use standard COPY by default.
- Where BuildKit is guaranteed (local buildx, some CI lanes), switch selected COPY to COPY --link.
- If CI fails on COPY --link, keep standard COPY in shared files and optionally offer Dockerfile.buildkit for local builds.

Detected gaps and inconsistencies (and resolutions)
- Duplicate PATH ENV across derived stages:
  - Resolution: Add ENV PATH="/opt/aifo/bin:${PATH}" in base and base-slim; remove duplicates downstream.
- Shim wrappers duplicated in base and base-slim via identical RUN blocks:
  - Resolution: Create shim-common stage post shim-builder; generate wrappers once; COPY into base/base-slim.
- Agent stages repeat ENV PATH:
  - Resolution: Remove redundant ENV PATH where base now provides it.
- plandex binary copied then chmod in separate RUN:
  - Resolution: COPY then chmod+strip in one RUN (strip is small but helpful).
- toolchains/node disjoint RUNs for corepack and deno:
  - Resolution: Merge into one RUN, preserve CA add/remove logic; add apt cache mounts.
- toolchains/cpp multiple small RUNs:
  - Resolution: Collapse symlink and home prep; fold cmake verification into cleanup.
- toolchains/rust: separate cmake verify and git config RUNs:
  - Resolution: Merge apt + cmake verify + git config; fold final verification into cleanup RUN.

Phased implementation plan
Phase 0 – Preparation and compatibility notes
- Confirm local builds use Docker Buildx/BuildKit (Makefile already prefers buildx).
- Confirm CI uses Kaniko; verify Kaniko behavior with COPY --link in a test branch.
- Outcome: If Kaniko rejects COPY --link, keep shared Dockerfiles on plain COPY; offer an optional Dockerfile.buildkit.

Phase 1 – PATH consolidation (Dockerfile: base and base-slim; derived stages)
- Add ENV PATH="/opt/aifo/bin:${PATH}" in base and base-slim once.
- Remove duplicated ENV PATH lines from codex/crush/aider/openhands/opencode/plandex and slim variants.
- Validation: Run unit/integration tests; confirm shim PATH tools are accessible across all agents.

Phase 2 – Create shim-common stage (Dockerfile)
- New stage after shim-builder:
  - COPY aifo-shim into /opt/aifo/bin via COPY (and COPY --link in BuildKit-variant).
  - Generate /opt/aifo/bin/sh, bash, dash and /usr/local/bin/aifo-entrypoint in one RUN.
- In base and base-slim, replace the current wrapper-generation RUNs with COPY from shim-common.
- Risk: Ensure identical content to prior wrappers; verify entrypoint behavior matches.
- Validation: “shim embed” test and agent startup paths.

Phase 3 – Merge adjacent RUN blocks (Dockerfile agent stages)
- Where safe, merge wrapper creation and entrypoint generation into one RUN (or fully reuse shim-common).
- Remove per-stage PATH envs (rely on base/base-slim).
- Keep CA injection/remove steps within merged RUNs to avoid residue.

Phase 4 – plandex binary optimization (Dockerfile: plandex/plandex-slim)
- Use COPY (COPY --link where available) from plandex-builder to /usr/local/bin/plandex.
- Apply chmod 0755 and strip in one RUN (strip tolerant: “|| true”).
- Validation: run “plandex --version”; ensure executable remains intact.

Phase 5 – toolchains/node (toolchains/node/Dockerfile)
- Merge corepack enable/prepare and deno install into a single RUN.
- Add apt cache mounts to the initial apt RUN to speed rebuilds:
  - --mount=type=cache,target=/var/cache/apt
  - --mount=type=cache,target=/var/lib/apt/lists
- Optionally strip /usr/local/bin/deno if present (|| true).
- Validation: node/yarn/pnpm/deno basic commands; confirm HOME/caches directories exist.

Phase 6 – toolchains/cpp (toolchains/cpp/Dockerfile)
- Collapse symlink steps (cc/c++ and cmake/ninja/pkg-config) and home prep into one RUN following apt RUN.
- Integrate cmake verification into the KEEP_APT cleanup RUN; reinstall if missing, then verify.
- Validation: cmake --version, clang/gcc presence; ccache path exists and writable.

Phase 7 – toolchains/rust (toolchains/rust/Dockerfile)
- Merge apt install, cmake verification, and system git config into one RUN.
- Keep grcov and cargo-nextest installation; strip binaries; clean cargo caches as before.
- Fold post-cleanup cmake verification into the KEEP_APT cleanup RUN.
- Validation: rustup/cargo/rustc versions, nextest available; cmake presence post-cleanup.

Phase 8 – CI and BuildKit differences
- If Kaniko rejects COPY --link:
  - Keep shared Dockerfiles on standard COPY.
  - Optionally add Dockerfile.buildkit with COPY --link for local buildx users.
- Ensure .gitlab-ci.yml continues using shared Dockerfiles (Kaniko path).
- Validation: Pipeline builds for all stages; local buildx sanity builds for the BuildKit variant.

Risk assessment and rollback
- PATH consolidation: low risk; if an agent breaks, re-add ENV PATH in that stage.
- shim-common: medium risk; ensure content parity; rollback by restoring per-stage RUN scripts.
- RUN merges: low-to-medium; ensure CA removal remains; rollback by splitting RUNs.
- COPY --link: builder-dependent; if CI breaks, use plain COPY.

Testing strategy
- Run “make check” locally (cargo nextest) after each phase.
- Run integration suites:
  - make test-integration-suite
  - make test-acceptance-suite
- For toolchain images, run specific tests:
  - make test-toolchain-rust
  - make test-toolchain-cpp
- Validate Docker images build for codex/crush/aider/openhands/opencode/plandex and slim variants.

Operational notes
- Keep lines ≤100 chars where possible; prefer 4-space indent (see CONVENTIONS.md).
- No dead code; avoid adding unused stages or wrappers.
- Prefer exhaustive matches in code changes; ensure search/replace blocks uniquely match.

Out-of-scope for v1
- Changing Playwright behavior or gating it behind ARGs.
- Making grcov optional in the Rust toolchain.
- Altering agent functionality or environment mappings (AIFO_* → OpenAI/Azure).

Acceptance criteria
- Fewer layers across agent and toolchain images without functional regressions.
- Successful CI builds (Kaniko) with unchanged shared Dockerfiles.
- Optional BuildKit variant demonstrates COPY --link benefits in local builds.
- All tests pass via “make check”.

How to run tests (post-implementation)
- make check
