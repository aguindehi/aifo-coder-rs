Spec: Consolidate all cargo nextest calls in Makefile to use identical args via ARGS_NEXTEST
Version: v1
Date: 2025-11-16

Goal
- Ensure every cargo nextest run invocation in Makefile uses ARGS_NEXTEST for global/consistent settings.
- Preserve test selection, concurrency, and run-ignored behaviors exactly as before.
- Standardize quoting so ARGS_NEXTEST expands correctly in docker/macOS-cross sh -lc commands.

Definitions
- ARGS_NEXTEST (global): --profile ci --no-fail-fast --status-level=fail --hide-progress-bar --cargo-quiet
- User-supplied extra args: $(ARGS) must remain last to allow pass-through overrides.
- Suite-specific flags (must remain where present):
  - Filters (-E 'test(/^…/)'), ignoring policy (--run-ignored ...), concurrency (-j 1), and any suite-specific switches.
- Environments and variables (must remain exactly as before):
  - GIT_CONFIG_NOSYSTEM, GIT_CONFIG_GLOBAL, GIT_TERMINAL_PROMPT, CARGO_TARGET_DIR, NICENESS_CARGO_NEXTEST, platform args.

Consistency rules
1) Always invoke cargo nextest run with ARGS_NEXTEST immediately after run:
   cargo nextest run $(ARGS_NEXTEST) [suite-specific flags] $(ARGS)
2) Do not repeat flags already provided by ARGS_NEXTEST (e.g., --profile ci, --no-fail-fast).
3) Preserve suite-specific flags in the same places (order can be: ARGS_NEXTEST, then suite flags, then $(ARGS)).
4) In docker/macOS-cross sh -lc segments, use double quotes for the full command string so Make expands $(ARGS_NEXTEST).
   Keep regex filters wrapped in single quotes inside the double-quoted string: -E 'test(/^.../)'
5) Keep -j 1 where the existing target uses it (acceptance/integration suites and coverage macro).
6) Do not alter version checks/installs (cargo nextest -V, cargo install cargo-nextest).

Inventory of changes (before → after)

A) test-acceptance-suite
- Before:
  cargo nextest run -j 1 --run-ignored ignored-only --no-fail-fast -E "$$EXPR" $(ARGS)
- After:
  cargo nextest run $(ARGS_NEXTEST) -j 1 --run-ignored ignored-only -E "$$EXPR" $(ARGS)

B) test-integration-suite
- Before:
  cargo nextest run -j 1 --no-fail-fast -E "$$EXPR" $(ARGS)
- After:
  cargo nextest run $(ARGS_NEXTEST) -j 1 -E "$$EXPR" $(ARGS)

C) test-macos-cross-image (inside container command)
- Before:
  sh -lc '/usr/local/cargo/bin/cargo nextest -V >/dev/null 2>&1 || /usr/local/cargo/bin/cargo install cargo-nextest --locked; /usr/local/cargo/bin/cargo nextest run --run-ignored ignored-only --profile ci --no-fail-fast -E "test(/^e2e_macos_cross_/)"'
- After:
  sh -lc "/usr/local/cargo/bin/cargo nextest -V >/dev/null 2>&1 || /usr/local/cargo/bin/cargo install cargo-nextest --locked; /usr/local/cargo/bin/cargo nextest run $(ARGS_NEXTEST) --run-ignored ignored-only -E 'test(/^e2e_macos_cross_/)'"

D) test-all-junit
Linux branch:
- No docker path (filtered):
  - Before: cargo nextest run --run-ignored all --profile ci --no-fail-fast -E "$$FEX" $(ARGS)
  - After:  cargo nextest run $(ARGS_NEXTEST) --run-ignored all -E "$$FEX" $(ARGS)
- Docker path (full suite):
  - Before: cargo nextest run --run-ignored all --profile ci --no-fail-fast $(ARGS)
  - After:  cargo nextest run $(ARGS_NEXTEST) --run-ignored all $(ARGS)

Non-Linux branch:
- No docker path (filtered):
  - Before: cargo nextest run --run-ignored all --profile ci --no-fail-fast -E "$$FEX" $(ARGS)
  - After:  cargo nextest run $(ARGS_NEXTEST) --run-ignored all -E "$$FEX" $(ARGS)
- Docker path (skip UDS tests):
  - Before: cargo nextest run --run-ignored all --profile ci --no-fail-fast -E '!test(/_uds/)' $(ARGS)
  - After:  cargo nextest run $(ARGS_NEXTEST) --run-ignored all -E '!test(/_uds/)' $(ARGS)

E) test-toolchain-rust
- rustup path:
  - Before: rustup run stable cargo nextest run -E 'test(/^int_toolchain_rust_/)' $(ARGS)
  - After:  rustup run stable cargo nextest run $(ARGS_NEXTEST) -E 'test(/^int_toolchain_rust_/)' $(ARGS)
- local cargo path:
  - Before: cargo nextest run -E 'test(/^int_toolchain_rust_/)' $(ARGS)
  - After:  cargo nextest run $(ARGS_NEXTEST) -E 'test(/^int_toolchain_rust_/)' $(ARGS)
