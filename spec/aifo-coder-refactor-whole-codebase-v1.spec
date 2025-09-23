AIFO Coder: Whole-Codebase Refactor and Optimization v1
=======================================================

Overview
- Purpose: Raise maintainability, consistency, and correctness across the entire codebase (binary, library, orchestrators, proxy/shim, and tests) without changing user-visible behavior unless explicitly allowed.
- Strategy: A phased, low-risk plan that consolidates duplicated utilities, completes orchestrator abstractions, standardizes error handling/logging, and enhances test ergonomics.
- Constraints: Preserve exact user-facing strings and outputs where golden tests rely on them; prefer incremental commits that keep CI green.

Goals
- Complete and unify orchestrator architecture; move runner logic into orchestrators.
- Eliminate duplicated utilities; centralize helpers.
- Standardize error types and exit-code mapping; unify color-aware logging.
- Improve input/prompt handling and documentation without changing strings.
- Enhance test structure and reuse; add missing targeted tests.
- Maintain behavior parity; ensure all existing tests remain green.

Non-goals
- Introduce breaking CLI changes.
- Alter golden output strings for tests, unless guarded and updated consistently.
- Add heavy dependencies or complex frameworks.

Key Findings and Improvement Areas

1) Orchestrators (incomplete abstractions)
- Files: src/fork/runner.rs; src/fork/orchestrators/{mod.rs,tmux.rs,powershell.rs,gitbash_mintty.rs,windows_terminal.rs}.
- Issues:
  - Placeholders exist (tmux.rs, powershell.rs, gitbash_mintty.rs); majority of logic lives in runner.rs.
  - src/fork/orchestrators/mod.rs is gated with #![cfg(windows)], yet references non-Windows variants (Selected::Tmux). Architecture is awkward and split by cfg in ways that hinder reuse.
- Actions:
  - Implement full orchestrators:
    - Unix: Tmux: new-session, split panes, layout, send-keys, attach/switch.
    - Windows Terminal: new-tab and split-pane; non-waitable.
    - PowerShell: Start-Process with PID capture; waitable; merge-friendly.
    - Git Bash/mintty: bash -lc inner; mintty wrapper; conditional exec-tail trimming.
  - Unified trait Orchestrator with launch()/supports_post_merge().
  - Cross-platform selection (select_orchestrator) compiled on all platforms, with platform-appropriate behavior.

2) Utility duplication and fragmentation
- Files: src/docker.rs (path_pair/ensure_file_exists), src/util/fs.rs, repeated have_git/init_repo patterns across tests.
- Issues:
  - Duplicated helpers; mixed usage increases maintenance burden.
- Actions:
  - Replace docker-local path_pair/ensure_file_exists with util::fs versions across codebase.
  - Add tests/common helpers (have_git, init_repo_with_default_user, which) to reduce repetition and improve clarity.

