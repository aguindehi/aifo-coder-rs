AIFO Coder: Whole-Codebase Refactor and Optimization v2
=======================================================

Context and scope
- This v2 specification builds on v1 (spec/aifo-coder-refactor-whole-codebase-v1.spec).
- Most v1 goals have been implemented: orchestrators (Unix tmux; Windows WT/PS/Git Bash), runner decomposition, utilities consolidation, notifications policy centralization, doctor/banner security parsing helper, color-aware logging helpers, and error-surface mappers.
- This v2 focuses on final consistency passes, hardening, small correctness items, security hygiene, test ergonomics, and documentation polish, while maintaining exact user-visible strings and behavior unless explicitly guarded by tests.

Guiding constraints
- Preserve all user-facing strings and outputs (golden tests) unless updated consistently across tests.
- Keep dependencies minimal; avoid heavy frameworks.
- Prefer low-risk, incremental changes; deliver in phases; keep CI green at every phase.

Comprehensive findings and improvement areas (v2)

1) Consistency: crate-qualified vs crate-relative references inside the library
- Files: several library modules refer to aifo_coder::… (crate name) instead of crate::… (preferred intra-crate).
- Issues:
  - Mixed usage across modules (e.g., src/fork/windows/helpers.rs tests refer to crate::ps_wait_process_cmd; orchestrators/windows_terminal.rs call aifo_coder::wt_build_new_tab_args).
  - While functionally OK (lib and bin targets share the crate), intra-lib code should consistently use crate:: to avoid surprise refactoring hazards if the crate name changes.
