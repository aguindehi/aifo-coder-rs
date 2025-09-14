2025-09-14 00:00 User <user@example.com>

Tests: add Phase-0 unit tests for label sanitization and LFS quick scan

- Added tests/sanitize_label.rs to lock fork_sanitize_base_label behavior (separators, trim, length).
- Added tests/repo_uses_lfs_quick.rs to verify .lfsconfig/.gitattributes (top-level and nested).
- Next: decompose fork_clean into clean/{plan,prompt,exec} modules per spec Phase 2.

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
