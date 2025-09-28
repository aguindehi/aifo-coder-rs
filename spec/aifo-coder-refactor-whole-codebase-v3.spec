Focused refactor and hardening plan (updated 2025-09-28)

Summary

 • Strong architecture and test coverage; careful preservation of user-facing strings.
 • Several prior quick wins are already implemented in code:
    – Proxy v1 maps timeouts to 504 (src/toolchain/proxy.rs).
    – HTTP parser guards: header count limit, CL mismatch, TE:chunked precedence
      (src/toolchain/http.rs).
 • Remaining opportunities focus on runtime harmonization, notifications hardening,
   proxy streaming backpressure, and minor portability.

Current strengths

 • Defensive surfaces: tool allowlists, absolute-path notifications, timeout escalation,
   fork locks, registry probing with overrides, sidecar caches/ownership init.
 • Clean run/exec previews (agent + toolchains), consistent shell escaping helpers.
 • Fork flows: base detection, snapshot commit, cloning best-effort with LFS/submodules,
   merge strategies, summaries, and metadata writers.
 • Broad tests on parsing, signaling, chunked trailers, editor presence, named volumes,
   network flags, unix socket transport, and preview formatting.

New insights and gaps

 1 Runtime harmonization in doctor and AppArmor

 • Observed: doctor.rs uses Command::new("docker") for several run/info checks; some
   calls already use container_runtime_path(). apparmor.rs resolves "docker" via which().
 • Impact: Inconsistent runtime resolution makes future runtime changes harder and can
   break environments with non-standard docker paths.
 • Plan: Standardize all doctor/app-armor docker invocations to use the resolved path
   (container_runtime_path()). Keep preview strings as "docker ..." for user familiarity.

 2 Notifications execution path: canonicalization and env hygiene

 • Observed: notifications.rs validates absolute exec path and basename allowlist, but
   spawns using the provided path without fs::canonicalize. Env is inherited wholesale.
 • Impact: Symlink surprises are low-risk but worth guarding against. Over-inherited env
   could leak sensitive vars into notification commands in stricter environments.
 • Plan: Canonicalize exec_abs best-effort before spawn; continue using basename of the
   canonicalized path. Provide an opt-in allowlist mode to trim child env by default
   (conservative, disabled unless explicitly enabled by env).

 3 Global env marker for official rust bootstrap

 • Observed: AIFO_RUST_OFFICIAL_BOOTSTRAP is set at start and removed at the end of
   toolchain_run(), but not wrapped by an RAII guard across all code paths (including
   session start flows).
 • Impact: A panic or early error path can leave the marker set for subsequent runs.
 • Plan: Introduce a small RAII guard struct (Drop unsets) scoped to toolchain session
   lifecycle; use in both toolchain_run() and toolchain_start_session().

 4 Proxy streaming backpressure (v2 stdout channel)

 • Observed: v2 streaming uses std::sync::mpsc::channel (unbounded). stderr drain is
   handled; stdout may grow unbounded if the client stalls.
 • Impact: Under heavy concurrent loads, memory usage can spike.
 • Plan: Switch to crossbeam-channel bounded (e.g., 32–128 chunks) with best-effort send:
   try_send, drop or short block + timeout. Emit a single warning line when dropping.
   Preserve current output format and exit codes.

 5 Portability: sed -i usage in container shell script

 • Observed: docker.rs uses sed -i without backup suffix. Busybox vs GNU sed variance
   can bite on future image changes.
 • Impact: Very low in current images; low-effort to harden.
 • Plan: Use sed -i'' -e "<pattern>" everywhere we depend on -i. Gate behind a helper
   function for readability.

 6 Logging consistency

 • Observed: Mixed use of raw eprintln and log_* color-aware helpers. Preview lines are
   mostly consistent but a few places still print raw.
 • Impact: Minor UX inconsistencies and grepping friction.
 • Plan: Prefer crate::log_* in new surfaces. Keep exact message texts unchanged.

 7 Minor cleanups: dead parameters and small helpers

 • Observed: Minor unused parameters (e.g., show(..., _mounted) in doctor.rs), small
   reusable helpers could be centralized (sed portable helper).
 • Impact: Low; improves maintainability.
 • Plan: Remove or rename unused params where safe (no user-facing string changes).
   Centralize portable-sed wrapper.

Status check (implemented vs outstanding)

 • Implemented:
   – Proxy v1 timeout to 504 (buffered path).
   – HTTP parser guardrails: header count and CL mismatch; TE precedence.
 • Outstanding:
   – Runtime harmonization in doctor.rs and apparmor.rs.
   – Notifications canonicalization + optional env allowlist trimming.
   – RAII guard for rust bootstrap marker across session lifecycle.
   – Bounded channel for v2 stdout streaming with drop/backpressure policy.
   – sed -i portability helper and adoption.
   – Logging consistency touch-ups.

Production-ready phase plan

Phase 1: Runtime harmonization and logging (low risk, no UX changes)
 • doctor.rs: replace all Command::new("docker") with resolved runtime path; reuse args
   already constructed. Preview strings remain "docker ...".
 • apparmor.rs: resolve runtime via container_runtime_path() instead of which("docker").
 • Minor logging harmonization: prefer log_* wrappers where we add new messages; keep
   exact texts for existing ones.
 • Tests: no changes required; behavior does not change.

