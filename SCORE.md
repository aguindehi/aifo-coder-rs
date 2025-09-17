# Source Code Scoring — 2025-09-18 00:45

Summary
- Implemented v5 phased toolchain shim plan: compiled Rust shim + shell wrappers, proxy with
  ExecId registry and /signal forwarding, disconnect termination semantics, host override shims,
  and launcher/plumbing to wire everything together. Native HTTP enabled; curl removed from slim images when KEEP_APT=0; retained where needed.

Grades
- Correctness: A-
  - Signal traps in Rust shim mirror POSIX shim (INT→TERM→KILL); parent-shell cleanup on Linux.
  - Proxy classifies endpoints, authenticates, streams with chunked prelude + trailers, and
    performs disconnect cleanup and optional max-runtime escalation.
  - Form parsing tolerant to CRLFCRLF/LFLF; allowlists enforced per toolchain kind.
- Robustness: A-
  - Best-effort retries on docker exec signals; defensive file I/O; unix socket transport gated.
  - Host override path supported read-only; wrappers avoid lingering shells.
- Performance: A-
  - Streaming (proto v2) with chunked transfer and minimal allocations; buffered v1 kept simple.
- Security: B+
  - Bearer validation strict; notifications endpoint whitelisted to 'say' only when configured.
  - Further hardening planned for native HTTP client (TLS/UDS validation and input caps).
- Maintainability: A
  - Clear modularization (auth, http, proxy, sidecar, routing, shim); logging consistent.
  - Tests cover key routines (exec wrapper args, URL/form parsing); more can be added.
- UX: A-
  - Unified verbose logs; clean prompt on disconnect via shim messaging and parent-shell handling.
- Test Coverage: B+
  - Unit tests present; recommend adding integration tests for proxy disconnect and signal paths.

Notable Strengths
- Feature parity between Rust and POSIX shims with consistent environment knobs and UX.
- Clean separation of responsibilities; good use of helper modules and re-exports.
- Backward-compatible defaults (exit semantics) with env toggles for legacy behavior.

Risks and Mitigations
- Curl retained in full images for tooling; removed from slim images when KEEP_APT=0.
- Parent-shell heuristics vary by distro: limited to Linux and guarded by env; proxy best-effort
  cleanup complements shim behavior.
- Docker CLI flakiness on signals: implemented brief retry; logs emphasized when verbose.

Recommendations (Next Steps)
1) Phase 4 acceptance tests:
   - Golden logs for native HTTP path (TCP/UDS); large-output and disconnect coverage.
   - Host override precedence and wrapper auto-exit behavior; signal escalation sequence.
2) Hardening and polish (v5.3):
   - Tighten input limits, error messages; broaden tests for tool availability routing.
   - Improve parent-shell cleanup fallback paths (non-/proc environments).
3) Documentation and release notes:
   - Describe verifying active shim and overriding with AIFO_SHIM_DIR; note curl removal from slim images.
   - Plan curl removal from full images after acceptance tests confirm no remaining dependencies.

Shall I proceed with these next steps?

# aifo-coder Source Code Scorecard

Date: 2025-09-07
Time: 12:00
Author: Amir Guindehi <amir.guindehi@mgb.ch>
Scope: Rust CLI launcher, Dockerfile multi-stage images (full and slim), toolchain sidecars (rust/node/python/c-cpp/go), embedded shim, host proxy (TCP + Linux unix socket), versioned toolchain specs and bootstrap, docs, tests, Makefile targets.

Overall grade: A (96/100)

Grade summary (category — grade [score/10]):
- Architecture & Design — A- [9]
- Rust Code Quality — A [10]
- Security Posture — A [9]
- Containerization & Dockerfile — A+ [10]
- Build & Release — A [9]
- Cross-Platform Support — A [10]
- Documentation — A [9]
- User Experience — A+ [10]
- Performance & Footprint — A- [9]
- Testing & CI — A- [9]

Executive summary
- aifo-coder remains a robust, secure, and developer-friendly tool. The main architectural opportunity is the size and cohesion of src/lib.rs and src/main.rs. A focused modularization will reduce coupling, improve testability, and make future features safer to land. A staged refactor plan is proposed below.

