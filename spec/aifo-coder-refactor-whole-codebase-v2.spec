AIFO Coder — Whole Codebase Refactor v2
Comprehensive, phase‑optimized specification

Executive summary
- This specification consolidates all identified improvements into a phased plan
  optimized for low risk, fast feedback, and minimal disruption to user‑visible
  behavior and golden outputs.
- Focus areas: Windows orchestrators correctness (SUPPRESS injection), proxy
  bind configurability on Linux, default image alignment, prompt/input
  consistency, final error‑surface uniformity, tests helper consolidation, and
  hygiene (optionally Dockerfiles and metrics).
- Deliverables are grouped into tightly scoped phases with clear acceptance
  criteria, backout strategy, and guardrails for test/golden stability.

Objectives
- Maintain external behavior and user‑visible strings unless explicitly called
  out as optional and gated behind environment variables or docs only.
- Improve correctness and security configurability while keeping the codebase
  easy to review and test incrementally.
- Reduce duplication and align defaults with shipped images and docs.

Non‑goals
- No changes to proxy/shim protocols (v1/v2) or CLI flags by default.
- No dependency additions; keep footprint lean.
- No changes to golden strings or help texts unless clearly marked optional and
  off by default.

Architecture context (current)
- Library provides layered modules:
  - fork::*: repo detection, snapshot, clone, merge, orchestrators, notices.
  - toolchain::*: sidecar lifecycle, proxy/shim, routing/allowlists, HTTP.
  - util::*: shell/json escaping, URL decode, Docker security parsing, fs.
  - color.rs: color policy and log helpers (info/warn/error).
  - apparmor.rs and registry.rs: runtime detection and selection helpers.
- Binaries orchestrate CLI, banner/doctor, fork runner, and toolchain session.
- Tests include unit, integration, and E2E (some gated/skipped by env/host).

Detailed findings and improvement areas
1) Windows orchestrators: SUPPRESS injection for toolchain warnings
   - Files: src/fork/orchestrators/{windows_terminal.rs,powershell.rs,gitbash_mintty.rs}
   - Issue: They call fork_ps_inner_string/fork_bash_inner_string directly, which
     don’t inject AIFO_CODER_SUPPRESS_TOOLCHAIN_WARNING; only tmux path exports
     it via pane env. Windows panes may show toolchain warnings unintentionally.
   - Fix: Use fork::inner::{build_inner_powershell,build_inner_gitbash}, which
     perform SUPPRESS injection and preserve quoting/dir semantics.

2) Toolchain default image mismatch (Node)
   - File: src/toolchain/images.rs
   - Mismatch: Default node image is node:20-bookworm-slim while Dockerfile
     bases use node:22-bookworm-slim.
   - Fix: Align default to 22; update tests expecting 20 accordingly.

3) Proxy bind host configurability (Linux)
   - File: src/toolchain/proxy.rs
   - Current: Linux binds 0.0.0.0 to allow container access; no opt‑in to bind
     to loopback for tighter exposure.
   - Fix: Introduce AIFO_TOOLEEXEC_BIND_HOST env; default remains 0.0.0.0 on
     Linux, 127.0.0.1 elsewhere. Add doctor verbose tip (no standard text change).

4) Prompt/input consistency
   - Files: src/ui/warn.rs, src/warnings.rs
   - Current: ui/warn has robust single‑key prompt; warnings.rs sometimes uses
     ad‑hoc read_line for prompts (e.g., LLM credentials).
   - Fix: Route interactive prompts via warn_prompt_continue_or_quit for
     consistent UX; preserve exact message text and env suppression logic.