- docker path:
  - Before: sh -lc "cargo nextest -V >/dev/null 2>&1 || cargo install cargo-nextest --locked; ... cargo nextest run -E 'test(/^int_toolchain_rust_/)' $(ARGS)"
  - After:  sh -lc "cargo nextest -V >/dev/null 2>&1 || cargo install cargo-nextest --locked; ... cargo nextest run $(ARGS_NEXTEST) -E 'test(/^int_toolchain_rust_/)' $(ARGS)"

F) test-toolchain-rust-e2e
- rustup path:
  - Before: rustup run stable cargo nextest run --run-ignored ignored-only -E 'test(/^e2e_toolchain_rust_/)' $(ARGS)
  - After:  rustup run stable cargo nextest run $(ARGS_NEXTEST) --run-ignored ignored-only -E 'test(/^e2e_toolchain_rust_/)' $(ARGS)
- local cargo path:
  - Before: cargo nextest run --run-ignored ignored-only -E 'test(/^e2e_toolchain_rust_/)' $(ARGS)
  - After:  cargo nextest run $(ARGS_NEXTEST) --run-ignored ignored-only -E 'test(/^e2e_toolchain_rust_/)' $(ARGS)
- docker path:
  - Before: sh -lc "cargo nextest -V >/dev/null 2>&1 || cargo install cargo-nextest --locked; ... cargo nextest run --run-ignored ignored-only -E 'test(/^e2e_toolchain_rust_/)' $(ARGS)"
  - After:  sh -lc "cargo nextest -V >/dev/null 2>&1 || cargo install cargo-nextest --locked; ... cargo nextest run $(ARGS_NEXTEST) --run-ignored ignored-only -E 'test(/^e2e_toolchain_rust_/)' $(ARGS)"

What stays unchanged
- test target already uses ARGS_NEXTEST consistently; no changes needed.
- Coverage pipeline macros (RUN_NEXTEST_WITH_COVERAGE) already include ARGS_NEXTEST; no changes needed.
- All cargo test fallbacks and version/installation checks remain intact.

Corrections and small cleanups identified during validation
- Fix typos in test-all-junit: redirect paths should be /dev/null, not /divert/null.
- Ensure sh -lc quoting in docker/macOS-cross segments uses double quotes at the outer level to expand $(ARGS_NEXTEST) on host,
  while keeping regex filters wrapped in single quotes inside.

Risks and mitigations
- Quoting changes: Use double quotes around the full sh -lc string and single quotes around filters to avoid shell/regex escape issues.
- Duplicate flags: Removing explicit --profile ci and --no-fail-fast in places where ARGS_NEXTEST is added prevents accidental overrides.

Acceptance criteria
- All cargo nextest run invocations include $(ARGS_NEXTEST).
- Suite-specific behaviors (filters, run-ignored policies, -j 1) remain identical.
- make check, make test-acceptance-suite, make test-integration-suite, make test-all-junit,
  make test-toolchain-rust, make test-toolchain-rust-e2e all execute the same set of tests as before.

Phased implementation plan

Phase 0: Land this spec (v1)
- Record date and version.
- Share motivation, inventory, and exact before→after changes.

Phase 1: Consolidation edits
- Modify the following sections in Makefile to inject $(ARGS_NEXTEST) and remove duplicated flags:
  - test-acceptance-suite
  - test-integration-suite
  - test-macos-cross-image (quoting fix + ARGS_NEXTEST)
  - test-all-junit (all branches)
  - test-toolchain-rust (all branches)
  - test-toolchain-rust-e2e (all branches)
- Apply typo fix in test-all-junit: replace /divert/null with /dev/null.

Phase 2: Lint and quick build
- Run make lint and fix any format/clippy issues if they arise due to line wrapping or quoting.

Phase 3: Verification
- Run:
  - make check
  - make test-acceptance-suite
  - make test-integration-suite
  - make test-all-junit
  - make test-toolchain-rust
  - make test-toolchain-rust-e2e
- Confirm output shows ARGS_NEXTEST flags present and that filters/run-ignored/concurrency are unchanged.

Phase 4: Documentation
- Note consolidation in CHANGELOG or commit message.
- Reference this spec and tie it to the Makefile changes.

Appendix: Examples of standardized invocations

- Acceptance:
  cargo nextest run $(ARGS_NEXTEST) -j 1 --run-ignored ignored-only -E "$$EXPR" $(ARGS)

- Integration:
  cargo nextest run $(ARGS_NEXTEST) -j 1 -E "$$EXPR" $(ARGS)

- All JUnit (Linux, no docker):
  cargo nextest run $(ARGS_NEXTEST) --run-ignored all -E "$$FEX" $(ARGS)

- All JUnit (Linux, docker path, full suite):
  cargo nextest run $(ARGS_NEXTEST) --run-ignored all $(ARGS)

- All JUnit (non-Linux, docker path, skip UDS):
  cargo nextest run $(ARGS_NEXTEST) --run-ignored all -E '!test(/_uds/)' $(ARGS)

- macOS cross (inside container):
  /usr/local/cargo/bin/cargo nextest run $(ARGS_NEXTEST) --run-ignored ignored-only -E 'test(/^e2e_macos_cross_/)'


End of spec v1
