Spec: Consolidate all cargo nextest calls in Makefile to use identical args via ARGS_NEXTEST
Version: v2
Date: 2025-11-16

Motivation
- Consistency: ensure every cargo nextest run uses the same global settings for predictable CI behavior.
- Reliability: avoid divergence of flags across suites when adding or maintaining tests.
- Performance and environment parity: preserve and continue to use existing secret and cache mounts (e.g., corporate CA injection on the host and within containers, cargo/git caches) wherever they currently exist so we do not regress connectivity or speed.
- Safety: keep suite semantics identical (filters, concurrency, run-ignored), and do not break existing test structures.

Goal
- Ensure every cargo nextest run invocation in Makefile uses ARGS_NEXTEST for global/consistent settings.
- Preserve test selection, concurrency, and run-ignored behaviors exactly as before.
- Standardize quoting so ARGS_NEXTEST expands correctly in docker/macOS-cross sh -lc commands.
- Do not eliminate secret and cache mounting; retain all existing mounts used for CA injection and caches to preserve performance and network trust on host and in containers.

Definitions
- ARGS_NEXTEST (global): --profile ci --no-fail-fast --status-level=fail --hide-progress-bar --cargo-quiet
- User-supplied extra args: $(ARGS) must remain last to allow pass-through overrides.
- Suite-specific flags (must remain where present):
  - Filters (-E 'test(/^…/)'), ignoring policy (--run-ignored ...), concurrency (-j 1), and any suite-specific switches.
- Environments and variables (must remain exactly as before):
  - GIT_CONFIG_NOSYSTEM, GIT_CONFIG_GLOBAL, GIT_TERMINAL_PROMPT, CARGO_TARGET_DIR, NICENESS_CARGO_NEXTEST, platform args, and any SSL-related variables.
- Secret and cache mounts: all existing docker run flags, bind mounts and cache mounts used to inject corporate CA or enable cargo/git caches must remain unchanged and in-place.
  - Examples:
    - Bind/secret mounts for CA (e.g., corporate PEMs) to ensure outbound TLS trust (host and containers).
    - Cache mounts/binds for cargo/target and cargo registry/git to speed nextest.
    - Any environment variables tied to CA usage (e.g., SSL_CERT_FILE/SSL_CERT_DIR, REQUESTS_CA_BUNDLE, NODE_EXTRA_CA_CERTS) must remain.

Consistency rules
1) Always invoke cargo nextest run with ARGS_NEXTEST immediately after run:
   cargo nextest run $(ARGS_NEXTEST) [suite-specific flags] $(ARGS)
2) Do not repeat flags already provided by ARGS_NEXTEST (e.g., --profile ci, --no-fail-fast).
3) Preserve suite-specific flags in the same places (order can be: ARGS_NEXTEST, then suite flags, then $(ARGS)).
4) In docker/macOS-cross sh -lc segments, use double quotes for the full command string so Make expands $(ARGS_NEXTEST) on host, while keeping regex filters wrapped in single quotes inside the double-quoted string: -E 'test(/^.../)'.
5) Keep -j 1 where the existing target uses it (acceptance/integration suites and coverage macro).
6) Do not alter version checks/installs (cargo nextest -V, cargo install cargo-nextest).
7) Retain all existing secret and cache mounts (and related env exports) as-is; do not remove or refactor them during consolidation. Where the nextest invocation is moved inside quotes, keep mounts on the docker run line unchanged.
8) Do not break existing test structures: targets, dependencies, needs, artifacts, and coverage macro behavior must remain exactly as before.

Verification against v1
- The v1 inventory and intended edits are correct in scope and preserve suite semantics.
- Gaps identified and addressed in v2:
  - Explicitly require retention of secret and cache mounts and CA-related env vars across all docker invocations.
  - Standardize quoting for macOS-cross and builder-container sh -lc segments to expand $(ARGS_NEXTEST) while preserving regex filters.
  - Fix typos in test-all-junit redirections (/divert/null → /dev/null).
  - Clarify that coverage macros already use $(ARGS_NEXTEST) and must remain unchanged.

Inventory of changes (before → after)
A) test-acceptance-suite
- Before:
  cargo nextest run -j 1 --run-ignored ignored-only --no-fail-fast -E "$$EXPR" $(ARGS)
- After:
  cargo nextest run $(ARGS_NEXTEST) -j 1 --run-ignored ignored-only -E "$$EXPR" $(ARGS)
- Notes: Secret/cache mounts and CA env remain unchanged (none added/removed here).

B) test-integration-suite
- Before:
  cargo nextest run -j 1 --no-fail-fast -E "$$EXPR" $(ARGS)
- After:
  cargo nextest run $(ARGS_NEXTEST) -j 1 -E "$$EXPR" $(ARGS)

C) test-macos-cross-image (inside container command)
- Before:
  sh -lc '/usr/local/cargo/bin/cargo nextest -V >/dev/null 2>&1 || /usr/local/cargo/bin/cargo install cargo-nextest --locked; /usr/local/cargo/bin/cargo nextest run --run-ignored ignored-only --profile ci --no-fail-fast -E "test(/^e2e_macos_cross_/)"'
- After:
  sh -lc "/usr/local/cargo/bin/cargo nextest -V >/dev/null 2>&1 || /usr/local/cargo/bin/cargo install cargo-nextest --locked; /usr/local/cargo/bin/cargo nextest run $(ARGS_NEXTEST) --run-ignored ignored-only -E 'test(/^e2e_macos_cross_/)' $(ARGS)"
