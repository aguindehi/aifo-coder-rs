AIFO Coder: Whole-Codebase Refactor and Optimization v1
=======================================================

Overview
- Purpose: Raise maintainability, consistency, correctness and testability across the codebase (binary, library, orchestrators, proxy/shim, and tests) without changing user-visible behavior unless explicitly allowed.
- Strategy: Low-risk phased refactor. Consolidate duplicated utilities, complete orchestrator abstractions, standardize error handling and exit codes, unify logging and prompts, improve test ergonomics, and add targeted coverage.
- Constraints: Preserve exact user-facing strings and outputs where golden tests rely on them. Avoid invasive changes in hot paths. Keep dependency footprint minimal.

Goals
- Complete and unify orchestrator architecture; extract runner logic into orchestrators.
- Eliminate duplicated utilities; centralize helpers under util::* and reuse across modules.
- Standardize error types and exit-code mapping; unify color-aware logging and prompt flows.
- Consolidate notifications policy validation; avoid split enforcement.
- Improve doctor/banner security parsing with a reusable helper.
- Strengthen tests: reduce duplication, add missing coverage for critical edge cases.
- Maintain behavior parity; ensure all existing tests remain green.

Non-goals
- Breaking CLI changes.
- Altering golden output strings (unless guarded by tests and updated consistently).
- Adding heavy dependencies or frameworks.

Comprehensive Findings and Improvement Areas

1) Orchestrators: incomplete abstractions and platform gating
- Files: src/fork/runner.rs; src/fork/orchestrators/{mod.rs,tmux.rs,powershell.rs,gitbash_mintty.rs,windows_terminal.rs}.
- Issues:
  - Tmux, PowerShell, Git Bash/mintty orchestrator modules are placeholders; WT has a stub launch.
  - src/fork/runner.rs contains very large monolithic orchestration logic, with significant duplication across Windows flows (Git Bash and mintty paths repeated).
  - src/fork/orchestrators/mod.rs is gated with #![cfg(windows)], contains mixed cfg for non-Windows (Selected::Tmux) but isn’t compiled cross-platform; selection isn’t integrated.
- Actions:
  - Implement orchestrators:
    - Tmux (Unix): new-session, split panes, layout, send-keys, attach/switch (extract from runner.rs).
    - Windows Terminal: new-tab and split-pane builder/executor, non-waitable orchestration, guidance prints delegated.
    - PowerShell: Start-Process per-pane with PID capture, Wait-Process support, merge-friendly flow.
    - Git Bash/mintty: bash -lc inner string with SUPPRESS injection, mintty wrapper; trim "; exec bash" when post-merge requested.
  - Make selection function compiled on all platforms and return selected orchestrator with an explicit reason.
  - Runner should delegate launch() to orchestrators; supports_post_merge() gates post-merge behavior.

2) Utility duplication and fragmentation
- Files: src/docker.rs (path_pair, ensure_file_exists), src/util/fs.rs; tests have repeated have_git/which/init_repo.
- Issues:
  - path_pair and ensure_file_exists are duplicated in docker.rs; identical helpers exist in util/fs.rs.
  - Tests repeatedly reimplement have_git(), which(), init_repo() across many files.
- Actions:
  - Replace docker-local path_pair/ensure_file_exists with util::fs::{path_pair, ensure_file_exists}.
  - Introduce tests/common.rs (or tests/support/mod.rs) exporting:
    - have_git()
    - init_repo_with_default_user(dir)
    - which(bin) cross-platform wrapper (which/where)
  - Update tests to import these helpers; keep identical skip messages and behavior.