- Actions:
  - Replace aifo_coder::… with crate::… inside library modules (src/fork/*, src/toolchain/*, src/util/*, src/color.rs, etc.), keeping the binary (src/main.rs) free to reference aifo_coder::.
  - Guard strictly against any public API changes; refactor references only.

2) Error-surface uniformity: residual io::Error::other string constructions
- Files: scattered; most already wrapped via display_for_fork_error/ToolchainError but a few plain io::Error::new/io::Error::other remain in proxy and helpers.
- Issues:
  - Minor inconsistency with v1 goals to standardize error mapping and display strings.
- Actions:
  - Audit remaining io::Error::new/io::Error::other sites:
    - src/toolchain/proxy.rs (respond_* helpers return plain io::Result<()> ok; keep runtime errors local, but wrap top-level io::Error construction to go through display_for_toolchain_error where a ToolchainError semantics apply).
    - src/registry.rs uses io::new only indirectly; acceptable.
  - Where errors cross crate boundaries or reach user, ensure mapping via exit_code_for_* helpers; otherwise keep internal.

3) Logging wrappers adoption (color-aware) where message text is identical
- Files: src/fork/summary.rs, parts of src/commands/mod.rs, src/fork/cleanup.rs.
- Issues:
  - A few eprintln!/paint calls replicate patterns with ANSI codes that could be handled via log_* helpers, but only adopt where text remains exactly identical to preserve goldens.
- Actions:
  - Replace a limited subset of eprintln!(paint(..., "<fixed-ansi>", "<exact-string>")) with log_info_stderr/log_warn_stderr/log_error_stderr where the exact message is 1:1 identical (no wording changes).
  - Do not touch golden-sensitive multi-line guidance blocks.

4) Node toolchain default image mismatch (docs vs code)
- Files: src/toolchain/images.rs (DEFAULT_IMAGE_BY_KIND uses node:20-bookworm-slim), Dockerfile base images use node:22.
- Issues:
  - Inconsistency may confuse users; doctor/banner/reg docs discuss Node 22 in places.
- Actions:
  - Plan to bump default node image to node:22-bookworm-slim to align with Dockerfile base (document in CHANGES, update tests if any expect node:20).
- Risks:
  - Behavior change (different toolchain image). Mitigate by gating the change behind explicit “v2” plan; ensure tests not relying on exact string for node default.

5) Tests duplication: have_git/which/init_repo/urlencode_component helpers repeated
- Files: many tests/* (fork_* suites and accept_*), despite tests/support/mod.rs providing canonical helpers.
- Issues:
  - Significant duplication hampers maintainability and future changes.
- Actions:
  - Incrementally refactor tests to import tests/support helpers:
    - have_git()
    - which()
    - init_repo_with_default_user(dir)
  - For urlencoding in acceptance tests, prefer dev-dependency urlencoding crate (already present) or add a tests/support::urlencode() thin wrapper; keep outputs identical.
- Risks:
  - Platform-gated tests; ensure per-file minimal changes; keep “skipping:” messages verbatim.

6) Security hygiene: HTTP parsing and logging
- Files: src/toolchain/http.rs, src/toolchain/proxy.rs, src/bin/aifo-shim.rs.
- Issues:
  - Header parsing is tolerant; good. However, ensure no header values are re-emitted directly to output without normalization (current code does not echo client headers to logs; OK).
  - Chunked/trailer handling robust; small opportunity: tighten body cap constant (BODY_CAP) as a const fn param in tests, but not required.
- Actions:
  - Document caps and behavior; keep code unchanged functionally.
  - Consider minimal fuzz test for parse_form_urlencoded()/read_http_request (optional).

7) Unix TTY subprocess settings (stty) robustness in warn_input_unix
- Files: src/ui/warn.rs.
- Issues:
  - stty not always present; code is best-effort; restores sane defaults; acceptable.
- Actions:
  - Add a short module comment note on best-effort nature; no code change required.
  - Ensure any future errors do not bubble; current code ignores failures (OK).

8) Windows orchestrators: waitability semantics and reasons
- Files: src/fork/orchestrators/*.rs and selection in mod.rs.
- Issues:
  - Semantics are correct; small doc opportunity: document “supports_post_merge” contract in trait and implementors.
- Actions:
  - Add docstrings clarifying behavior; no behavior change.

9) Small dead_code allowances remain
- Files: src/fork/types.rs (allow(dead_code) on structs that are used), and other scattered #[allow(dead_code)] introduced during v1.
- Issues:
  - Now that code paths are in use, these allowances can be removed to tighten linting.
- Actions:
  - Remove unnecessary #[allow(dead_code)] in v2 after verifying no compile warnings.
  - Keep allowances where platform gates may hide usage (cfg(windows)/cfg(unix)).

10) Docker/AppArmor flows: minor doc polish and quiet variants
- Files: src/apparmor.rs, src/doctor.rs, src/banner.rs.
- Issues:
  - Profiles and logs are good; ensure quiet variants do not warn in non-interactive paths.
- Actions:
  - Minor documentation improvements; no functional changes.

11) Registry probe/cache: behavior is solid
- Files: src/registry.rs.
- Issues:
  - Mixed logs via eprintln! are acceptable; CHANGES documents source selection already.
- Actions:
  - No code changes required; optional: add hadolint/Makefile target in future for Dockerfile lint (defer).

12) Build system and Dockerfiles
- Files: Dockerfile, toolchains/rust/Dockerfile, toolchains/cpp/Dockerfile, Makefile.
- Issues:
  - Overall good; consider hadolint target; optional CA handling notes are present.
- Actions:
  - Optional future hardening; leave behavior as-is in v2.

13) Proxy timeouts and escalation constants
- Files: src/toolchain/proxy.rs and shim.
- Issues:
  - Values and environment hooks are reasonable; no user-visible changes proposed.
- Actions:
  - Document the envs centrally (existing docs are good); no code change.

14) Documentation coverage and contributor guidance
- Files: crate/module docs are strong; some modules can benefit from short clarifying headers (orchestrators trait, warn module caveats, error mapping guide).
- Actions:
  - Add short doc comments; ensure line length guidance respected; no behavior change.

Acceptance criteria (v2)
- All tests remain green; user-visible strings unchanged (unless noted and updated consistently).
- Intra-lib references use crate:: uniformly (no use of aifo_coder:: inside library modules).
- Residual io::Error::other/new sites creating user-displayed strings are wrapped or localized per v1 mapping.
- Optional Node image default aligned to Node 22 (if accepted).
- Duplicated test helpers largely refactored to tests/support use; acceptance suite messages unchanged.
- Added doc comments; removed stale #[allow(dead_code)] where safe.

Risks and mitigations
- Changing default Node image alters behavior.
  - Mitigation: gate under v2; announce in CHANGES; verify no tests assert the specific string for node default.
- crate:: refactor may miss a path.
  - Mitigation: compile-time failures will flag; refactor incrementally.
- Removing #[allow(dead_code)] may trigger warnings on non-target platforms.
  - Mitigation: keep allowances where cfg-gating hides usage.

Phase-optimized plan (v2)

Phase 1: Hygiene and consistency (low risk)
- Replace aifo_coder:: with crate:: inside library modules (no change in binary/main).
- Remove stale #[allow(dead_code)] where types/functions are in active use.
- Add small doc comments:
  - Orchestrator trait supports_post_merge semantics.
  - warn module stty caveat.
  - Error mapping README note at top of errors.rs.

Phase 2: Error-surface audit and tiny refactors
- Audit and wrap any remaining io::Error::other/new that propagate to user-visible boundaries through display_for_* helpers or keep locally scoped.
- Where possible, adopt log_* wrappers for identical, fixed text eprintln! lines (keep strings identical).

Phase 3: Tests consolidation (incremental)
- Migrate duplicated have_git/which/init_repo helpers to tests/support wherever feasible.
- For acceptance urlencoding helpers, switch to urlencoding crate or a small tests/support::urlencode() wrapper (keep outputs identical).
- Keep skip messages and outputs verbatim; run per-test-file conversion to minimize diffs.

Phase 4: Optional node image alignment (if approved)
- Bump DEFAULT_IMAGE_BY_KIND for node to "node:22-bookworm-slim".
- Update CHANGES.md; check doctexts and any tests referencing default strings; adjust accordingly.

Phase 5: Docs polish
- Add missing module headers and inline comments per items above.
- Consider a CONTRIBUTING snippet (future) with notes on tests/support and error/logging helpers.

Non-goals (v2)
- No orchestrator behavior changes; no CLI-breaking changes.
- No heavy dependency additions; no change to proxy protocol behavior or strings.

Appendix: Quick audit notes by file (selected)
- src/fork/orchestrators/mod.rs: selection logic is cross-platform and tested; add trait docs.
- src/fork/summary.rs: uses paint and println; acceptable; adopt log_* only where identical text prints exist and safe.
- src/toolchain/images.rs: consider node default bump.
- src/ui/warn.rs: robust; document stty best-effort.
- src/toolchain/proxy.rs: central dispatcher healthy; maintain existing strings; no behavior changes; comment on caps already present in http module.

Success metrics
- Cleaner intra-crate imports; simpler future refactors.
- Reduced duplication in tests; easier maintenance.
- Error/logging consistency fully aligned with v1 intent.
- Optional modernization of node default image to 22 to match container base.
