AIFO Coder — lib.rs refactoring and cleanup v1
Phased plan, validation, and specification

Context and goals
- lib.rs currently serves as both the crate façade and a container for multiple unrelated helpers (Windows fork builders, UI warning/prompt, merge enum, fs/id utilities) plus a large cross-cutting test module.
- Objective: slim lib.rs to a clean façade and relocate implementation details into focused modules without changing public API behavior, user-visible strings, exit codes, or stream routing.
- Preserve all documented Phase 0 constraints from docs/phase0-string-inventory-main.md and docs/phase0-updates.md, including helper invariants called out in “Helper verification.”

Non-goals and constraints
- Do not change any user-visible strings, color decisions, or stdout/stderr routing.
- Do not change exit codes.
- Preserve existing helper behavior:
  - fork_ps_inner_string/fork_bash_inner_string must NOT include AIFO_CODER_SUPPRESS_TOOLCHAIN_WARNING.
  - fork_bash_inner_string must keep the “; exec bash” suffix (bin/orchestrator trims it when requested).
  - wt_* argument builders must include "wt" as argv[0] for preview strings (callers drop argv[0] for execution).
  - ps_wait_process_cmd returns "Wait-Process -Id …".
- Keep the current public API paths stable via re-exports in lib.rs (callers continue using aifo_coder::… symbols).
- cfg(windows) gating must be preserved for Windows-only helpers so non-Windows builds compile cleanly.

API-compatibility commitments
After refactoring, the following public symbols remain available at the same paths via re-exports:
- aifo_coder::fork_ps_inner_string
- aifo_coder::fork_bash_inner_string
- aifo_coder::wt_orient_for_layout
- aifo_coder::wt_build_new_tab_args
- aifo_coder::wt_build_split_args
- aifo_coder::ps_wait_process_cmd
- aifo_coder::warn_print
- aifo_coder::warn_prompt_continue_or_quit
- aifo_coder::MergingStrategy
- aifo_coder::path_pair
- aifo_coder::ensure_file_exists
- aifo_coder::create_session_id

Behavioral invariants to re-validate
- Windows inner strings: no SUPPRESS var injected; bash variant ends with “; exec bash”.
- wt_* preview args include "wt" at argv[0]; callers must keep dropping it before Command::new(wt_path).
- warn_prompt_continue_or_quit keeps identical prompt behavior and prints two trailing newlines after decision on all platforms.
- No change to helper timing/ordering noted in Section M of the string inventory.
- No change to exit codes or stream routing (stderr vs stdout).

Known duplications and gaps
- Duplicate CRLFCRLF detection in src/util.rs and src/toolchain/http.rs.
- Duplicate env KV assembly across fork_ps_inner_string and fork_bash_inner_string.
- Duplicate Windows Terminal arg tail in wt_build_new_tab_args/split_args.
- Repeated prompt end newlines in warn_prompt_continue_or_quit branches.
- Potential future gap: parse_form_urlencoded in toolchain/http.rs re-implements URL decoding that could leverage util::url_decode (optional; must preserve behavior).

Phased plan

Phase 0 — Guards and validation (no code changes)
- Reconfirm: all strings in docs/phase0-string-inventory-main.md and docs/phase0-updates.md remain unchanged by this refactor.
- Reconfirm helper invariants: wt_* argv[0]="wt" for previews; Windows inner builders exclude SUPPRESS var; bash inner ends with “; exec bash”.
- Ensure plan preserves cfg(windows) semantics and that re-exports are also cfg-gated so non-Windows builds do not attempt to resolve Windows-only symbols.

Phase 1 — Deduplicate CRLFCRLF scanning (low-risk)
- Treat util::find_crlfcrlf as canonical; remove the local helper in src/toolchain/http.rs and import/use crate::find_crlfcrlf.
- Optional micro-cleanup (guarded by tests): use crate::find_header_end to locate header end and compute body_start directly (identical behavior).
- Acceptance:
  - cargo test passes.
  - No changes to HTTP parsing behavior (request classification, body handling) or logs.

