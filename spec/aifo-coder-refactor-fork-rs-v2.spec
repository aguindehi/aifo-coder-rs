Title: Refactor src/fork.rs into cohesive submodules with stable public API (v2)
Version: v2

Overview and validation
- The v1 plan is sound: keep src/fork.rs as the public facade within the library crate, refactor
  internally into cohesive helpers, and preserve all public behavior and strings.
- To avoid collision with the binary’s src/fork/* tree, add private helper modules under a new
  directory (src/fork_impl/*) and import them into src/fork.rs via #[path].
- Reuse the existing src/fork/meta.rs (manual JSON, preserved key order) from the library side
  via #[path = "fork/meta.rs"]. Export minimal helpers there and delete duplicated meta helpers
  from src/fork.rs.
- No CLI, output, or semantic changes are permitted. Public API remains bit-for-bit identical.

Scope
- In-scope: internal module decomposition, centralizing git invocation, session scanning, pane
  checks, metadata reuse, and plan/prompt/execute decomposition for maintenance flows.
- Out-of-scope: any user-visible changes, new dependencies, or removal of the binary-side fork
  modules under src/fork/*.

Public API stability (must remain unchanged)
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

Consistency and behavior guarantees
- Git calls: always use -C <repo>, respect current stdio silencing, and add
  -c protocol.file.allow=always for file:// sources where used today.
- JSON and prompts: outputs and strings must remain identical (including color decisions,
  punctuation, and wording). Continue using manual JSON writers in meta.rs to preserve key order.
- Exit codes and error pathways remain unchanged.
- Colorization: keep using color_enabled_stdout/stderr and paint with the same escape sequences.
- Best-effort cleanups (temp files, sidecars) must remain best-effort.

Compressed phased implementation plan

Phase 0: Safety net tests (no refactor yet)
- Add fast unit/integration tests to lock current behavior:
  - fork_sanitize_base_label edge cases (separators, collapse, truncation, empty).
  - repo_uses_lfs_quick for .lfsconfig and nested .gitattributes filter=lfs.
  - fork_list ordering by created_at and stale flag computation; assert JSON/plain outputs.
  - fork_clean plan classification for dirty/submodules-dirty/ahead/base-unknown via temp repos.
  - compose_merge_message prefixing (“Octopus merge: …”) and 160-char truncation.
- Use // ignore-tidy-linelength where needed in tests; no dependency changes.

Phase 1: Internal helpers and metadata reuse (no behavior change)
- Add src/fork_impl/git.rs (private):
  - git(repo, args) -> io::Result<Output>
  - git_ok(repo, args) -> bool
  - git_stdout_str(repo, args) -> Option<String>
  - git_status_porcelain(repo) -> Option<String>
  - git_supports_lfs() -> bool
  - helper to amend args with -c protocol.file.allow=always for file:// paths
- Add src/fork_impl/scan.rs (private):
  - forks_base(repo_root) -> PathBuf
  - list_session_dirs(base) -> Vec<PathBuf>
  - list_pane_dirs(session_dir) -> Vec<PathBuf>
  - read_created_at(session_dir) -> u64 (from meta or fs metadata)
  - age_days(now, created_at) -> u64
- Add src/fork_impl/panecheck.rs (private):
  - struct PaneCheck { clean: bool, reasons: Vec<String> }
  - pane_check(pane_dir, base_commit: Option<&str>) -> PaneCheck
    (dirty via status porcelain, submodules via submodule status, ahead/base-unknown via rev-list)
- In src/fork.rs, replace duplicated inline git and scanning code with these helpers internally.
- Integrate src/fork/meta.rs into the library via #[path = "fork/meta.rs"] mod meta;
  add public helpers in meta.rs and switch callsites:
  - pub fn extract_value_string(text, key) -> Option<String>
  - pub fn extract_value_u64(text, key) -> Option<u64>
  - pub fn append_fields_compact(repo_root, sid, fields_kv: &str) -> io::Result<()>
    (migrate fork_meta_append_fields unchanged)
- Remove meta_extract_value and fork_meta_append_fields from src/fork.rs.

Phase 2: Decompose maintenance flows (no behavior change)
- Clean path:
  - src/fork_impl/clean/plan.rs:
    - Build per-session plan: (session_dir, Vec<(pane_dir, PaneCheck)>)
    - Compute protected vs clean counts; decide deletability under force/keep-dirty modes
    - Render the exact dry-run JSON plan as today
  - src/fork_impl/clean/prompt.rs:
    - Print totals and prompt identically to current messages (guard non-interactive stdin)
  - src/fork_impl/clean/exec.rs:
    - Execute deletions; call toolchain_cleanup_session before removing session dirs
    - With keep_dirty, delete only clean panes and update .meta.json via src/fork/meta.rs
- fork_clean in src/fork.rs becomes a thin orchestrator calling plan -> optional JSON/dry-run exit
  -> prompt (unless yes/json) -> exec; keep messages and exit codes identical.
- List/notice/autoclean path:
  - src/fork_impl/list.rs: collect list rows (sid, panes, created_at, age_days, base_label, stale),
    sorted by created_at; render identical JSON/plain output for single-repo and workspace modes
  - src/fork_impl/notice.rs: compute stale-notice and autoclean decisions; keep text identical
- Merge/clone/snapshot helpers:
  - src/fork_impl/clone.rs: move fork_clone_and_checkout_panes with same two-attempt clone strategy,
    submodules update, LFS steps, and origin push URL handling
  - src/fork_impl/snapshot.rs: move fork_create_snapshot; encapsulate temporary index handling
  - src/fork_impl/merge.rs: move collect_pane_branches, compose_merge_message, and merge logic;
    preserve messages, temp file cleanup, and metadata updates

Phase 3: Consistency pass and verification (no behavior change)
- Ensure all git invocations consistently use -C and current stdio behavior.
- Ensure file:// protocol.allow=always is passed everywhere it is today.
- Preserve exact strings (prompts, warnings, JSON) and color codes; verify with golden assertions
  where feasible.
- Confirm temporary files (merge message) are always removed best-effort.
- Run and fix tests; compare outputs for fork_list and fork_clean dry-run JSON byte-for-byte.

Acceptance criteria
- All functions listed under “Public API stability” keep identical signatures and behavior.
- fork_list JSON/plain outputs match exactly (single-repo and workspace).
- fork_clean dry-run JSON and interactive prompts match exactly; refusal and protection logic
  remains unchanged; execution results match across modes (force/keep-dirty/default).
- Merge flows (fetch-only and octopus) and metadata append operations remain identical, including
  key order and field names written by meta.rs.
- Existing and new tests pass on supported platforms.

Risks and mitigations
- JSON key order: continue to use src/fork/meta.rs manual writers; only add minimal exported
  helpers to avoid reordering.
- Prompt wording: route through a helper only if it can emit identical text; otherwise keep
  direct I/O flows.
- Dual trees (library vs binary): place new helpers under src/fork_impl and import meta via
  #[path = "fork/meta.rs"] to avoid collisions with src/fork/* used by the binary.
- LFS/Submodules variability: centralize git calls and keep current silent stdio behavior;
  maintain two-attempt clone strategy and LFS detection.

Migration notes
- Keep src/fork.rs as the public facade; add #[path] wiring to private helpers in src/fork_impl/*.
- Do not rename src/fork.rs to src/fork/mod.rs to avoid colliding with the binary’s module tree.
- Ensure src/lib.rs continues to pub use crate::fork::* so external callers remain unchanged.
