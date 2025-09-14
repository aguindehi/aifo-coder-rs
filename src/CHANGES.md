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
