Refactor: unify stderr logging via color-aware helpers (log_*)

Rationale
- Consistency: single place decides color (TTY/NO_COLOR/AIFO_CODER_COLOR).
- Safety: preserve exact message text; helpers only wrap color codes.
- Scope control: apply to stderr one-liners; keep structured/stdout/streaming as-is.

Non-goals
- Do not alter stdout outputs (lists, JSON, summaries) or message text.
- Do not touch proxy/shim streaming, CR/LF formatting, or tee-to-file behavior.
- Do not rewrite multi-line, structured color formatting (banner.rs, doctor.rs).

Policy
- Use aifo_coder::log_info_stderr for informational stderr lines.
- Use aifo_coder::log_warn_stderr for warnings and “note:” lines.
- Use aifo_coder::log_error_stderr for errors and refusals.
- Always precompute let use_err = aifo_coder::color_enabled_stderr(); in the scope and reuse it.
- Keep the message string identical to today (no rewording, no punctuation changes).

Exclusions
- src/toolchain/proxy.rs (special CR/LF logs, tee to file, timing constraints).
- src/bin/aifo-shim.rs (stdout/stderr interleaving, CR/LF-cued UX).
- src/banner.rs (structured, per-token colors).
- src/doctor.rs (structured, per-token colors and columns).
- Any function that prints to stdout (keep as-is).

Phased rollout (production-friendly)

Phase 0: Inventory and guardrails
- Add this spec.
- Document the policy above in src/color.rs (doc comment only).
- No functional change.

Phase 1: Sidecar and mounts (low risk) (1–2 commits)
- src/toolchain/sidecar.rs:
  - ensure_network_exists/remove_network: docker preview lines => log_info_stderr.
  - choose_session_network: failure line => log_warn_stderr.
  - toolchain_run: run/exec preview and cleanup preview => log_info_stderr.
- src/toolchain/mounts.rs:
  - init_rust_named_volume/init_node_cache_volume: verbose docker preview => log_info_stderr.

Phase 2: CLI preview/info surfaces
- src/main.rs:
  - print_verbose_run_info: all stderr eprintln => log_info_stderr.
  - “dry-run requested; not executing Docker.” => log_info_stderr.
- src/commands/mod.rs:
  - run_images: “aifo-coder images” header => log_info_stderr.
  - run_toolchain: kind/image/no-cache informational lines => log_info_stderr.

Phase 3: Registry diagnostics
- src/registry.rs:
  - Diagnostics “checking…”, “reachable…using…”, “not reachable…”, “curl not found…”
    => log_info_stderr for positive info, log_warn_stderr for fallbacks.
  - Keep quiet variants unchanged.

Phase 4: Fork error paths
- src/fork/preflight.rs:
  - All error eprintln => log_error_stderr (git/tmux/Windows tool errors).
- src/fork/runner.rs:
  - Repo-required, base-detection and cloning errors => log_error_stderr.
  - Keep stdout summaries and guidance intact.

Phase 5: Optional notices (can be deferred)
- src/fork_impl/notice.rs:
  - “Found N old … Consider …” => log_info_stderr.
  - “Auto-clean: removed …” summary => log_info_stderr.

Small examples (apply pattern consistently; message text unchanged)
- Info:
  Before: eprintln!("aifo-coder: docker: {}", preview);
  After:  let use_err = aifo_coder::color_enabled_stderr();
          aifo_coder::log_info_stderr(use_err, &format!("aifo-coder: docker: {}", preview));

- Warn:
  Before: eprintln!("aifo-coder: warning: failed to create session network {}", net);
  After:  let use_err = aifo_coder::color_enabled_stderr();
          aifo_coder::log_warn_stderr(
            use_err,
            &format!("aifo-coder: warning: failed to create session network {}", net),
          );

- Error:
  Before: eprintln!("aifo-coder: error: tmux not found. Please install tmux to use fork mode.");
  After:  let use_err = aifo_coder::color_enabled_stderr();
          aifo_coder::log_error_stderr(
            use_err,
            "aifo-coder: error: tmux not found. Please install tmux to use fork mode.",
          );

Acceptance criteria
- No change to stdout output (content or ordering).
- No change to message text literals (only color wrapping when enabled).
- All tests pass under non-TTY CI (helpers avoid color in non-TTY).
- Manual spot-check: with a TTY, info=cyan, warn=yellow, error=red for converted lines.

Testing plan
- Unit/integration:
  - Build docker preview tests: unaffected (they use returned preview strings).
  - Fork tests: verify error branches still show the same text.
  - Registry env tests: unaffected for “quiet”; manual run to verify diagnostics.
- Manual:
  - Run with and without TTY; confirm colors flip correctly (NO_COLOR and AIFO_CODER_COLOR).
  - Smoke-run “--dry-run --verbose” for aider/crush/codex to see preview lines.

Risk analysis and mitigations
- Risk: ANSI sequences appear where tests expect plain text.
  Mitigation: CI runs non-TTY; helpers disable color. Keep message text identical.
- Risk: Proxy/shim output order changes.
  Mitigation: Excluded from this refactor.
- Risk: Accidental stdout changes.
  Mitigation: Scope explicitly limited to stderr eprintln calls.

Rollout and rollback
- Land phases sequentially; each commit compiles and passes tests.
- If any regression appears, revert the latest phase commit(s) only; phases are independent.

Ownership
- Implementation: core maintainers of toolchain and fork modules.
- Reviewer focus: verify string literals unchanged, only wrapper adoption.

Change map (file-level checklist)
- src/toolchain/sidecar.rs: info+warn conversions as above.
- src/toolchain/mounts.rs: verbose docker preview => info.
- src/main.rs: print_verbose_run_info + dry-run => info.
- src/commands/mod.rs: run_images header + run_toolchain info lines => info.
- src/registry.rs: diagnostics => info/warn (quiet variant untouched).
- src/fork/preflight.rs: errors => error.
- src/fork/runner.rs: errors => error.
- Optional: src/fork_impl/notice.rs: summaries => info.
- Excluded: proxy.rs, bin/aifo-shim.rs, banner.rs, doctor.rs, stdout printers.

Timeline (optimal)
- Day 1: Phase 1 + 2.
- Day 2: Phase 3 + 4.
- Day 3: Phase 5 (optional) + manual validation sweep.

Notes
- Prefer one “use_err” per function scope, not per log line.
- Do not add explicit flushes; keep existing buffering behavior.
- Respect the existing casing/prefix (“aifo-coder: …”, “warning: …”, “error: …”).
