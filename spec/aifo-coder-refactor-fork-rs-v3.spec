Title: Refactor src/fork.rs into cohesive submodules with stable public API (v3, implemented)
Version: v3-implemented-2025-09-15

Overview and validation
- The refactor is complete: src/fork.rs remains the public facade; internal logic is fully
  decomposed into cohesive helpers under src/fork_impl/* while preserving all public behavior
  and strings.
- To avoid collisions with the binary’s src/fork/* tree, helpers live under src/fork_impl/* and
  are imported via #[path] in src/fork.rs.
- Existing src/fork/meta.rs (manual JSON writer; key order preserved) is reused and exported from
  the library; all metadata operations continue to preserve key order and field names.
- Public API and CLI outputs remain bit-for-bit compatible; acceptance is backed by goldens.

Current scope and state (implemented)
- Cohesive internal modules:
  - src/fork_impl/git.rs: git(), git_stdout_str(), git_status_porcelain(), git_supports_lfs(),
    push_file_allow_args(), set_file_allow(), git_cmd(), git_cmd_quiet().
  - src/fork_impl/scan.rs: session_dirs(), pane_dirs_for_session(), secs_since_epoch().
  - src/fork_impl/panecheck.rs: PaneCheck, pane_check().
  - src/fork_impl/notice.rs: stale notice and autoclean logic (delegated from fork.rs).
  - src/fork_impl/list.rs: data collection and rendering for fork_list (single repo and workspace).
  - src/fork_impl/clone.rs: fork_clone_and_checkout_panes_impl() (two-attempt strategy, submodules,
    LFS best-effort, origin push URL).
  - src/fork_impl/snapshot.rs: fork_create_snapshot_impl() (temporary index + commit-tree).
  - src/fork_impl/merge.rs: collect_pane_branches_impl(), preflight_clean_working_tree_impl(),
    compose_merge_message_impl(), and merging flows (fetch-only, octopus).
  - src/fork_impl/clean/{plan.rs,prompt.rs,exec.rs}: clean planning, interactive prompt/refusal,
    execution and metadata updates.
- Public facade delegates:
  - fork_list(), fork_clean(), fork_print_stale_notice(), fork_autoclean_if_enabled(),
    fork_clone_and_checkout_panes(), fork_create_snapshot(), fork_merge_branches(),
    fork_merge_branches_by_session().
- Binary-side improvements:
  - Orchestrator code adopts helpers for porcelain status and rev-parse; no behavior changes.
  - Windows/Unix orchestrators and prompts remain byte-for-byte identical.

Tests and goldens (Phase 0 realized)
- Added coverage to lock current behavior:
  - Label sanitization: tests/sanitize_label.rs (separators, collapsing, trimming, length).
  - Quick LFS detection: tests/repo_uses_lfs_quick.rs (top-level and nested .gitattributes).
  - List goldens:
    - Single-repo JSON: tests/fork_list_public_json_golden.rs.
    - Workspace JSON: tests/fork_list_workspace_golden.rs.
    - Plain (non-color) single-repo and workspace: tests/fork_list_plain_nocolor.rs.
    - Plain (forced color) single-repo and workspace: tests/fork_list_plain_color.rs,
      tests/fork_list_workspace_plain_color.rs.
    - Workspace multiple repos JSON (order-insensitive): tests/fork_list_workspace_multi.rs.
  - Clean plan classification: src/fork_impl/clean/plan.rs (module tests for dirty, base-unknown,
    ahead).
  - Merge message prefix and truncation: src/fork_impl/merge_tests.rs.
  - Git helpers: tests/git_helpers.rs (stdout-str failure path, porcelain clean, file-allow args).
  - Snapshot presence and type: tests/fork_snapshot.rs.
- All tests pass across the matrix; JSON/strings verified by exact match where applicable.

Consistency and behavior guarantees (maintained)
- Git calls: consistently use -C <repo>; file:// allowed where required via helper; stdout/stderr
  policy unchanged.
- JSON and prompts: outputs and strings remain identical (including color decisions). Manual
  JSON writer preserves key order.
- Exit codes and error pathways unchanged.
- Colorization: continues to use color_enabled_stdout/stderr and paint with the same escapes.
- Best-effort cleanups for temp files and sidecars remain best-effort.

CI and tooling
- GitHub Actions workflow added with Linux, macOS and Windows test jobs (Clippy with -D warnings,
  full test runs). Artifacts uploaded for packaging on Linux/macOS.

Acceptance criteria (met)
- All public functions under “Public API stability” retained with identical signatures and behavior.
- fork_list JSON/plain outputs match exactly for single-repo and workspace modes (goldens).
- fork_clean dry-run JSON and prompts match; refusal and protection logic unchanged; execution
  results match across modes (force, keep-dirty, default).
- Merge flows (fetch-only and octopus) and metadata updates identical, including key order and
  field names written by meta.rs.
- Test suite: 230 passed, 24 skipped.

Risks and mitigations
- JSON key order: retained by manual writers in src/fork/meta.rs; call sites updated only to use
  exported helpers.
- Prompt wording and color: preserved by delegating without rewriting messages; golden tests guard
  critical output.
- Platform variance (git/LFS/submodules): helpers normalize invocations; behavior preserved with
  two-attempt clone and best-effort operations.

Migration notes
- src/fork.rs remains the facade; private helpers live under src/fork_impl/*.
- src/fork/meta.rs is imported and exported from the library; no duplicated JSON writers.
- src/lib.rs continues to pub use crate::fork::* so external callers remain unchanged.

Roadmap for future improvements (jumpstart)
- Testing:
  - Add TTY vs non-TTY variants for plain outputs to further lock color/no-color decisions.
  - Expand workspace tests with multiple repositories and mixed stale/age combinations for both
    JSON and plain outputs (order-insensitive assertions retained for JSON).
- Git helper adoption:
  - Periodically audit for any newly introduced direct Command::new("git") spawns outside fork
    modules and migrate to git_cmd/git_cmd_quiet as appropriate.
- Documentation:
  - Continue augmenting module-level docs in src/fork_impl/* and binary-side fork/* for faster
    onboarding and maintenance clarity.
- Optional future enhancements (non-breaking):
  - Structured logs (behind a feature/env) for merge/clean flows.
  - Concurrency/throughput guardrails for heavy operations under load.
  - Additional goldens for messages surrounding post-merge/autoclean guidance.

Change log reference (summary)
- Modules added: git, scan, panecheck, notice, list, clone, snapshot, merge, clean/{plan,prompt,exec}.
- Helpers added: git_cmd, git_cmd_quiet, push_file_allow_args, set_file_allow.
- Tests added: sanitize label, LFS quick, list goldens (JSON/plain, single/workspace), clean plan,
  merge message, git helpers, snapshot.
- CI: multi-OS matrix with clippy -D warnings and full test suites.
