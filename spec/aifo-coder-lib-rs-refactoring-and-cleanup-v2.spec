AIFO Coder — lib.rs refactoring and cleanup v2
Phased specification, validation, and implementation plan

Executive summary
- Goal: Transform src/lib.rs from a “kitchen sink” into a clean crate façade by moving implementation details into focused modules, removing small duplications, and preserving 100% of public API paths, user-visible strings, exit codes, and stream routing.
- This v2 plan validates the prior v1 proposal against the current repository state and tightens scope, order, and acceptance checks.
- The refactor preserves all Phase 0 constraints from docs/phase0-string-inventory-main.md and docs/phase0-updates.md.

Validated constraints and current state (as of v2)
- src/lib.rs currently contains:
  - Windows-only helpers (cfg(windows)):
    - ps_quote_inner (private), fork_ps_inner_string, fork_bash_inner_string
    - wt_orient_for_layout, wt_build_new_tab_args, wt_build_split_args
    - ps_wait_process_cmd
    - Invariants:
      - No AIFO_CODER_SUPPRESS_TOOLCHAIN_WARNING in inner strings (orchestrators inject it).
      - Bash variant ends with “; exec bash”.
      - wt_* include "wt" at argv[0] for previews; callers drop it for execution.
  - UI warnings:
    - warn_print, warn_prompt_continue_or_quit (two trailing newlines after decision).
  - Public MergingStrategy (clap::ValueEnum).
  - Generic helpers: path_pair, ensure_file_exists, create_session_id.
  - A large #[cfg(test)] module spanning util, http/notifications, sidecar previews, lock paths, fork helpers, etc.
- src/fork/env.rs provides fork_env_for_pane() including SUPPRESS and SKIP_LOCK for orchestrators (correct); does not yet provide an “inner” KV helper that explicitly excludes SUPPRESS for inner string builders.
- Duplication confirmed:
  - find_crlfcrlf exists in both src/util.rs and src/toolchain/http.rs. util::find_crlfcrlf should be canonical.
  - wt_* builders share an identical “-d <dir> <psbin> -NoExit -Command <inner>” tail.
  - warn_prompt_continue_or_quit repeats the “finish prompt line” printing across branches.

API compatibility guarantees
- The following public symbols remain available at the same paths via re-exports from aifo_coder:::
  - fork_ps_inner_string, fork_bash_inner_string
  - wt_orient_for_layout, wt_build_new_tab_args, wt_build_split_args
  - ps_wait_process_cmd
  - warn_print, warn_prompt_continue_or_quit
  - MergingStrategy
  - path_pair, ensure_file_exists, create_session_id
- Windows-only public items remain behind cfg(windows) so non-Windows builds continue to compile without resolving them.

Behavioral invariants (must hold after refactor)
- No change to any user-visible string, whitespace, color decisions, or stdout/stderr routing listed in the Phase 0 inventory docs.
- Exit codes remain unchanged (NotFound → 127; generic errors → 1).
- Windows inner builders:
  - No AIFO_CODER_SUPPRESS_TOOLCHAIN_WARNING in returned inner strings.
  - Bash inner keeps the “; exec bash” suffix.
- wt_* preview args include "wt" at argv[0]; execution paths continue to drop argv[0] before Command::new(wt_path).
- warn_prompt_continue_or_quit:
  - Interactive behavior unchanged on Windows (_getch) and Unix (stty non-canonical, no-echo, read 1 byte).
  - Emits exactly two trailing newlines after the decision across all branches.

Optimized phased plan

Phase 0 — Inventory guard (documentation-only)
- Reconfirm that all strings in docs/phase0-string-inventory-main.md and docs/phase0-updates.md are preserved across phases.
- Validate cfg(windows) gating semantics, ensuring non-Windows builds do not require resolving Windows-only items.

Phase 1 — Deduplicate CRLFCRLF scanning (low-risk; localized)
- Canonicalize util::find_crlfcrlf. Remove the local helper in src/toolchain/http.rs and import use crate::find_crlfcrlf.
- Optional (micro-cleanup): replace header terminator search with crate::find_header_end to compute body_start when tests confirm byte-identical behavior.
- Acceptance:
  - cargo test passes.
  - No change to HTTP parsing behavior or logs.

Phase 2 — Centralize inner-builder pane env KVs (prep-only; behavior-preserving)
- In src/fork/env.rs, add:
  - pub(crate) fn fork_inner_env_kv(agent: &str, sid: &str, i: usize, pane_state_dir: &Path) -> Vec<(String, String)>
  - Return exactly the KVs used by inner builders today:
    - AIFO_CODER_SKIP_LOCK=1
    - AIFO_CODER_CONTAINER_NAME
    - AIFO_CODER_HOSTNAME
    - AIFO_CODER_FORK_SESSION
    - AIFO_CODER_FORK_INDEX
    - AIFO_CODER_FORK_STATE_DIR
  - Explicitly exclude AIFO_CODER_SUPPRESS_TOOLCHAIN_WARNING (injected by orchestrators).