- Notes: Keep any docker run -v/--mount lines for CA and caches intact (host→container injection). Preserve CA env (e.g., SSL_CERT_FILE/SSL_CERT_DIR) if exported.

D) test-all-junit
Linux branch:
- No docker path (filtered):
  - Before: cargo nextest run --run-ignored all --profile ci --no-fail-fast -E "$$FEX" $(ARGS)
  - After:  cargo nextest run $(ARGS_NEXTEST) --run-ignored all -E "$$FEX" $(ARGS)
- Docker path (full suite):
  - Before: cargo nextest run --run-ignored all --profile ci --no-fail-fast $(ARGS)
  - After:  cargo nextest run $(ARGS_NEXTEST) --run-ignored all $(ARGS)
- Redirect fixes:
  - Before: >/divert/null
  - After:  >/dev/null
Non-Linux branch:
- No docker path (filtered):
  - Before: cargo nextest run --run-ignored all --profile ci --no-fail-fast -E "$$FEX" $(ARGS)
  - After:  cargo nextest run $(ARGS_NEXTEST) --run-ignored all -E "$$FEX" $(ARGS)
- Docker path (skip UDS):
  - Before: cargo nextest run --run-ignored all --profile ci --no-fail-fast -E '!test(/_uds/)' $(ARGS)
  - After:  cargo nextest run $(ARGS_NEXTEST) --run-ignored all -E '!test(/_uds/)' $(ARGS)
- Notes: Retain any mounts/secret handling on docker run lines.

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
- Notes: Keep CA/caches mounts and env exports on surrounding docker run lines unchanged.

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
- Notes: Retain secret and cache mounts; do not remove or relocate them.

What stays unchanged
- The test target already uses ARGS_NEXTEST consistently; no changes needed.
- Coverage pipeline macros (RUN_NEXTEST_WITH_COVERAGE) already include ARGS_NEXTEST; no changes needed.
- All cargo test fallbacks and version/installation checks remain intact.
- All secret and cache mounts (for corporate CA injection and caches) remain exactly as-is. This includes docker run bind mounts and cache mounts, and any related environment variable exports.

Corrections and small cleanups identified during validation
- Fix typos in test-all-junit: redirect paths should be /dev/null, not /divert/null.
- Ensure sh -lc quoting in docker/macOS-cross segments uses double quotes at the outer level to expand $(ARGS_NEXTEST) on host, while keeping regex filters wrapped in single quotes inside.
- Do not alter or remove any secret or cache mount lines; keep their relative order and grouping.
- Ensure CA-related env variables (e.g., SSL_CERT_FILE, SSL_CERT_DIR) remain exported wherever they are used today to prevent TLS failures in CI or local runs.

Risks and mitigations
- Quoting changes: Use double quotes around the full sh -lc string and single quotes around filters to avoid shell/regex escape issues.
- Duplicate flags: Removing explicit --profile ci and --no-fail-fast in places where ARGS_NEXTEST is added prevents accidental overrides.
- Environment trust: Retaining mounts and env exports ensures the corporate CA remains active and prevents registry/git TLS issues.
- Performance: Preserving cache mounts avoids slowing down nextest due to re-fetching crates and re-building test artifacts.

Acceptance criteria
- All cargo nextest run invocations include $(ARGS_NEXTEST).
- Suite-specific behaviors (filters, run-ignored policies, -j 1) remain identical.
- Secret and cache mounts, and related env exports, are unchanged and still effective.
- make check, make test-acceptance-suite, make test-integration-suite, make test-all-junit, make test-toolchain-rust, make test-toolchain-rust-e2e all execute the same set of tests as before.

Phased implementation plan

Phase 0: Land this spec (v2)
- Record date and version.
- Share motivation and exact before→after changes.
- Reiterate preservation of secret and cache mounts for CA and performance.

Phase 1: Consolidation edits
- Modify the following sections in Makefile to inject $(ARGS_NEXTEST) and remove duplicated flags:
  - test-acceptance-suite
  - test-integration-suite
  - test-macos-cross-image (quoting fix + ARGS_NEXTEST)
  - test-all-junit (all branches; fix /divert/null → /dev/null)
  - test-toolchain-rust (all branches)
  - test-toolchain-rust-e2e (all branches)
- Ensure surrounding docker run lines (if any) retain all existing secret and cache mounts, and env exports used for CA trust.
- Do not change targets that already use $(ARGS_NEXTEST) consistently.

Phase 2: Lint and quick build
- Run make lint and fix any format/clippy issues if they arise due to line wrapping or quoting.
- Confirm secret and cache mounts remain referenced in docker targets and that env exports are intact.

Phase 3: Verification
- Run:
  - make check
  - make test-acceptance-suite
  - make test-integration-suite
  - make test-all-junit
  - make test-toolchain-rust
  - make test-toolchain-rust-e2e
- Confirm output shows $(ARGS_NEXTEST) flags present and that filters/run-ignored/concurrency are unchanged.
- Validate that corporate CA injection is active (e.g., registry/git commands succeed) and caches operate as expected (no unexpected re-fetch/rebuild).

Phase 4: Documentation
- Note consolidation in CHANGELOG or commit message.
- Reference this spec and tie it to the Makefile changes, explicitly mentioning preserved secret and cache mounts for CA injection and performance.

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
  /usr/local/cargo/bin/cargo nextest run $(ARGS_NEXTEST) --run-ignored ignored-only -E 'test(/^e2e_macos_cross_/)' $(ARGS)

End of spec v2
