2025-09-15 00:00 User <user@example.com>

Docs: add runner module docs; refine SCORE next steps

- Added module-level docs to src/fork/runner.rs (binary-side orchestrator).
- Updated src/SCORE.md recommendations to the next iteration (monitor CI, more goldens).

2025-09-15 00:00 User <user@example.com>

QA: test suite green (225 passed, 24 skipped)

- Verified CI and tests across modules; all tests are now green.
- No functional changes; documentation and minor cleanup only.

2025-09-15 00:00 User <user@example.com>

QA: test suite green (223 passed, 24 skipped)

- Verified recommendations implemented; entire test suite green.
- No code changes in this entry; documentation-only update.

2025-09-15 00:00 User <user@example.com>

Tests: add golden JSON and git helper tests; docs and git_cmd helper

- Added golden-style JSON row formatter test for fork_list and a targeted test suite for git helpers.
- Introduced git_cmd builder in fork_impl/git.rs and adopted it in pane checks; added module docs across fork_impl/*.
- Refactored list JSON building to use a shared formatter to keep byte-for-byte stability.
- Kept public behavior unchanged; all tests remain green.

2025-09-14 00:00 User <user@example.com>

Tests: add missing Phase-0 coverage; centralize git file:// allow helper

- Added unit tests for fork list ordering and stale-flag computation (module tests in fork_impl/list.rs).
- Added unit tests for fork clean plan classification (dirty, base-unknown, ahead) in fork_impl/clean/plan.rs.
- Added unit tests for compose_merge_message prefix/truncation (module tests via fork_impl/merge_tests.rs).
- Introduced helper in fork_impl/git.rs to centralize -c protocol.file.allow=always:
  push_file_allow_args (for previews) and set_file_allow (for Command).
- Updated merge/clone internal call sites to use the helper while preserving behavior and logs.

2025-09-14 00:00 User <user@example.com>

Refactor: extract list/clone/snapshot helpers into fork_impl and delegate

- Moved fork_list into src/fork_impl/list.rs; fork.rs now delegates, preserving output and ordering.
- Moved fork_clone_and_checkout_panes into src/fork_impl/clone.rs; behavior identical.
- Moved fork_create_snapshot into src/fork_impl/snapshot.rs; behavior identical.
- Left fork_clean decomposition (plan/prompt/exec) for the next step to complete Phase 2.
- All public APIs and strings preserved; existing tests continue to pass.

2025-09-14 00:00 User <user@example.com>

Refactor: extract merge helpers into fork_impl; delegate from fork.rs

- Extracted merge-related helpers and flows into src/fork_impl/merge.rs:
  collect_pane_branches, preflight_clean_working_tree, compose_merge_message, and
  public wrappers fork_merge_branches/fork_merge_branches_by_session now delegate.
- Kept behavior, outputs, and exit codes identical; internal only.
- Added module-level docs to clarify responsibilities of the new helper module.
- Left clone/snapshot/list extraction as next steps to keep this change focused.

2025-09-14 00:00 User <user@example.com>

Refactor fork.rs; add helpers; fix lock CWD race; update spec

- Decomposed fork logic into private modules under src/fork_impl (git, panecheck, notice)
  and delegated from src/fork.rs without changing public APIs or outputs.
- Centralized Git calls (git_stdout_str, git_status_porcelain, git_supports_lfs) and
  replaced ad-hoc invocations where appropriate.
- Implemented pane cleanliness checks in fork_impl/panecheck.rs to unify logic.
- Moved stale notice and auto-clean logic to fork_impl/notice.rs; fork.rs now delegates.
- Fixed candidate_lock_paths to capture the initial CWD to avoid races in tests.
- Updated specification spec/aifo-coder-refactor-fork-rs-v3.spec to v3-implemented.
- Ensured manual JSON writers in src/fork/meta.rs are reused and exported via lib.
- All tests pass: 207 passed, 24 skipped.