What changed since last score
- Standardized “To inspect and merge changes” guidance output and reduced repetition.
- Added a single preflight toolchain warning in fork orchestrator with opt-in abort, and suppressed duplicate pane warnings.
- Extracted CLI-only helpers (warnings, guidance, doctor) into separate modules in the binary for better readability.
- Maintained behavior and test coverage while improving UX consistency.

Key strengths
- Composable design across launcher, sidecars, proxy and shim with safe defaults and minimal global state.
- Excellent developer UX with clear diagnostics, previews, and color-aware warnings.
- Strong cross-platform orchestration (tmux on Unix; Windows Terminal/PowerShell/Git Bash on Windows).
- Careful security posture: no docker.sock, AppArmor on Linux, token-auth proxy, allowlists, uid:gid mapping.

Current gaps and risks
- Large monolithic files (src/lib.rs and src/main.rs) obscure boundaries and complicate changes.
- Fork and proxy logic would benefit from dedicated modules with explicit dependencies and test seams.
- Proxy operability (optional structured logs, rate limits) not yet present.

Detailed assessment

1) Architecture & Design — A- [9/10]
- Strengths: clear layering; well-factored helpers for docker command building, registry probing, and security checks.
- Opportunities: modularize by concern to reduce file size, improve clarity, and enable focused ownership.

2) Rust Code Quality — A [10/10]
- Idiomatic clap usage, error kinds, OnceCell caches, platform cfgs, and careful shell escaping.
- Tests cover tricky helpers (url/form decoding, CRLF header parsing, lock file candidates, tool routing).

3) Security Posture — A [9/10]
- Good defaults and isolation. Future: optional structured logs for proxy execs and concurrency limits.

4) Containerization & Dockerfile — A+ [10/10]
- Multi-stage builds, slim/full variants, embedded shim, named caches; images are consistent and efficient.

5) Build & Release — A [9/10]
- Deterministic docker previews, registry selection with provenance, SBOM target; CI can publish SBOMs by default.

6) Cross-Platform Support — A [10/10]
- Thoughtful support for macOS/Windows/Linux, including unix sockets on Linux and host-gateway mapping.

7) Documentation — A [9/10]
- Toolchains guide, Dockerfile targets, and Makefile utilities are well-documented; add more troubleshooting examples per OS.

8) User Experience — A+ [10/10]
- Color-aware warnings, dry-run previews, single orchestrator prompt, and standardized guidance blocks.

9) Performance & Footprint — A- [9/10]
- Named caches are effective; opportunities to defer sidecar startup or parallelize specific operations.

10) Testing & CI — A- [9/10]
- Strong unit tests; docker-gated smokes exist and can be elevated in CI.

Proposed refactoring plan (cohesive modularization)

Goals
- Decompose src/lib.rs and src/main.rs into cohesive modules by responsibility.
- Retain the current external API via crate-root re-exports to keep tests and main.rs stable.
- Improve readability, ownership, and compile times; enable focused testing per module.

Target module layout (library)

- src/color.rs
  - ColorMode, set_color_mode, color_enabled_stdout/stderr, paint.
- src/registry.rs
  - preferred_registry_prefix{_quiet}, preferred_registry_source, invalidate_registry_cache, disk cache helpers, test overrides.
- src/apparmor.rs
  - docker_supports_apparmor, desired_apparmor_profile{_quiet}, kernel detection/availability helpers.
- src/docker.rs
  - container_runtime_path, build_docker_cmd, env pass-through list, mount builders, preview formatter.
- src/util.rs
  - json_escape, shell_escape, shell_join, url_decode, find_crlfcrlf, find_header_end, strip_outer_quotes, shell_like_split_args.
- src/toolchain.rs
  - normalize_toolchain_kind, default_toolchain_image{_for_version}, toolchain_write_shims,
    toolchain_run, toolchain_start_session, toolchain_cleanup_session, toolchain_purge_caches,
    toolexec_start_proxy (TCP/unix), notifications parsing & execution policy (say-only).
