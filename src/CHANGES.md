2025-09-18 02:55 User <user@example.com>

Shim v5.4: disconnect UX — start proxy signals during shim wait

- Rust shim now drops the /exec stream immediately on disconnect to trigger the proxy's INT→TERM→KILL sequence and logs during the shim's verbose wait, preventing prompt overwrite.
- Kept Linux parent-shell termination; exit semantics unchanged (defaults preserved).
- Acceptance tests: accept_disconnect and Phase 4 suite passing locally.

2025-09-18 02:40 User <user@example.com>

QA: test suite green (231 passed, 30 skipped)

- Verified all tests are green with latest changes; 30 skipped due to docker/UDS gating.

2025-09-18 02:35 User <user@example.com>

QA: Phase 4 acceptance tests passed

- Ran make test-accept-phase4: native HTTP TCP passed; UDS skipped on non-Linux;
  wrapper auto-exit check passed/skipped conditionally based on local image.
- v5 implementation complete across Phases 1–4; release notes and verification
  guide added.

2025-09-18 02:15 User <user@example.com>

QA: test suite green (231 passed, 26 skipped)

- Verified all tests are green after acceptance tests/doc additions.

2025-09-18 02:00 User <user@example.com>

Shim v5.4: Phase 4 acceptance tests, golden docs, and curl retention

- Added Phase 4 acceptance tests (ignored by default): TCP/UDS native HTTP and wrapper check.
- Introduced docs/RELEASE_NOTES_v5.md and docs/VERIFY_v5.md with quick verification checklist.
- Adjusted curl policy: retain curl in full agent images; slim images still drop curl when KEEP_APT=0.

2025-09-18 01:30 User <user@example.com>

QA: test suite green (231 passed, 24 skipped)

- Verified all tests are green after TTY default change and native HTTP refinements.

2025-09-18 01:15 User <user@example.com>

Shim v5.2: proxy TTY default on (AIFO_TOOLEEXEC_TTY=0 disables)

- In streaming proto (v2), allocate a TTY by default for better interactive flushing.
- Set AIFO_TOOLEEXEC_TTY=0 to disable TTY allocation; v1 buffered path unchanged.

2025-09-18 01:00 User <user@example.com>

Shim v5.2: refine native HTTP urlencoding and finalize Phase 3

- Native HTTP client: encode '*' in application/x-www-form-urlencoded components (safer RFC compliance).
- Verified case-insensitive X-Exit-Code handling in headers/trailers; unified disconnect UX retained.
- Phase 3 complete; proceed to Phase 4 acceptance tests (TCP/UDS, large output, disconnect).

2025-09-18 00:45 User <user@example.com>

Shim v5.2: polish native HTTP and proxy docs

- Native HTTP client: tolerant X-Exit-Code parsing (case-insensitive headers/trailers).
- Proxy docs clarified disconnect sequence (INT → TERM → KILL) to match implementation.
- Next: add acceptance tests (TCP/UDS, large output, disconnect) before removing curl from full images.

2025-09-18 00:30 User <user@example.com>

Shim v5.2: drop curl from slim runtime images (KEEP_APT=0)

- Removed curl from codex-slim, crush-slim, and aider-slim runtime images when KEEP_APT=0.
- Kept curl in builder stages and full images as needed for tooling (e.g., uv install).
- Native HTTP client remains default; set AIFO_SHIM_NATIVE_HTTP=0 to force curl fallback.

2025-09-18 00:00 User <user@example.com>

Shim v5.2: enable native HTTP by default; curl fallback opt-out

- Native HTTP client (TCP/UDS, chunked + trailers) enabled by default in Rust shim.
- Set AIFO_SHIM_NATIVE_HTTP=0 to force curl-based path (temporary safety valve).
- Behavior and exits preserved; curl removal will follow after acceptance tests.

2025-09-17 12:30 User <user@example.com>

Shim v5: finalize parity and logging; minor polish

- Unified proxy verbose logs to include exec_id in parsed-request line.
- Ensured Linux-only signal hooks for Rust shim; preserved default 0 exit on traps/disconnect.
- Kept curl-based client for v5.0–v5.1; wrappers auto-exit and respect no_shell_on_tty markers.

2025-09-17 12:00 User <user@example.com>

QA: test suite green (231 passed, 24 skipped)

- Verified all tests are green with latest shim/proxy changes.

2025-09-17 12:00 User <user@example.com>

Shim v5: compiled Rust shim, wrappers, proxy signals

- Baked compiled Rust aifo-shim into images; added sh/bash/dash auto-exit wrappers.
- Implemented ExecId registry and /signal in proxy; disconnect termination UX and escalation.
- Added host-generated POSIX shims (override) with traps and markers; curl remains (v5.0–v5.1).
- Launcher integrates SHELL=/opt/aifo/bin/sh and PATH symlinks; optional unix socket on Linux.