Phase 2 — Centralize inner-builder env KVs (prep work; behavior-preserving)
- Add helper in src/fork/env.rs:
  - pub(crate) fn fork_inner_env_kv(agent: &str, sid: &str, i: usize, pane_state_dir: &Path) -> Vec<(String, String)>
  - Return exactly: AIFO_CODER_SKIP_LOCK=1, AIFO_CODER_CONTAINER_NAME, AIFO_CODER_HOSTNAME, AIFO_CODER_FORK_SESSION, AIFO_CODER_FORK_INDEX, AIFO_CODER_FORK_STATE_DIR.
  - Explicitly exclude AIFO_CODER_SUPPRESS_TOOLCHAIN_WARNING (orchestrators/panes still inject it).
- Unit-test the helper against expected keys/values.
- Acceptance:
  - No external behavior change yet; inner builders will adopt this in Phase 4.

Phase 3 — Extract UI warnings (moderate, self-contained)
- New module: src/ui/warn.rs
- Move warn_print and warn_prompt_continue_or_quit unchanged in behavior.
- Add small private finish_prompt_line() to emit the two trailing newlines and call it from all branches (Windows, Unix, fallback) to remove duplication without altering output.
- In lib.rs: mod ui; pub use ui::warn::{warn_print, warn_prompt_continue_or_quit};
- Acceptance:
  - All call sites compile via aifo_coder::warn_*.
  - Prompt behavior, color decisions, and output formatting (including whitespace) remain identical.

Phase 4 — Extract Windows fork helpers (moderate; cfg-gated)
- New module: src/fork/windows/helpers.rs (and corresponding mod declarations).
- Move, unchanged in behavior and signatures, under #[cfg(windows)]:
  - ps_quote_inner (private)
  - fork_ps_inner_string
  - fork_bash_inner_string
  - wt_orient_for_layout
  - wt_build_new_tab_args
  - wt_build_split_args
  - ps_wait_process_cmd
- Implement a private wt_tail(psbin, pane_dir, inner) -> Vec<String> to remove duplicate arg tails.
- Update fork_ps_inner_string and fork_bash_inner_string to render env from fork::env::fork_inner_env_kv (Phase 2).
- In lib.rs: re-export these Windows-only helpers under cfg(windows) to preserve aifo_coder::* paths:
  - #[cfg(windows)] pub use fork::windows::helpers::{...};
- Acceptance:
  - Existing unit tests validating Windows helpers pass under cfg(windows).
  - No change to preview args; "wt" remains argv[0].
  - fork_bash_inner_string still ends with “; exec bash”.

Phase 5 — Move fork merge enum to fork domain (low-risk)
- New module: src/fork/merge.rs OR reuse existing src/fork/types.rs.
- Move MergingStrategy (clap::ValueEnum) as-is (add use clap::ValueEnum in the new file).
- In lib.rs: pub use fork::merge::MergingStrategy; (or fork::types::MergingStrategy if placed there).
- Ensure re-export is not cfg-gated (cross-platform).
- Acceptance:
  - All call sites (e.g., src/fork/post_merge.rs, src/main.rs) continue compiling and behave identically.

Phase 6 — Extract fs/id helpers (low-risk)
- New module: src/util/fs.rs — move path_pair and ensure_file_exists.
- New module: src/util/id.rs — move create_session_id.
- In lib.rs: pub use util::fs::{path_pair, ensure_file_exists}; pub use util::id::create_session_id;
- Ensure no change in semantics (e.g., file creation and parent mkdir logic for ensure_file_exists).
- Acceptance:
  - All call sites compile; behavior identical.

Phase 7 — Slim lib.rs to a façade (moderate; churn contained)
- Remove implementation details moved in previous phases from lib.rs:
  - Windows helpers, UI warnings, MergingStrategy enum, fs/id helpers, and the large #[cfg(test)] module.