- src/lock.rs
  - RepoLock, acquire_lock{,_at}, should_acquire_lock, candidate_lock_paths, normalized_repo_key_for_hash, hash_repo_key_hex.
- src/fork.rs
  - repo_root, fork_*: sanitize_base_label, base_info, create_snapshot, clone_and_checkout_panes,
    repo_uses_lfs_quick, fork_list/clean/autoclean/stale_notice, fork_merge_branches{,_by_session}.
  - Windows-only helpers re-exported for tests: fork_ps_inner_string, fork_bash_inner_string,
    wt_orient_for_layout, wt_build_new_tab_args, wt_build_split_args, ps_wait_process_cmd.

Crate root (lib.rs)
- mod {color,registry,apparmor,docker,util,toolchain,lock,fork};
- pub use {color::*, registry::*, apparmor::*, docker::*, util::*, toolchain::*, lock::*, fork::*};

Binary-only modules (src/)
- src/warnings.rs
  - warn_if_tmp_workspace, maybe_warn_missing_toolchain_agent, maybe_warn_missing_toolchain_for_fork.
- src/guidance.rs
  - print_inspect_merge_guidance (standardized output for “To inspect and merge changes”).
- src/doctor.rs
  - run_doctor (invokes re-exported library APIs, prints diagnostics).

Design considerations
- Library modules remain free of CLI-only prints except via helpers returning strings where practical; the binary orchestrates user I/O.
- The docker module depends on apparmor/color/util; others avoid cyclic deps by clear boundaries.

Phased execution plan

Phase 1 — Binary-only extraction (low risk)
- Move from main.rs to:
  - warnings.rs: warn_if_tmp_workspace, maybe_warn_missing_toolchain_*.
  - guidance.rs: print_inspect_merge_guidance.
  - doctor.rs: run_doctor.
- Update main.rs to mod warnings/guidance/doctor and use imports.
- Run cargo test.

Phase 2 — Core helpers (moderate)
- Extract color.rs, util.rs, apparmor.rs, registry.rs.
- Replace in-file implementations in lib.rs with module imports; add pub use re-exports.
- Run cargo test; verify registry and apparmor tests.

Phase 3 — Docker and lock layers (moderate)
- Extract docker.rs and lock.rs and re-export.
- Ensure build_docker_cmd compiles with re-exports; verify docker preview tests.

Phase 4 — Toolchains and proxy (higher impact)
- Extract toolchain.rs (sidecars, proxy, notifications).
- Re-export all public functions; ensure toolchain and proxy tests (including unix sockets) pass.

Phase 5 — Fork lifecycle (higher impact)
- Extract fork.rs (snapshot/clone/merge/clean/list/autoclean & Windows helpers).
- Re-export public functions; validate all fork_* tests (including JSON formatting, colorized paths, stale notices).

Phase 6 — Polish and linting (low risk)
- Remove dead imports and legacy code; add module docs (//!).
- Enable clippy lints (e.g., cargo clippy -- -D warnings) and fix issues.
- Consider splitting very long functions like fork_merge_branches into smaller helpers.

Acceptance criteria
- All unit tests compile and pass across platforms; docker-gated tests pass when enabled.
- Public API at crate root unchanged (re-exports); no regressions in CLI behavior or exit codes.
- main.rs slimmed to subcommand dispatch, with warnings/guidance/doctor delegated.

Risk mitigation
- Keep module boundaries narrow and add pub use to maintain outward API shape.
- Land phases in separate PRs to keep diffs reviewable; run test matrix on Linux/macOS/Windows where possible.

Post-refactor enhancements (optional)
- Add structured logs for proxy executions (tool/kind/exit/duration) behind a feature flag or env.
- Concurrency limiter per sidecar/tool kind for safety under load.
- E2E tests inside agent containers to validate shim→proxy→sidecar pipeline.
- Documentation: quickstart for toolchains, fork troubleshooting per OS, unix-socket permissions.

Summary
- This plan minimizes risk while yielding tangible maintainability gains. It preserves behavior, protects the public API via re-exports, and leverages the existing test suite to validate each step. Completing the phases will make aifo-coder simpler to extend and safer to evolve.