- Add a unit test in src/fork/env.rs verifying keys and representative values.
- Acceptance: compile + unit test passes; no external behavior change yet.

Phase 3 — Extract UI warnings (self-contained; minimal churn)
- New module: src/ui/warn.rs
  - Move warn_print and warn_prompt_continue_or_quit as-is.
  - Add a private fn finish_prompt_line() that prints the two trailing newlines and call it uniformly in all branches.
  - Do not alter prompt text, color logic, or platform-specific read behavior.
- In lib.rs:
  - mod ui;
  - pub use ui::warn::{warn_print, warn_prompt_continue_or_quit};
- Acceptance:
  - All callers compile via aifo_coder::warn_*.
  - Byte-for-byte identical outputs in tests, including whitespace.

Phase 4 — Extract Windows fork helpers (cfg-gated; behavior-preserving)
- New module: src/fork/windows/helpers.rs (introduce mod fork::windows).
  - Move, unchanged in signatures and behavior:
    - ps_quote_inner (private)
    - fork_ps_inner_string
    - fork_bash_inner_string
    - wt_orient_for_layout
    - wt_build_new_tab_args
    - wt_build_split_args
    - ps_wait_process_cmd
  - Implement wt_tail(psbin, pane_dir, inner) -> Vec<String> (private) to remove the arg-tail duplication in wt_*.
  - Update fork_ps_inner_string/fork_bash_inner_string to use fork::env::fork_inner_env_kv for KV rendering (still excluding SUPPRESS).
- In lib.rs:
  - #[cfg(windows)] pub use fork::windows::helpers::{
      fork_ps_inner_string, fork_bash_inner_string,
      wt_orient_for_layout, wt_build_new_tab_args, wt_build_split_args,
      ps_wait_process_cmd
    };
- Acceptance:
  - Windows-only unit tests pass under cfg(windows).
  - “wt” remains argv[0] in preview args.
  - Bash inner retains “; exec bash”.

Phase 5 — Move MergingStrategy into fork domain (clarity; low-risk)
- Prefer a lib-only module to avoid compiling orchestrator structs into the library:
  - Place the MergingStrategy enum in src/fork/strategy.rs (add use clap::ValueEnum).
- In lib.rs: re-export as aifo_coder::MergingStrategy (non-gated). The internal module path is an implementation detail.
- Acceptance: all call sites (e.g., src/fork/post_merge.rs, src/main.rs) compile and behave identically.

Phase 5a — Decouple lib from orchestrator types (fix clippy without masking)
- Problem: the lib target compiles src/fork/types.rs (ForkSession, Pane, ForkOptions), which are not constructed in library code, triggering -D dead_code.
- Solution:
  - Do not compile src/fork/types.rs in the lib target; keep these structs in the bin module tree only.
  - Keep the public enum in a lib-only module (src/fork/strategy.rs) and re-export it as aifo_coder::MergingStrategy (see Phase 5).
  - Change fork_env_for_pane to a parts-based signature so it no longer depends on ForkSession/Pane:
    - pub fn fork_env_for_pane(sid: &str, pane_index: usize, container_name: &str, pane_state_dir: &Path) -> Vec<(String, String)>
    - Return exactly the same KVs as before (including AIFO_CODER_SUPPRESS_TOOLCHAIN_WARNING=1 and AIFO_CODER_SKIP_LOCK=1).
  - Update bin call sites to pass these parts (e.g., src/fork/inner.rs build_tmux_launch_script). No change to user-visible behavior.
  - Keep fork_inner_env_kv (Windows-only) unchanged; it continues to exclude SUPPRESS and is used by inner string builders.
- Acceptance:
  - cargo clippy and cargo test pass with -D warnings.
  - No change to user-visible strings, exit codes, or streams.
  - Public API commitments remain intact (aifo_coder::MergingStrategy path).

Phase 6 — Extract fs/id helpers (low-risk; clarity)
- New modules:
  - src/util/fs.rs: move path_pair, ensure_file_exists.
  - src/util/id.rs: move create_session_id.
- In lib.rs:
  - pub use util::fs::{path_pair, ensure_file_exists};
  - pub use util::id::create_session_id;
- Acceptance: compile; tests pass; behavior unchanged.