3) Error handling and exit codes
- Files: src/main.rs, src/fork/runner.rs, src/toolchain/*, src/fork_impl/*.
- Issues:
  - Frequent use of io::Error::other and ad-hoc eprintln!/println!; inconsistent exit code mappings.
- Actions:
  - Introduce error enums per subsystem:
    - ForkError, ToolchainError, ProxyError.
  - Centralize exit-code mapping in binary (main.rs / commands/mod.rs).
  - Preserve current message strings; only standardize the mapping flow.

4) Logging consistency and color usage
- Files: src/color.rs, mixed prints across modules.
- Issues:
  - Inline color codes scattered across modules; minor duplication.
- Actions:
  - Keep paint() and color mode; add a minimal logger-like helper or conventions doc to reduce duplication without changing content.

5) UI prompt handling (platform-dependent)
- Files: src/ui/warn.rs.
- Issues:
  - Platform-specific single-key input logic is correct but monolithic and harder to read.
- Actions:
  - Extract platform-specific input readers into helpers warn_input_unix(), warn_input_windows(), warn_input_fallback(); keep strings and logic identical.

6) Runner decomposition
- Files: src/fork/runner.rs.
- Issues:
  - Large, monolithic file with duplicated sequences (Windows flows).
- Actions:
  - Decompose fork_run into orchestrator-independent steps:
    - preflight; base determination; snapshot; cloning; metadata; orchestrator dispatch; guidance; post-merge.
  - Move orchestration mechanics into orchestrator modules.

7) Metadata writing/parsing
- Files: src/fork/meta.rs; src/fork.rs; src/fork_impl/*.
- Observations:
  - Manual JSON writers used to preserve key order; minimal parsers are correct but should have clear tests.
- Actions:
  - Keep manual writers; add tests for extract_value_string/extract_value_u64 edge cases (already partial).
  - Keep append_fields_compact and ensure consistent usage.

8) Notifications policy validation consolidation
- Files: src/toolchain/notifications.rs.
- Issues:
  - Validation duplicated between parse_notifications_command_config() and parse_notif_cfg(); risk of divergence.
- Actions:
  - Limit parse_notifications_command_config() to tokenization (YAML->argv tokens).
  - Enforce policy (absolute path, trailing {args}) exclusively in parse_notif_cfg(); proxy uses parse_notif_cfg() whenever applicable.

9) Docker security options parsing
- Files: src/banner.rs, src/doctor.rs.
- Issues:
  - Duplicated parsing logic for Docker security options.
- Actions:
  - Add a helper (e.g., docker_security_options_parse) to parse and report apparmor/seccomp/cgroupns/rootless; reuse in banner and doctor.

10) Toolchain/proxy/shim cohesion
- Files: src/toolchain/{proxy.rs,shim.rs,http.rs,auth.rs,notifications.rs}.
- Observations:
  - Good alignment on signals, trailers, exit semantics; logging consistent with specs.
- Actions:
  - Keep current behavior; verify edge cases with additional tests; continue to separate responsibilities.

11) Tests structure and ergonomics
- Files: tests/*.rs.
- Issues:
  - Repeated env checks and helpers.
- Actions:
  - Introduce shared test helpers; unify skip message styles; add targeted tests for orchestrator selection and warn input helpers.

12) Style and readability
- Observations:
  - Some long lines (>100 characters) especially in format strings/logs; acceptable where required by golden tests.
- Actions:
  - Prefer <100 chars where feasible; strictly preserve tested strings.

Acceptance Criteria
- All existing tests remain green (no regressions in golden outputs).
- Orchestrator modules fully implement behavior previously in runner.rs; runner delegates orchestration.
- Utilities consolidated; duplicated helpers removed.
- Error handling standardized; exit-code mapping consistent and documented.
- UI prompt refactor uses helper functions; behavior and strings unchanged.
- Documentation improved (module headers and DESIGN-style overview); no external behavior changes.

Risks and Mitigations
- Risk: changing string outputs breaks goldens.
  - Mitigation: retain all user-visible text verbatim; refactor structure only.
- Risk: orchestration migration introduces control-flow regressions.
  - Mitigation: phased migration per orchestrator; unit tests; manual verification of logs.
- Risk: utility consolidation changes imports.
  - Mitigation: stepwise search/replace; CI per phase.

Detailed Phase-Optimized Plan

Phase 1: Foundations and Low-Risk Refactors
- Consolidate utilities:
  - Replace docker.rs path_pair/ensure_file_exists calls with util::fs::{path_pair,ensure_file_exists}.
- Add Docker security options parsing helper:
  - Introduce a function to parse Docker info JSON and extract apparmor/seccomp/cgroupns/rootless; reuse in banner.rs and doctor.rs.
- Refactor ui/warn input handling:
  - Extract platform-specific input readers into small functions; retain messages, logic, and environment checks.
- Establish error enums and exit-code mapping:
  - Define ForkError/ToolchainError/ProxyError in their respective modules; map to exit codes centrally in main/commands.
- Tests/common helpers:
  - Create test helpers (have_git, init_repo_with_default_user, which) for reuse; update tests incrementally.
- Deliverables:
  - No behavior changes; CI green; small CHANGES.md entries describing internal refactors.

Phase 2: Orchestrators Complete and Cross-Platform Selection
- Implement orchestrators:
  - tmux (Unix): move full session/pane/layout/send-keys/attach logic from runner.rs.
  - Windows Terminal (Windows): implement preview and spawn behavior; mark non-waitable; retain guidance prints.
  - PowerShell (Windows): implement Start-Process flows, PID capture, Wait-Process; merge-friendly.
  - Git Bash/mintty (Windows): implement bash -lc inner string + mintty wrapper; trimming exec-tail when post-merge requested.
- Cross-platform selection:
  - Rework select_orchestrator to be compiled on all platforms; consistent decision tree with platform-appropriate outputs and env flags.
- Runner delegation:
  - Move orchestration logic out of runner.rs and call orchestrators via trait; keep meta writes and guidance unchanged.
- Deliverables:
  - Green tests; identical strings; CHANGES.md describing internal movement.

Phase 3: Notifications Policy Consolidation and Proxy Alignments
- Consolidate validation:
  - parse_notifications_command_config(): tokenization only.
  - parse_notif_cfg(): enforce absolute exec path and trailing {args}; proxy uses parse_notif_cfg() outputs.
- Verify timeout and exit semantics; minor test additions for policy edge cases.
- Deliverables:
  - Green tests; CHANGES.md describing validation refactor; no user-visible changes.

Phase 4: Error/Logging Refinement and Runner Decomposition
- Runner decomposition:
  - Split into orchestrator agnostic steps; helpers for metadata writing and post-merge application.
- Logging consistency:
  - Optionally add a minimal internal logging helper to consolidate paint() usage in frequent patterns; DO NOT change text.
- Deliverables:
  - Green tests; smaller runner.rs focused on coordinator responsibilities; CHANGES.md updated.

Phase 5: Documentation and Style
- Documentation:
  - Module-level docs for orchestrators and major refactors.
  - DESIGN-style overview documenting orchestration flows, proxy/shim interactions, and environment variables.
- Style:
  - Reduce long lines where safe; preserve golden strings untouched.
- Deliverables:
  - Documentation in repo; CHANGES.md describing doc additions.

Backward Compatibility & Testing Strategy
- Keep existing messages and enums/states; unit tests updated to use helpers where appropriate.
- Add new tests:
  - Orchestrator selection (Windows and Unix paths via env simulation).
  - ui/warn input helper coverage (platform-gated).
  - Notifications validation under single-source policy.

Exit-Code Mapping (standardization guide)
- 0: success
- 1: generic failure
- 86: proxy/shim not configured (consistent with shim exiting 86)
- 124: timeout
- 127: missing command/tool (e.g., docker, tmux)

Environment Variable Notes
- Maintain AIFO_TOOLEEXEC_* spelling; optional transitional aliases can be considered later.
- Confirm documentation for:
  - AIFO_CODER_FORK_STATE_BASE / AIFO_CODER_FORK_STATE_DIR
  - AIFO_SESSION_NETWORK / AIFO_TOOLEEXEC_ADD_HOST (Linux)
  - AIFO_NOTIFICATIONS_* (allowlist, max args)
  - AIFO_RUST_* (bootstrap, sccache)
  - Color: AIFO_CODER_COLOR, NO_COLOR

Timeline & Sequencing (suggested)
- Phase 1: 1–2 days; CI green needed to proceed.
- Phase 2: 3–5 days; migration per orchestrator with thorough verification.
- Phase 3: 1–2 days; consolidation and tests.
- Phase 4: 2–3 days; runner decomposition and logging consistency.
- Phase 5: 1 day; docs and minor style fixes.

Out-of-Scope (for v1)
- Introducing structured JSON logging for proxy (can be a follow-up).
- Changing auth/proto semantics beyond validation consolidation.
- Widening notifications beyond current allowlist rules.

Appendix: References
- Orchestrators: src/fork/orchestrators/*, src/fork/runner.rs.
- Utilities: src/util/fs.rs, src/docker.rs.
- Prompt/UI: src/ui/warn.rs.
- Proxy/Shim: src/toolchain/{proxy.rs,shim.rs,http.rs,auth.rs,notifications.rs}.
- Metadata: src/fork/meta.rs.
- Tests: tests/*.rs; add tests/common.rs for shared helpers.
