Title: Refactor src/fork.rs into cohesive, reusable submodules with stable public API
Version: v1

Goals
- Decompose src/fork.rs (monolithic) into smaller, testable modules.
- Reuse the existing src/fork/meta.rs (manual JSON with preserved key order) and stop duplicating
  meta logic in src/fork.rs.
- Keep all public function names, signatures, exit codes, and user-visible text identical.
- Reduce duplication (git spawning, session scanning, pane checks, meta handling).
- Improve correctness and maintainability without changing behavior.

Non-goals
- No CLI changes and no text changes visible to users.
- No new dependencies.
- No changes to src/main.rs (the public API remains aifo_coder::*).
- Do not remove or modify the binary-side fork modules under src/fork/* in this phase.

Public API that must remain unchanged
- repo_root
- fork_sanitize_base_label
- fork_base_info
- fork_create_snapshot
- fork_branch_name
- fork_session_dir
- repo_uses_lfs_quick
- fork_clone_and_checkout_panes
- struct ForkCleanOpts
- fork_list
- fork_clean
- fork_print_stale_notice
- fork_autoclean_if_enabled
- fork_merge_branches
- fork_merge_branches_by_session

Key observations in current code
- src/fork.rs contains many concerns: repo detection, snapshotting, cloning, LFS/submodules,
  listing, cleaning (plan/prompt/execute), stale notice, autoclean, merging, and message
  composition.
- It reimplements metadata parsing/writing (meta_extract_value, fork_meta_append_fields),
  while src/fork/meta.rs already provides robust writers and tests.
- Git invocation patterns are repeated with slight variations; no central helper.
- fork_clean mixes planning, prompting, and execution in one function, making it harder to test.

High-level design
- Keep src/fork.rs as the public facade within the library crate.
- Add internal private helper modules in a separate directory to avoid collision with the
  binary’s src/fork/* tree (e.g., src/fork_impl/* referenced from src/fork.rs via #[path]).
- Reuse existing src/fork/meta.rs from the library side via #[path = "fork/meta.rs"] and export
  minimal helpers there; delete duplicated meta helpers from src/fork.rs.
- Decompose long functions (fork_clean, fork_list, merge helpers) into plan + prompt/render +
  execute stages. The public facade orchestrates these stages to preserve behavior.

Phased plan

Phase 0: Tests safety net (no refactor yet)
- Add unit tests (temp repos) for:
  - fork_sanitize_base_label (separators, collapse, truncation, empty case).
  - repo_uses_lfs_quick (.lfsconfig and nested .gitattributes with filter=lfs).
  - fork_list JSON shape and stale flag computation (order by created_at).
  - fork_clean classification (dirty/submodules-dirty/ahead/base-unknown) via temp repos.
  - compose_merge_message prefixing (“Octopus merge: …”) and truncation.
- Use // ignore-tidy-linelength in tests if needed.

Phase 1: Internal helpers (no behavior change)
- Add new private modules under src/fork_impl and reference them from src/fork.rs:
  - fork_git.rs (private):
    - git(repo, args) -> io::Result<Output> with -C and piped stdio.
    - git_ok(repo, args) -> bool.
    - git_stdout_str(repo, args) -> Option<String>.
    - git_status_porcelain(repo) -> Option<String>.
    - git_supports_lfs() -> bool (current probe).
    - Helper to add -c protocol.file.allow=always for file:// sources.
  - fork_scan.rs (private):
    - forks_base(repo_root) -> PathBuf (repo/.aifo-coder/forks).
    - list_session_dirs(base) -> Vec<PathBuf>.
    - list_pane_dirs(session_dir) -> Vec<PathBuf>.
    - read_created_at(session_dir) -> u64 (from meta or fs metadata).
    - age_days(now, created_at) -> u64.
  - fork_panecheck.rs (private):
    - struct PaneCheck { clean: bool, reasons: Vec<String> }.
    - pane_check(pane_dir, base_commit: Option<&str>) -> PaneCheck
      (dirty via status porcelain, submodules-dirty via submodule status,
       ahead vs base-unknown via rev-list count).
- Replace direct git invocations in src/fork.rs with these helpers internally.
- Preserve current arguments, stdio behavior, and output text.

Phase 2: Metadata reuse and minimal expansion (no behavior change)
- In src/fork/meta.rs, add public helpers consumed by src/fork.rs:
  - pub fn extract_value_string(text, key) -> Option<String>.
  - pub fn extract_value_u64(text, key) -> Option<u64>.
  - pub fn append_fields_compact(repo_root, sid, fields_kv: &str) -> io::Result<()>
    (migrate fork_meta_append_fields unchanged).
- Update src/fork.rs to:
  - Replace meta_extract_value calls with meta::extract_value_*.
  - Replace fork_meta_append_fields calls with meta::append_fields_compact.
- Remove duplicated meta_* helpers from src/fork.rs.

Phase 3: fork_clean decomposition (no behavior change)
- Move plan-building logic to fork_impl/clean_plan.rs:
  - Build per-session plan: session dir + Vec<(pane_dir, PaneCheck)>.
  - Compute protected vs clean counts and which sessions are deletable per mode.
  - Provide function to render the JSON dry-run plan exactly as today.
- Move user prompt to fork_impl/clean_prompt.rs:
  - Factor interactive confirmation printing the exact same prompt strings and totals.
  - Use ui::warn only if it can render identical text; otherwise keep current direct I/O.
- Move execution to fork_impl/clean_exec.rs:
  - Execute deletions; call toolchain_cleanup_session before removing a session dir.
  - With keep_dirty, delete only clean panes and update .meta.json using src/fork/meta.rs
    (preserve fields and messages).
- Keep fork_clean in src/fork.rs as an orchestrator that calls these helpers and preserves
  exit codes and user-visible text.

Phase 4: Listing, stale notice, and autoclean (no behavior change)
- fork_impl/list.rs:
  - Collect rows (sid, panes, created_at, age_days, base_label, stale), sorted by created_at.
- fork_list remains a thin wrapper, producing identical JSON/plain output for single-repo and
  workspace modes.
- fork_impl/notice.rs:
  - Implement stale-notice and autoclean calculations.
- Keep fork_print_stale_notice and fork_autoclean_if_enabled as thin wrappers over notice helpers.

Phase 5: Clone, snapshot, and merge helpers (no behavior change)
- fork_impl/clone.rs: move fork_clone_and_checkout_panes logic; preserve path vs file:// fallback,
  submodule updates, push URL setting, and LFS operations.
- fork_impl/snapshot.rs: move fork_create_snapshot; encapsulate temporary index handling.
- fork_impl/merge.rs: move collect_pane_branches, compose_merge_message, and merge logic.
- Public functions fork_clone_and_checkout_panes, fork_create_snapshot, fork_merge_branches,
  and fork_merge_branches_by_session delegate to these helpers; messages unchanged.

Phase 6: Consistency and minor robustness (no behavior change)
- Ensure all git calls use -C consistently and silent stdio where current code does.
- Ensure file:// protocol.allow is consistently passed where needed.
- Ensure temporary merge message file is always removed (best-effort).
- Keep colorized printing via color_enabled_stdout/stderr and paint. Do not change strings.

Acceptance criteria
- All functions listed under “Public API” remain identical in signature and behavior.
- All existing messages and prompts remain identical (byte-for-byte where feasible).
- fork_list JSON/plain outputs match exactly for the same state.
- fork_clean dry-run JSON and interactive prompts match exactly; protection refusal logic unchanged.
- Merge flow (fetch-only and octopus) and metadata append operations remain identical.
- Tests added in Phase 0 pass on supported platforms.

Risks and mitigations
- JSON ordering differences: continue using src/fork/meta.rs writers that preserve key order for
  known fields; only add minimal exported helpers.
- Prompt behavior: keep wording and flow; avoid helper substitutions that alter text.
- Dual “fork” trees (library vs binary): add helper modules under src/fork_impl and import
  src/fork/meta.rs via #[path] to avoid collisions with the binary’s src/fork/*.

Migration notes
- Update src/fork.rs to reference helper modules with #[path = "fork_impl/..."] mod ...;
  keep public functions and outputs identical.
- Ensure src/lib.rs continues to pub use crate::fork::* so callers remain unchanged.

Open questions
- Do any external consumers rely on exact .meta.json key order beyond what meta.rs preserves?
- Should we feature-gate verbose tracing of git invocations to aid debugging?
