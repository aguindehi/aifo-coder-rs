Spec: Drop target caching in CI and adopt sccache for compiled artifacts
Version: v1
Date: 2025-11-17

Motivation
- CI test jobs currently cache both .cargo and target, resulting in tens of thousands of files
  being uploaded at job end. This inflates wall time and bandwidth without proportional benefit.
- We want to keep corporate CA injection and existing secret/cache mounts intact, preserve existing
  test structures and behavior, and reduce CI overhead by switching compiled-artifact caching to
  sccache, which is smaller and more effective than caching target.

Goals
- Remove target from GitLab cache in CI test jobs.
- Narrow .cargo caching to registry and git sources only.
- Enable sccache in CI test jobs to cache compiled artifacts efficiently.
- Disable incremental compilation in CI to reduce file churn and improve determinism.
- Do not change test selection, nextest flags, or artifact flows.
- Do not modify or remove any secret or cache mounts used for corporate CA injection or performance.

Non-goals
- Do not change how artifacts are passed between jobs (continue using artifacts and needs).
- Do not change Docker build-time secret handling or remove secret mounts.
- Do not change make targets or nextest filters consolidated earlier.

Current state (summary)
- .rust-ci-base defines a cache with policy: pull-push and paths: .cargo, target.
- make test uses cargo nextest with consolidated ARGS_NEXTEST and uploads JUnit XML artifact.
- Large cache uploads dominate job end due to target and full .cargo contents.

Proposed changes
1) CI caching and compiler settings
- Set CARGO_INCREMENTAL=0 in .rust-ci-base to reduce target/incremental file churn.
- Switch cache policy to pull to avoid uploading heavy caches on every job.
- Limit cache paths to:
  - .cargo/registry
  - .cargo/git
  - .cache/sccache (new; sccache directory)
- Enable sccache by setting:
  - RUSTC_WRAPPER=sccache
  - SCCACHE_DIR=$CI_PROJECT_DIR/.cache/sccache

2) Image support (rust-builder)
- Ensure sccache is available in the rust-builder image used by test jobs by installing the distro
  package (Debian/Bookworm: apt-get install -y sccache).
- Keep all existing secret mounts and CA handling in Dockerfile unchanged.

3) Test structure and artifacts
- No changes to make targets, nextest invocations, or JUnit artifact emission.
- No change to artifact-producing jobs (build-launcher*, publish-release). Caches are independent of
  artifact flows; removing target from cache does not affect artifact availability.

Consistency and correctness checks
- Corporate CA injection:
  - Dockerfile retains all RUN --mount=type=secret usages and environment variable exports for CA.
  - CI job environment remains unchanged aside from the new sccache variables.
- Test behavior:
  - ARGS_NEXTEST consolidation is preserved; filters, --run-ignored, and -j settings unchanged.
- Cache effectiveness:
  - .cargo/registry and .cargo/git still accelerate dependency resolution.
  - sccache provides a compact compiled-artifact cache across runs without massive file counts.

Risks and mitigations
- Potential cold-compile slowdown if sccache is empty:
  - Mitigated by retained .cargo registry cache and rust-builder image prebuilts.
- sccache availability:
  - Installed in rust-builder, placed on PATH by apt; wired via RUSTC_WRAPPER.
- Cache directory growth:
  - Scoped to $CI_PROJECT_DIR/.cache/sccache; purged automatically by GitLab cache retention.

Rollback plan
- Re-enable previous cache paths (add target and policy: pull-push) and unset RUSTC_WRAPPER.
- Remove sccache package install from rust-builder if necessary.

Phased implementation plan

Phase 0: Land this spec (v1)
- Record date, goals, and non-goals.
- Align stakeholders on using sccache and dropping target from cache.

Phase 1: CI changes (implemented)
- .rust-ci-base:
  - Add CARGO_INCREMENTAL=0, RUSTC_WRAPPER=sccache, SCCACHE_DIR=$CI_PROJECT_DIR/.cache/sccache.
  - Change cache policy to pull.
  - Limit cache paths to .cargo/registry, .cargo/git, and .cache/sccache.

Phase 2: Image update (implemented)
- Dockerfile rust-builder stage:
  - Install sccache via apt-get (no change to CA secrets mount).
  - No other changes to toolchain.

Phase 3: Validation
- Pipelines on MR and main:
  - Observe “Saving cache” enumerates far fewer files.
  - Ensure nextest runs and artifacts (JUnit) are present as before.
  - Confirm build-launcher and release artifacts unaffected.
- Optional: check sccache stats (sccache --show-stats) interactively to verify usage.

Phase 4: Documentation
- Keep this spec file in the repo to document the rationale and approach.

Acceptance criteria
- make test and test-e2e jobs complete successfully with identical test sets and results.
- Cache upload overhead is substantially reduced (no target caching).
- Corporate CA injection and secret mounts remain functional.
- sccache directory presence and usage is visible in CI workspace; cache path includes it.
- No changes to artifact availability or release flow.

End of spec v1