2025-09-15 00:00 User <user@example.com>

QA: test suite green (230 passed, 24 skipped)

- Verified all tests are green with latest changes.

2025-09-15 00:00 User <user@example.com>

Tests: add non-color plain goldens for single-repo and workspace

- Added tests/fork_list_plain_nocolor.rs to lock non-color plain output for single-repo and workspace modes under non-TTY capture.
- Updated SCORE.md test counts and notes; kept behavior identical.

2025-09-15 00:00 User <user@example.com>

Tests: add workspace plain-color golden; refine SCORE

- Added tests/fork_list_workspace_plain_color.rs to lock colored plain output for --all-repos when AIFO_CODER_COLOR=always.
- Updated SCORE.md test counts and refined recommendations for further golden scenarios (TTY vs non-TTY).
- No functional changes; behavior remains identical.

2025-09-15 00:00 User <user@example.com>

QA: test suite green (227 passed, 24 skipped)

- Verified all tests are green with latest changes.

2025-09-15 00:00 User <user@example.com>

Docs: add runner module docs; refine SCORE next steps

- Added module-level docs to src/fork/runner.rs (binary-side orchestrator).
- Updated src/SCORE.md recommendations to the next iteration (monitor CI, more goldens).

2025-09-15 00:00 User <user@example.com>

QA: test suite green (225 passed, 24 skipped)

- Verified CI and tests across modules; all tests are now green.
- No functional changes; documentation and minor cleanup only.

2025-09-15 00:00 User <user@example.com>

QA: test suite green (223 passed, 24 skipped)

- Verified recommendations implemented; entire test suite green.
- No code changes in this entry; documentation-only update.

2025-09-15 00:00 User <user@example.com>

Tests: add golden JSON and git helper tests; docs and git_cmd helper

- Added golden-style JSON row formatter test for fork_list and a targeted test suite for git helpers.
- Introduced git_cmd builder in fork_impl/git.rs and adopted it in pane checks; added module docs across fork_impl/*.
- Refactored list JSON building to use a shared formatter to keep byte-for-byte stability.
- Kept public behavior unchanged; all tests remain green.

2025-09-14 00:00 User <user@example.com>

Tests: add missing Phase-0 coverage; centralize git file:// allow helper

- Added unit tests for fork list ordering and stale-flag computation (module tests in fork_impl/list.rs).
- Added unit tests for fork clean plan classification (dirty, base-unknown, ahead) in fork_impl/clean/plan.rs.
- Added unit tests for compose_merge_message prefix/truncation (module tests via fork_impl/merge_tests.rs).
- Introduced helper in fork_impl/git.rs to centralize -c protocol.file.allow=always:
  push_file_allow_args (for previews) and set_file_allow (for Command).
- Updated merge/clone internal call sites to use the helper while preserving behavior and logs.

2025-09-14 00:00 User <user@example.com>

Refactor: extract list/clone/snapshot helpers into fork_impl and delegate

- Moved fork_list into src/fork_impl/list.rs; fork.rs now delegates, preserving output and ordering.
- Moved fork_clone_and_checkout_panes into src/fork_impl/clone.rs; behavior identical.
- Moved fork_create_snapshot into src/fork_impl/snapshot.rs; behavior identical.
- Left fork_clean decomposition (plan/prompt/exec) for the next step to complete Phase 2.
- All public APIs and strings preserved; existing tests continue to pass.

2025-09-14 00:00 User <user@example.com>

Refactor: extract merge helpers into fork_impl; delegate from fork.rs

- Extracted merge-related helpers and flows into src/fork_impl/merge.rs:
  collect_pane_branches, preflight_clean_working_tree, compose_merge_message, and
  public wrappers fork_merge_branches/fork_merge_branches_by_session now delegate.
- Kept behavior, outputs, and exit codes identical; internal only.
- Added module-level docs to clarify responsibilities of the new helper module.
- Left clone/snapshot/list extraction as next steps to keep this change focused.

2025-09-14 00:00 User <user@example.com>

Refactor fork.rs; add helpers; fix lock CWD race; update spec

- Decomposed fork logic into private modules under src/fork_impl (git, panecheck, notice)
  and delegated from src/fork.rs without changing public APIs or outputs.
- Centralized Git calls (git_stdout_str, git_status_porcelain, git_supports_lfs) and
  replaced ad-hoc invocations where appropriate.
- Implemented pane cleanliness checks in fork_impl/panecheck.rs to unify logic.
- Moved stale notice and auto-clean logic to fork_impl/notice.rs; fork.rs now delegates.
- Fixed candidate_lock_paths to capture the initial CWD to avoid races in tests.
- Updated specification spec/aifo-coder-refactor-fork-rs-v3.spec to v3-implemented.
- Ensured manual JSON writers in src/fork/meta.rs are reused and exported via lib.
- All tests pass: 207 passed, 24 skipped.
