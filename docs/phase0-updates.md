AIFO Coder — Phase 0 updates: additional string inventory and helper verification notes

Scope
- Complement docs/phase0-string-inventory-main.md with strings observed in src/main.rs that were not explicitly listed.
- Reconfirm helper availability and behaviors in src/lib.rs for later phases.

Additional user-visible strings in src/main.rs

- Generic fork orchestration guard (stderr):
  - "aifo-coder: no panes to create."

- Windows Terminal non-waitable fallback (stderr):
  - "aifo-coder: warning: one or more Windows Terminal panes failed to open."

Placement guidance for the main inventory
- Add "aifo-coder: no panes to create." under:
  - C) Fork preflights and generic errors (stderr)
  - H) Tmux path-specific messages (stderr) — the same message is used before tmux orchestration when no panes exist.

- Add "aifo-coder: warning: one or more Windows Terminal panes failed to open." under:
  - D) Windows orchestrators — selection, launch, fallback (stderr/stdout; exact text)
    - In the non-waitable Windows Terminal fallback subsection (printed when split-pane fails during the best-effort, non-waiting launch).

Helper verification re-check (src/lib.rs)
- Orchestrator inner builders:
  - fork_ps_inner_string(...) and fork_bash_inner_string(...) exist and do not include AIFO_CODER_SUPPRESS_TOOLCHAIN_WARNING=1.
    - Bin-side inner builders must inject this variable in later phases as specified.
  - fork_bash_inner_string(...) ends with "; exec bash" — must be trimmed when a post-merge strategy is requested.

- Windows Terminal helpers:
  - wt_orient_for_layout(layout, i) returns "-H"/"-V".
  - wt_build_new_tab_args(...) and wt_build_split_args(...) include "wt" as argv[0].
    - Keep full vectors for preview strings.
    - When executing with Command::new(wt_path), drop the first "wt" element.

- Waiting helper:
  - ps_wait_process_cmd(ids) returns "Wait-Process -Id …" as expected.

- Merge/clean helpers and base functions:
  - fork_merge_branches_by_session/fork_clean present and used by main.rs.
  - repo_root(), fork_base_info(), fork_create_snapshot(), fork_clone_and_checkout_panes() present.

- Base commit SHA behavior in main.rs (for metadata) is implemented as required:
  - Prefer snapshot SHA when created; else rev-parse --verify base_ref_or_sha; else fallback to HEAD SHA from fork_base_info().

Notes
- Guidance combinations in print_inspect_merge_guidance calls match the inventory:
  - Git Bash/mintty: include_remote_examples=true, extra_spacing_before_wrapper=false.
  - Windows Terminal: include_remote_examples=false, extra_spacing_before_wrapper=true.
  - PowerShell: include_remote_examples=false, extra_spacing_before_wrapper=true.
  - Tmux: include_remote_examples=false, extra_spacing_before_wrapper=true; use_color_header based on stdout TTY.

Phase 0 verification notes (documentation-only)
- Additional strings listed here (“no panes to create”, Windows Terminal split-pane fallback warning) are confirmed present and unchanged.
- Helper availability cross-check:
  - Windows-only helpers are cfg-gated; non-Windows builds compile without resolving them.
  - Notifications helpers and HTTP form parsing remain available via aifo_coder:: toolchain façade wrappers.
- No discrepancies found between these updates and the main inventory; subsequent phases must preserve these strings verbatim.