3) Error handling and exit codes
- Files: widespread (src/main.rs, src/fork/runner.rs, src/toolchain/*, src/fork_impl/*).
- Issues:
  - Inconsistent usage of io::Error::other with ad-hoc string payloads; exit codes scattered (sometimes 1, sometimes 127).
  - Proxy/shim use structured mapping but other subsystems do not.
- Actions:
  - Introduce lightweight error enums per subsystem:
    - ForkError
    - ToolchainError
    - ProxyError (already well-structured internally)
  - Centralize exit-code mapping in binary-side glue (main.rs and commands/mod.rs), preserving current visible messages.
  - Where io::Error::other is used for sentinel cases, wrap into the appropriate error enum variant internally and map to ExitCode.

4) Logging and color usage consistency
- Files: src/color.rs; scattered eprintln!/println! with ANSI codes.
- Issues:
  - Colorization is correct but duplicated across modules with inline ANSI sequences; small inconsistencies in repeated patterns.
- Actions:
  - Keep paint() and color mode; add a minimal internal helper or conventions doc for common patterns:
    - info(), warn(), error() wrappers that format via paint() but retain exact strings (no text changes). Use only where message text is fully identical.

5) UI prompt handling refactor
- Files: src/ui/warn.rs; uses platform-specific input logic inline.
- Issues:
  - Monolithic function mixes UNIX stty and Windows getch logic; harder to test/maintain.
- Actions:
  - Extract platform-specific single-key input readers into helpers:
    - warn_input_unix()
    - warn_input_windows()
    - warn_input_fallback()
  - Keep logic and strings identical; retain environment checks (AIFO_CODER_NO_WARN_PAUSE, CI).

6) Runner decomposition
- Files: src/fork/runner.rs.
- Issues:
  - Very large control flow; duplicated paths on Windows; orchestration and meta writing intertwined.
- Actions:
  - Decompose into orchestrator-agnostic steps:
    - Preflight checks
    - Base determination and snapshot
    - Clone and checkout
    - Metadata writing (delegate to crate::fork::meta)
    - Orchestrator selection and launch
    - Post-merge application
    - Guidance printing and cleanup
  - Move platform-specific orchestration mechanics into orchestrator modules; keep runner coordinating.

7) Notifications policy validation consolidation
- Files: src/toolchain/notifications.rs and public wrapper in src/toolchain.rs.
- Issues:
  - Both parse_notifications_command_config() and parse_notif_cfg() enforce policy (absolute exec path, trailing {args}). Split enforcement increases drift risk.
- Actions:
  - Limit parse_notifications_command_config() strictly to tokenization (YAML -> argv tokens).
  - Enforce policy in parse_notif_cfg() only; make the proxy and public wrapper use parse_notif_cfg() outputs for validation.
  - Ensure error texts remain identical to current ones; adjust wrapper mapping.

8) Doctor/banner Docker security parsing duplication
- Files: src/banner.rs and src/doctor.rs.
- Issues:
  - Both parse Docker info SecurityOptions JSON-ish output with local string scanning; duplicated logic.
- Actions:
  - Introduce helper docker_security_options_parse(raw_json_str) returning a struct:
    - has_apparmor, seccomp_profile, cgroupns_mode, rootless
  - Reuse in banner.rs and doctor.rs; preserve printed strings.

9) Toolchain routing and container checks
- Files: src/toolchain/routing.rs.
- Issues:
  - The tool availability check (tool_available_in) spawns threads for simple exec checks; minimal but could be standardized via a timeout util.
- Actions:
  - Consider a tiny timeout wrapper util for subprocess polling; ensure consistency with existing practice. Preserve behavior and return semantics.

10) Tests coverage and ergonomics
- Files: tests/*.rs.
- Issues:
  - Repeated have_git/which/init_repo patterns; missing targeted coverage for orchestrator selection on non-Windows builds (currently orchestrators/mod.rs is cfg(windows)).
- Actions:
  - Consolidate test helpers as per item 2.
  - Add orchestrator selection tests gated by platform:
    - Unix: Tmux selection path
    - Windows: WT/PowerShell/Git Bash paths via env flags (already present in orchestrators/mod.rs tests; expand scope once selection compiles cross-platform).
  - Add tests around warn prompt helpers and notifications policy consolidation.

11) Style and readability
- Observations:
  - Some lines exceed 100 characters (intentional for golden outputs); code paths with long format strings are acceptable.
- Actions:
  - Prefer shorter lines where feasible in non-golden code; keep golden-sensitive strings exactly as-is.

12) Minor correctness and hygiene items
- Ensure a consistent mapping of AIFO_TOOLEEXEC_* variables across docker, toolchain_session, shim and proxy (currently consistent; document invariants).
- Audit duplicated Windows flows in runner.rs (Git Bash/mintty) and consolidate repeated inner logic via orchestrators.
- Consider small helper for which() availability checks to remove scattered which::which usage duplicates with unix/windows variants.

Acceptance Criteria
- All existing tests remain green.
- Orchestrators implemented and integrated; runner delegates orchestration; selection compiled cross-platform.
- Utilities consolidated; duplicated helpers removed; docker.rs reuses util::fs.
- Notifications policy strictly enforced in one place; wrappers adjusted.
- Security options helper reused in doctor and banner; identical outputs preserved.
- UI warn prompt refactored into helpers with identical behavior.
- Documentation updated; small module-level docs added where needed.

Risks and Mitigations
- Changing strings breaks goldens.
  - Mitigation: retain strings verbatim; refactor structure only; guard with tests.
- Orchestrator migration introduces control-flow regressions.
  - Mitigation: phased migration per orchestrator; unit tests; manual verification of logs.
- Utility consolidation changes imports.
  - Mitigation: stepwise search/replace; compile and run tests per phase.

Phased Plan (optimized)

Phase 1: Utility consolidation and low-risk refactors
- Replace docker.rs local path_pair/ensure_file_exists with util::fs equivalents.
- Extract UI prompt input helpers in ui/warn.rs, keeping strings intact.
- Add docker_security_options_parse helper; reuse in banner.rs and doctor.rs.
- Create tests/support module:
  - have_git(), init_repo_with_default_user(), which() cross-platform
  - Update existing tests to import helpers.
- Establish error enums (ForkError, ToolchainError) and central exit-code mapping in main.rs and commands/mod.rs, preserving printed messages.
- Deliverables: CI green; no behavior changes; small CHANGES entry.

Phase 2: Orchestrators implementation and selection integration
- Implement:
  - Tmux orchestrator: new-session, split, layout, send-keys, attach/switch (extracted from runner.rs).
  - Windows Terminal orchestrator: build new-tab/split-pane args, execute with wt path; mark non-waitable.
  - PowerShell orchestrator: Start-Process with PID capture and optional waiting; merge-friendly.
  - Git Bash/mintty orchestrator: bash -lc inner (SUPPRESS injection), mintty wrapper; trim exec tail when requested.
- Rework select_orchestrator to compile on all platforms; return Selected variant with reason.
- Make runner delegate all platform-specific orchestration to orchestrators.
- Deliverables: identical strings and flows; runner simplified; CI green.

Phase 3: Notifications policy consolidation
- Restrict parse_notifications_command_config() to tokenization.
- Move absolute exec path + trailing {args} placeholder policy into parse_notif_cfg() only.
- Update proxy and wrapper to use parse_notif_cfg() for policy checks.
- Add tests for edge cases (absolute path, misplaced {args}, extra placeholders).
- Deliverables: green tests; unchanged error texts; CHANGES entry.

Phase 4: Error/logging refinement and runner decomposition
- Finalize error enums mapping; refactor ad-hoc io::Error::other sites.
- Add tiny logger helpers or conventions doc to reduce inline ANSI duplication without changing text.
- Split runner into clear orchestration steps and meta writing; ensure guidance and post-merge flows are unchanged.
- Deliverables: smaller runner.rs; consistent logging; CI green.

Phase 5: Documentation and style
- Add module-level docs for orchestrators, runner and security options parsing.
- Document environment invariants (AIFO_TOOLEEXEC_*, AppArmor profile selection).
- Prefer shorter lines where safe; preserve golden strings.
- Deliverables: documentation added; no behavior changes.

Backward Compatibility and Test Strategy
- Keep all messages and behaviors; expand tests using shared helpers.
- Add orchestrator selection tests for Unix and Windows (env-flag driven).
- Add warn prompt helper tests (platform-gated).
- Add notifications validation tests for consolidated policy.

Exit-Code Mapping Guide (unchanged externally, centralized internally)
- 0: success
- 1: generic failure/refusal
- 86: proxy/shim not configured (consistent with shim)
- 124: timeout
- 127: missing command/tool (e.g., docker, tmux)

Environment Variables Invariants (document and validate)
- AIFO_TOOLEEXEC_URL/TOKEN: propagated across toolchain_session, docker, shim, proxy.
- AIFO_SESSION_NETWORK/AIFO_TOOLEEXEC_ADD_HOST: session network and Linux host-gateway behavior.
- AIFO_NOTIFICATIONS_*: allowlist, max args, config path; strict policy in parse_notif_cfg().
- AIFO_RUST_*: bootstrap, sccache, linker flags; sidecar env application.
- Color: AIFO_CODER_COLOR and NO_COLOR respected globally.

Appendix: Notable code areas to refactor or implement
- src/fork/runner.rs: decompose and delegate to orchestrators; eliminate repeated Windows flows.
- src/fork/orchestrators/*: implement tmux, windows_terminal, powershell, gitbash_mintty; integrate selection.
- src/docker.rs: remove local path_pair/ensure_file_exists; reuse util::fs; keep behavior and previews.
- src/ui/warn.rs: extract input helpers; keep exact text and logic.
- src/toolchain/notifications.rs: consolidate policy in parse_notif_cfg(); wrappers use consolidated policy.
- src/banner.rs and src/doctor.rs: reuse docker security parsing helper; preserve output strings.
- tests: introduce tests/support helpers; update existing tests; add orchestrator and prompt coverage.

Success Metrics
- Reduced code duplication in orchestrators and utilities.
- Runner simplified with clear orchestration phases.
- Centralized error mapping; consistent exit codes; unchanged user-facing strings.
- Tests pass with added coverage; reduced maintenance overhead.