Phase 7 — Slim lib.rs to a façade (moderate; contained churn)
- Remove from src/lib.rs:
  - Windows helpers (now under fork::windows::helpers).
  - UI warnings (now under ui::warn).
  - MergingStrategy enum (now under fork::types).
  - fs/id helpers (now under util submodules).
  - The large #[cfg(test)] module; relocate tests as outlined in Phase 8.
- Keep:
  - mod declarations for apparmor, color, docker, fork, lock, registry, toolchain, util, ui.
  - pub use re-exports preserving public API symbols.
- Ensure all Windows-only re-exports are cfg-gated.
- Acceptance: cargo test passes (macOS/Linux/Windows); no public API breakage.

Phase 8 — Test relocation and ownership (incremental; optional)
- Move unit tests to their owning modules:
  - Windows helpers → src/fork/windows/helpers.rs
  - Lock path tests → src/lock.rs
  - HTTP/notifications tests → src/toolchain/{http.rs,notifications.rs}
  - Sidecar preview tests → src/toolchain/sidecar.rs
  - Fork clone/snapshot/merge/cleanup tests → src/fork/* (e.g., inner.rs, cleanup.rs, post_merge.rs)
- Cross-cutting tests remain in tests/.
- Prefer importing moved items via aifo_coder::* to validate the public façade.
- Acceptance: identical assertions; improved incremental build times.

Optional Phase 9 — Micro-dedup in HTTP parsing (strictly behavior-preserving)
- Replace toolchain/http.rs decode_component calls with crate::url_decode for both key and value if test coverage confirms identical behavior (notably: ‘+’ → space, best-effort handling of invalid hex remains compatible).
- Acceptance: cargo test passes; no changes to parsed outputs.

Cross-cutting acceptance and verification
- Byte-for-byte string stability:
  - Compare outputs for all inventory-listed messages before and after refactor in CI.
- Helper invariants:
  - Windows inner strings exclude SUPPRESS; Bash inner ends with “; exec bash”.
  - wt_* keep argv[0]="wt" for previews; Callers continue to drop argv[0] before execution.
  - warn_prompt_continue_or_quit emits two trailing newlines and preserves platform-specific read behavior.
- Streams and color:
  - All eprintln!/println! routing remains identical.
  - color_enabled_* decisions are unchanged.
- Exit codes and timing:
  - No change to exit code mapping.
  - No change to env read/set timing (Section M of the inventory).

Risks and mitigations
- Missed re-exports or incorrect cfg gating break downstream code.
  - Mitigation: implement and compile re-exports prior to removals; add cfg(windows) gates to both modules and re-exports; run cargo test on macOS/Linux/Windows.
- Subtle whitespace changes in prompts.
  - Mitigation: centralize finishing newlines via finish_prompt_line(); verify byte-for-byte with tests.
- Test relocation masking regressions.
  - Mitigation: perform code moves first; relocate tests in a separate commit (Phase 8) with no changes to assertions.

Backout strategy
- Each phase is independently revertible.
- Because public API is preserved via re-exports, any module move can be temporarily reverted (re-inline specific functions) without changing callers.

Implementation checklist (quick reference)
- Phase 1:
  - src/toolchain/http.rs: remove local find_crlfcrlf; use crate::find_crlfcrlf (optionally crate::find_header_end).
- Phase 2:
  - src/fork/env.rs: add fork_inner_env_kv(..) excluding SUPPRESS; unit test it.
- Phase 3:
  - Add src/ui/warn.rs; move warn_print and warn_prompt_continue_or_quit; add finish_prompt_line(); re-export in lib.rs.
- Phase 4:
  - Add src/fork/windows/helpers.rs; move Windows helpers; add wt_tail(); use fork_inner_env_kv; cfg-gate and re-export in lib.rs.
- Phase 5:
  - Place MergingStrategy in src/fork/strategy.rs (lib-only); re-export from lib.rs.
- Phase 5a:
  - Change fork_env_for_pane to parts-based signature; update bin call sites; ensure the lib does not compile src/fork/types.rs.
- Phase 6:
  - Create src/util/fs.rs and src/util/id.rs; move path_pair, ensure_file_exists, create_session_id; re-export in lib.rs.
- Phase 7:
  - Slim src/lib.rs to façade; keep mod declarations and re-exports; remove moved impls and large #[cfg(test)].
- Phase 8:
  - Relocate tests to owning modules; keep integration tests in tests/; prefer aifo_coder::* imports.
- Phase 9 (optional):
  - toolchain/http.rs: consider using crate::url_decode in parse_form_urlencoded after verifying behavior equivalence.

Notes
- Prefer smaller commits per phase for easier review and bisectability.
- Validate Windows gating by running cargo test --target x86_64-pc-windows-gnu in CI where possible or in a Windows environment.
- Maintain exact user-visible behavior listed in the Phase 0 inventory; treat these as compatibility tests during the refactor.
