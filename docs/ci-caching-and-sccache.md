# CI caching and sccache adoption for Rust builds

Date: 2025-11-17

Summary
- We reduced CI cache overhead by:
  - Removing target from GitLab caches in test jobs.
  - Narrowing .cargo caching to registry and git sources.
  - Enabling sccache for compiled artifacts.
  - Disabling incremental compilation (CARGO_INCREMENTAL=0) for determinism and lower churn.
- We preserved all secret and cache mounts used for corporate CA injection and performance.
- Test structures, nextest flags, and artifact flows remain unchanged.

Spec reference
- See aifo-coder-drop-ci-caching-of-rust-target-add-sscache-v1.spec for motivation, goals,
  acceptance criteria, and phased plan.

What changed
- .gitlab-ci.yml (.rust-ci-base):
  - variables: CARGO_INCREMENTAL=0, RUSTC_WRAPPER=sccache, SCCACHE_DIR=$CI_PROJECT_DIR/.cache/sccache.
  - cache: policy set to pull; paths limited to .cargo/registry, .cargo/git, .cache/sccache.
- Dockerfile (rust-builder and macOS cross builder stages):
  - Installed sccache via apt to ensure availability in CI images.

What did not change
- Corporate CA secret mounts and environment handling in Dockerfile remain intact.
- Test selection and nextest invocation semantics (filters, run-ignored, -j) are unchanged.
- Artifacts continue to be used to pass build outputs across jobs.

Validation guidance
- After a pipeline, review sccache stats in job logs (after_script prints them).
- Expect far fewer files listed in “Saving cache” compared to previous target caching.
- JUnit and release artifacts are unaffected.

Rollback
- If needed, revert to previous behavior by:
  - Restoring target to cache paths and cache policy to pull-push.
  - Unsetting RUSTC_WRAPPER and removing sccache install from images.

Notes
- sccache directory is scoped to $CI_PROJECT_DIR/.cache/sccache and included in the job cache.
- CI cache retention policies will purge stale sccache content automatically.

End of doc