Phase 2: Notifications hardening
 • Canonicalize exec_abs before spawn; use canonicalized basename for allowlist check.
 • Add opt-in AIFO_NOTIFICATIONS_TRIM_ENV=1:
   – When enabled, spawn with a minimal env allowlist (PATH, HOME, LANG, LC_*,
     user-requested vars), omitting sensitive vars by default.
   – Default remains current (no trim) to avoid breaking environments.
 • Tests: add unit tests to ensure canonicalization tolerates failures and preserves
   behavior; add opt-in env trimming tests behind feature/env gates.

Phase 3: Rust bootstrap RAII guard
 • Introduce BootstrapGuard that sets AIFO_RUST_OFFICIAL_BOOTSTRAP on creation and
   unsets in Drop.
 • Use in toolchain_run() and in ToolchainSession (which wraps toolchain_start_session());
   standalone callers invoking toolchain_start_session directly should create a BootstrapGuard
   themselves to keep AIFO_RUST_OFFICIAL_BOOTSTRAP set across preview + exec.
 • Ensure guard lifetime aligns with preview + exec flow and session start lifecycle.
 • Tests: add a small test to assert the env is cleared on early error paths.

Phase 4: Proxy v2 bounded streaming channel
 • Replace unbounded mpsc with crossbeam-channel bounded:
   – Producer: try_send with short retry; on persistent backpressure, drop chunk and
     emit a single warning line "aifo-coder: proxy stream: dropping output (backpressure)".
   – Consumer: unchanged chunked prelude and trailer semantics; exit codes unchanged.
 • Add a small metric counter (in-memory, printed only when AIFO_TOOLCHAIN_VERBOSE=1).
 • Tests: add backpressure simulation under #[ignore] acceptance lane; no change in
   default unit tests.

Phase 5: Sed portability helper
 • Introduce a tiny helper (container script generator) that emits sed -i'' -e ... forms
   instead of plain -i. Replace current sed calls in docker.rs script building.
 • Tests: keep existing preview assertions (strings differ only in sed flags inside the
   container script). Verify no functional change.

Phase 6: Parser fuzzing and documentation
 • Expand http.rs tests: multiple TE headers, malformed chunk sizes, excessive headers,
   mixed CL+TE precedence confirmed.
 • Document proxy HTTP semantics (v1 buffered vs v2 streaming, status vs trailers) in
   docs/TOOLEEXEC_PROTOCOL.md.

Risk management and roll-back

 • Runtime harmonization: zero-risk; guarded by existing container_runtime_path() error
   mapping and identical preview strings.
 • Notifications: canonicalization is best-effort; failures fall back to original path.
   Env trimming is opt-in; default disabled.
 • Bootstrap guard: scope-restricted; Drop guarantees cleanup even on early returns.
 • Bounded streaming: gated by a small bounded size; if regressions appear, fallback to
   unbounded can be feature-gated via AIFO_PROXY_UNBOUNDED=1.
 • Sed helper: only affects container-internal setup; fallback to current behavior if
   helper is disabled via AIFO_SED_PORTABLE=0.

Testing plan (incremental)

 • Unit: notifications canonicalization + allowlist; bootstrap guard lifetime; sed helper.
 • Integration: doctor runtime harmonization on hosts with docker path variations.
 • Acceptance (#[ignore]): proxy backpressure simulation; streaming warning line presence.

Security posture

 • Tighten notifications exec path via canonicalization; optional child env trimming.
 • No new exposed surfaces; auth/proto validation unchanged.
 • Maintain allowlist basenames and argument policies; user-visible texts unchanged.

Observability

 • Optional verbose counters for dropped chunks in v2; single-line warning when dropping.
 • Consider future opt-in JSON log lines gated by AIFO_TOOLCHAIN_JSON_LOG=1; message
   texts remain identical in human-readable mode.

What not to change

 • All user-facing strings (including colors, spacing, punctuation) remain identical.
 • Tool allowlists, image selection strings, preview formats, ordering, and quoting are
   preserved.
 • Notifications policy error messages stay unchanged.

Implementation notes (brief)

 • doctor.rs/app-armor: refactor Command::new("docker") to resolved runtime; reuse args.
 • notifications.rs: canonicalize exec_abs and use canonicalized basename; add env trim
   path guarded by env; default off.
 • sidecar.rs: RAII guard for AIFO_RUST_OFFICIAL_BOOTSTRAP; ensure guard spans preview
   and exec; apply similarly in toolchain_start_session().
 • proxy.rs: crossbeam-channel bounded for v2 stdout; best-effort backpressure handling;
   single warning line on drop; no change to trailer semantics.
 • docker.rs: emit sed -i'' -e forms via a helper; keep rest of script intact.

By following these phases, we harden runtime resolution, improve notifications safety,
reduce proxy memory risk under stalls, and make the container script more portable —
all without changing today’s UX or breaking existing tests.