- Keep only:
  - mod declarations for apparmor, color, docker, fork, lock, registry, toolchain, util, and new ui module.
  - pub use re-exports for moved symbols to preserve the public API surface.
- Ensure Windows-only re-exports are behind #[cfg(windows)].
- Acceptance:
  - cargo test passes on Unix/macOS and Windows (CI matrix).
  - No public API breakage observed in downstream crates (if any).

Phase 8 — Test relocation and ownership (optional, incremental)
- Relocate unit tests to the modules they exercise (where practical and safe without heavy churn):
  - Windows helpers tests → src/fork/windows/helpers.rs
  - Lock path tests → src/lock.rs
  - HTTP/notifications tests → src/toolchain/{http.rs,notifications.rs}
  - Sidecar run/exec preview tests → src/toolchain/sidecar.rs
  - Fork clone/snapshot/merge tests → under src/fork/*
- Integration-crossing tests stay under tests/.
- Prefer importing via aifo_coder::* to validate public API stability.
- Acceptance:
  - Test assertions and expected strings remain unchanged.
  - Faster incremental builds due to reduced recompilation of lib.rs.

Cross-cutting correctness checks
- Strings, color, and streams:
  - Confirm all user-visible messages (stdout/stderr) remain byte-for-byte identical where applicable.
  - Confirm warn_prompt_continue_or_quit behavior is unchanged across OS branches (including two trailing newlines).
- Helper invariants:
  - Windows helpers maintain documented behaviors (no SUPPRESS var in inner strings; bash trailing exec preserved; "wt" argv[0] present).
- Environment and timing:
  - No changes to environment variable read/set timing described in Section M of the string inventory.
- Exit codes:
  - 127 error mapping and 1 for generic errors preserved.

Risks and mitigations
- Risk: API path changes if re-exports are missed or incorrectly gated.
  - Mitigation: implement re-exports first; compile after each phase; tests reference aifo_coder::* to surface issues.
- Risk: cfg(windows) complexities for helper modules and re-exports.
  - Mitigation: guard all Windows-only modules and re-exports with #[cfg(windows)]; ensure non-Windows builds compile without stubs.
- Risk: Subtle whitespace differences in warning prompts.
  - Mitigation: maintain exact eprintln!/eprint! sequences; centralize the two-newline emission via finish_prompt_line() but keep call sites identical.
- Risk: Test relocation churn can mask regressions if done wholesale.
  - Mitigation: defer test relocation to Phase 8; keep earlier phases code-only; relocate incrementally.

Backout strategy
- Each phase can be reverted independently if CI breaks.
- Re-exports ensure downstream stability; if a moved module causes issues, re-inline the specific functions into lib.rs temporarily while keeping the public API unchanged.

Acceptance criteria (global)
- cargo test passes across supported platforms (macOS/Linux/Windows).
- No change in user-visible strings, exit codes, or log routing.
- No change in helper behaviors documented in Phase 0 docs.
- Code structure adheres better to single-responsibility principles; lib.rs serves primarily as façade.

Implementation checklist (quick reference)
- Add fork::env::fork_inner_env_kv (no SUPPRESS) and use it in Windows inner builders.
- Create modules:
  - src/ui/warn.rs (warn_print, warn_prompt_continue_or_quit)
  - src/fork/windows/helpers.rs (Windows inner builders and wt helpers)
  - src/fork/merge.rs (MergingStrategy) OR place in src/fork/types.rs
  - src/util/fs.rs (path_pair, ensure_file_exists)
  - src/util/id.rs (create_session_id)
- toolchain/http.rs: remove local find_crlfcrlf; use crate::find_crlfcrlf (optional: crate::find_header_end).
- lib.rs: remove moved implementations; add cfg-gated re-exports for Windows helpers; re-export moved symbols.