5) Error‑surface final uniformity
   - Files: fork_impl/*, toolchain/*, util/*
   - Current: Most io::Error::other strings now wrapped via display_for_*;
     ensure remaining edge cases are wrapped and exit codes map via centralized
     helpers (exit_code_for_*).
   - Fix: Final sweep and minor refactors only where string does not change.

6) Tests duplication and helper reuse
   - Files: tests/* (various)
   - Current: Several tests inline which/have_git/init_repo helpers; a shared
     tests/support/mod.rs exists.
   - Fix: Consolidate test helpers to tests/support; keep “skipping:” messages
     identical and test‑controlled.

7) Hygiene and consistency
   - Files: various
   - Replace direct ANSI prints where identical color helper exists; run rustfmt
     and clippy over non‑golden code; keep dead_code markers only when justified.
   - Keep README “make check” guidance aligned with Makefile nextest usage.

8) Optional observability (proxy metrics)
   - File: src/toolchain/proxy.rs
   - Add gated counters/timing summaries when AIFO_PROXY_METRICS=1 and verbose
     is true. Default off; no user‑visible changes otherwise.

9) API provider hint refinement (optional)
   - File: src/docker.rs
   - Current: Sets OPENAI_API_TYPE=azure whenever AIFO_API_BASE exists; base
     might not be Azure.
   - Fix: Add optional AIFO_API_PROVIDER=azure gate (or heuristic) while
     retaining current default behavior unless explicitly set; off by default.

10) Dockerfiles hygiene (optional)
   - Files: toolchains/rust/Dockerfile, toolchains/cpp/Dockerfile
   - Rust: docker.io likely unnecessary in sidecar; reconsider installing it.
     World‑writable perms (0777) for home/cargo simplify uid mapping but carry
     risk; keep functional intent but prefer runtime chown where feasible.
     Clarify PATH vs CARGO_HOME precedence in comments.
   - C++: Base looks fine; retain ccache; keep CA secret comments; apt cleanup OK.

Risk, impact, and guardrails
- Message stability: Do not change user‑visible strings unless optional and
  gated; tests rely on exact text.
- Connectivity: Proxy bind default remains unchanged; loopback is opt‑in only.
- Windows orchestrators: Use existing inner builders to minimize risk and
  preserve exact command shapes.
- Tests: Update only those expecting legacy node:20; keep skip messages.

Performance and security considerations
- Bind override improves host exposure choices; default unchanged maintains
  current behavior.
- No new allocations/hot‑path changes expected; native proxy logic remains
  efficient; metrics (if enabled) use lightweight counters.
- Dockerfile hygiene (optional) reduces surface by removing unneeded packages.

Deliverables per phase (optimized plan)
Phase 1 — Correctness and default alignment (low risk, high value)
A1. Orchestrators SUPPRESS injection (Windows)
    - Replace inner string builders:
      - windows_terminal.rs: build_inner_powershell()
      - powershell.rs: build_inner_powershell()
      - gitbash_mintty.rs: build_inner_gitbash(exec_shell_tail based on selection)
    - Tests: Windows‑only unit test stubs (cfg(windows)) to assert inner builders
      used; existing smoke tests suffice for functional verification.
    - Acceptance: Windows panes no longer show toolchain warnings when they
      should be suppressed; no message changes.

A2. Node default image alignment
    - Update default_toolchain_image(node) -> node:22-bookworm-slim.
    - Update tests that assert unknown kind fallback (node:20 -> node:22).
    - Acceptance: All tests green; README alignment (already node:22).

A3. Error‑surface audit final pass
    - Sweep fork_impl/* and toolchain/* for io::Error::other sites and wrap
      through display_for_* helpers; exit codes through exit_code_for_* helpers.
    - No string changes; no behavior drift.
    - Acceptance: Lints/tests green; no golden diffs.

Backout strategy for Phase 1:
- Revert orchestrator changes independently by flipping inner builder calls.
- Revert node default to 20 if regressions arise; restore tests.

Phase 2 — Security configurability (Linux)
B1. Proxy bind host override
    - Add AIFO_TOOLEEXEC_BIND_HOST; use when set; otherwise default to 0.0.0.0
      on Linux, 127.0.0.1 elsewhere (existing behavior preserved).
    - Doctor (verbose) tip: Document how/why to use loopback binding; no change
      to standard (non‑verbose) outputs.
    - Acceptance: Bind address honored; container connectivity intact; tests &
      smokes pass.

Backout:
- Remove env override path; retain defaults.

Phase 3 — Prompt/input consistency
C1. warnings.rs prompt refactor
    - Route interactive prompts (e.g., LLM credentials prompt) through
      ui::warn::warn_prompt_continue_or_quit using the existing text lines.
    - Preserve suppression env semantics and message text.
    - Acceptance: UX consistent; tests unaffected; no golden changes.

Backout:
- Switch prompt back to line‑read if needed.

Phase 4 — Tests consolidation
D1. tests helper reuse
    - Replace local helpers in target tests with tests/support/mod.rs:
      have_git(), which(), init_repo_with_default_user().
    - Preserve exact “skipping:” messages and behavior.
    - Acceptance: Tests remain green; duplication reduced.

Backout:
- Re‑introduce local helper in a specific test if platform quirk emerges.

Phase 5 — Hygiene (non‑functional)
E1. Color/log helpers adoption
    - Replace inline ANSI where identical helper exists (log_info_stderr,
      log_warn_stderr, log_error_stderr, paint), strictly preserving text.
E2. rustfmt/clippy over non‑golden code; keep dead_code allowances where needed.
    - Acceptance: Lints clean; no behavior/string changes.

Backout:
- Revert specific helper swaps that prove noisy or risky.

Phase 6 — Optional refinements
F1. Proxy metrics (gated)
    - Implement AIFO_PROXY_METRICS=1 to print counters/timings only in verbose
      mode; default off; no behavior change otherwise.
F2. API provider hint refinement (gated)
    - Support AIFO_API_PROVIDER=azure to set OPENAI_API_TYPE=azure explicitly.
F3. Dockerfiles hygiene (toolchains)
    - Consider removing docker.io from rust toolchain image unless justified.
    - Clarify PATH/CARGO_HOME comments; preferentially keep runtime chown flow.
    - Acceptance: Build/test unaffected; images slimmer where applicable.

Backout:
- Remove metrics prints; ignore provider env; retain installed packages.

Acceptance criteria (global)
- All unit/integration tests pass (incl. platform‑gated and smokes where
  prerequisites exist).
- Golden outputs unchanged (list/clean/merge/doctor/banner/etc.).
- Windows panes suppress toolchain warnings across orchestrators.
- Proxy supports optional loopback bind without disrupting defaults.
- Defaults aligned with Dockerfile bases (Node 22).
- Error surfaces uniformly wrapped; exit codes standardized.
- Reduced test helper duplication; lints clean on non‑golden code.

Milestones and sequencing
- M1 (Phase 1): 1–2 days, PR 1–2 (orchestrators + defaults; audit fixes).
- M2 (Phase 2): 0.5–1 day, PR 3 (bind override + doc tip).
- M3 (Phase 3): 0.5 day, PR 4 (prompt refactor).
- M4 (Phase 4): 0.5–1 day, PR 5 (tests consolidation).
- M5 (Phase 5): 0.5 day, PR 6 (hygiene).
- M6 (Phase 6, optional): as approved, small PRs for metrics/provider/Dockerfiles.

Backwards compatibility and roll‑forward plan
- Each phase is independently revertible.
- Defaults and strings remain stable; optional features are env‑gated.
- Roll‑forward guarded by make check (nextest), platform smokes, and focused
  reviews per PR.

Appendix A — File‑level change map (targets)
- src/fork/orchestrators/windows_terminal.rs: use build_inner_powershell
- src/fork/orchestrators/powershell.rs: use build_inner_powershell
- src/fork/orchestrators/gitbash_mintty.rs: use build_inner_gitbash
- src/toolchain/images.rs: node default -> node:22-bookworm-slim
- src/toolchain/proxy.rs: AIFO_TOOLEEXEC_BIND_HOST (Linux) + optional metrics
- src/warnings.rs: route prompts via ui::warn helpers
- fork_impl/*, toolchain/*: final error‑surface audit wrap via display_for_*
- tests/*: consolidate helpers into tests/support/mod.rs
- toolchains/rust/Dockerfile (optional): consider removing docker.io; clarify
  PATH/CARGO_HOME and permissions comments
- toolchains/cpp/Dockerfile (optional): confirm minimal hygiene comments

Appendix B — Test plan (high level)
- Unit: orchestrators selection (Windows), image defaults, bind override
- Integration: fork smokes (tmux/wt), notifications policy unchanged
- Doctor: verify verbose bind tip present only when expected
- Dockerfiles (optional): local image builds if CI path exists

Appendix C — Out‑of‑scope candidates (backlog)
- CONTRIBUTING.md for contributors (run tests/lints; platform notes)
- hadolint target for Dockerfiles
- Proxy metrics export hooks (beyond stderr) for future telemetry
