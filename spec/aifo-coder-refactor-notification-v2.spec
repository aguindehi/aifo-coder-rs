# AIFO Coder – Notification Refactor (v2) – Specification

Status: ready-for-implementation
Target version: 0.6.0
Date: 2025-09-21

Summary
- Finalize and harden the notification execution flow with a phased, test-frozen
  rollout plan.
- Replace hand-rolled YAML parsing with serde_yaml and enforce an absolute path
  for the configured executable.
- Keep a strict allowlist (default: "say") gated by the basename of the absolute
  executable path; extendable via env.
- Permit controlled dynamic arguments via an optional trailing "{args}"
  placeholder only; exact-args otherwise.
- Implement robust timeout with cooperative termination (TERM then KILL), proper
  HTTP status mapping, and proto gating even in noauth mode.
- Deliver comprehensive unit/integration tests after each phase to freeze
  behavior; no regressions across phases.

Non-goals
- Templating beyond a single trailing "{args}" placeholder.
- Full process-tree termination across all platforms (we terminate the immediate
  child only).
- Adding new notifiers beyond configuration; only path-based configuration with
  allowlist remains in scope.

Definitions
- Config file: AIFO_NOTIFICATIONS_CONFIG env override; else ~/.aider.conf.yml.
- notifications-command: YAML node describing the command to run.
- Allowlist: A list of permitted command basenames (default ["say"]), checked
  against the configured absolute executable path.
- Basename: The filename component of exec_abs (e.g., "/usr/bin/say" -> "say").

1. Configuration model

1.1 Accepted YAML types for notifications-command
- Sequence (YAML array), flow or block: ["<abs-path>/say", "--title", "AIFO", "{args}"]
- Single string (compat-only): parsed via a shell-like tokenizer supporting
  single/double quotes. Prefer YAML sequences.

1.2 Tokens and placeholders
- All tokens are strings; first token MUST be an absolute path (starts with "/")
  to the executable. Non-absolute paths are rejected.
- An optional special last token "{args}" enables request-supplied arguments to
  be appended at that position. Only one "{args}" is allowed and it must be
  strictly trailing.
- Without "{args}", the configured argument vector is exact and immutable
  (request args must equal it).

1.3 Backward compatibility and migration
- Legacy ["say", "--title", "AIFO"] (non-absolute) is rejected with:
  "notifications-command executable must be an absolute path"
- Migration examples:
  - macOS: ["/usr/bin/say", "--title", "AIFO", "{args}"]
  - Linux: ["/usr/bin/notify-send", "--app-name=AIFO", "{args}"]
- Tests must build a stub and reference it by absolute path.

2. Security and policy

2.1 Absolute path
- Resolve and execute only absolute paths from configuration; ignore PATH.

2.2 Allowlist
- Default allowed basenames: ["say"].
- Environment override:
  - AIFO_NOTIFICATIONS_ALLOWLIST: comma-separated basenames; trimmed, deduped,
    capped at 16 entries. This broadened set still requires an absolute path in
    config.
- The request must set cmd to exactly the basename(exec_abs). Otherwise reject.

2.3 Argument validation
- No "{args}": request argv must exactly match fixed_args; else 403.
- With trailing "{args}":
  - Append up to MAX_ARGS from request argv; truncate beyond limit.
  - Default MAX_ARGS = 8; env override via AIFO_NOTIFICATIONS_MAX_ARGS (clamped
    to [1, 32]).
  - Enforce a soft body cap via HTTP parsing (1 MiB already); locally rely on
    arg-count cap.
- Preconfigured flags preceding "{args}" are immutable.

2.4 Environment of child
- Inherit current environment; absolute executable path used; do not rely on
  PATH resolution. Additional sanitization is out-of-scope for v2.

3. Request handling and HTTP semantics

3.1 Endpoint
- POST /notify only.

3.2 Headers
- Authorization: Bearer <token> required unless noauth mode is enabled.
- X-Aifo-Proto: must be "2", enforced even in noauth mode.

3.3 Form encoding
- Content-Type: application/x-www-form-urlencoded
- Fields:
  - cmd: required; equals basename(exec_abs)
  - arg: zero or more; order preserved

3.4 Responses
- Success: 200 OK, X-Exit-Code: <code>, body (stdout + stderr).
- Policy/config violations (forbidden cmd, args mismatch, non-absolute exec):
  403 Forbidden; X-Exit-Code: 86; body: reason + "\n".
- Bad request (missing fields): 400 Bad Request; X-Exit-Code: 86.
- Unauthorized: 401 Unauthorized.
- Unsupported proto: 426 Upgrade Required.
- Exec spawn error: 500 Internal Server Error; X-Exit-Code: 86; body includes
  error message.
- Timeout: 408 Request Timeout; X-Exit-Code: 124; body: "timeout\n".

4. Execution and timeout model

4.1 Spawning and capture
- Use Command::spawn with piped stdout/stderr; do not rely on Command::output
  inside a background thread.
- Capture stdout/stderr; combine by appending stderr after stdout.

4.2 Timeout and termination
- Timeout value:
  - AIFO_NOTIFICATIONS_TIMEOUT_SECS > 0 if set; else reuse global toolexec
    timeout when present; else default 5 seconds.
- On timeout:
  - Send SIGTERM on Unix (or Child::kill() on non-Unix which behaves like
    TerminateProcess).
  - Wait ~250 ms; if still alive (Unix), send SIGKILL; ensure wait/reap.
  - Return Timeout (HTTP maps to 408; X-Exit-Code: 124; body "timeout\n").
- If the deadline is exceeded and the process exits during TERM grace, still
  communicate timeout (conservative UX) and ensure the child was reaped.

5. Auth and noauth

5.1 Authorization
- Bearer token required and checked case-insensitively for the scheme.

5.2 Proto
- X-Aifo-Proto: "2" is mandatory for /notify in both auth and noauth modes. If
  invalid/missing, return 426.

6. Errors and user-facing strings
- "notifications-command executable must be an absolute path"
- "command 'X' not allowed for notifications"
- "only executable basename 'X' is accepted (got 'Y')"
- "arguments mismatch: configured [...] vs requested [...]"
- "host 'X' execution failed: <io-error>"
- "timeout"

7. Implementation plan (phased, with tests to freeze behavior)

Overview: Each phase is small, individually testable, and adds incremental
behavior. Do not progress to the next phase until the included tests are green.
Where a phase changes semantics, include migration notes and adjust tests.

PHASE 0 – Groundwork and invariants
- Document v1 behavior; mark legacy parsing and PATH reliance as deprecated.
- Introduce a new internal struct for config:
  struct NotifCfg {
    exec_abs: PathBuf,
    fixed_args: Vec<String>,
    has_trailing_args_placeholder: bool,
  }
- No functional change yet; just scaffolding and feature flags behind tests.

Tests (Phase 0)
- Build-only tests to ensure NotifCfg structure exists and is private to module.
- Keep existing tests running unchanged.

PHASE 1 – serde_yaml parsing + absolute path validation
- Replace parse_notifications_command_config() internals with serde_yaml:
  - Accept String or Seq<String>.
  - Convert tokens to Vec<String>.
  - Enforce absolute path for argv[0]; error if not absolute.
  - Validate optional trailing "{args}" and disallow elsewhere.
  - Populate NotifCfg accordingly.
- Return specific Err messages per 6.

Migration notes
- Any config using non-absolute executables fails fast with a clear message.
- Tests must switch to writing absolute stub paths in temp directories.

Tests (Phase 1)
- Unit: Parse sequence without placeholder:
  - ["/bin/echo","-n"] => exec_abs=/bin/echo, fixed_args=["-n"], placeholder=false
- Unit: Parse with placeholder:
  - ["/bin/echo","--","{args}"] => exec_abs=/bin/echo, fixed_args=["--"], placeholder=true
- Unit: Reject non-absolute:
  - ["echo","ok"] => Err("notifications-command executable must be an absolute path")
- Unit: Reject placeholder not trailing:
  - ["/bin/echo","{args}","--"] => Err(...)
- Unit: Single string legacy:
  - "/bin/echo -n" parsed accordingly.

PHASE 2 – Allowlist + basename gating
- Compute allowed basenames:
  - Start with ["say"].
  - Merge AIFO_NOTIFICATIONS_ALLOWLIST if set; split by comma, trim, dedup,
    cap to 16 entries; ignore empties.
- Require basename(exec_abs) ∈ allowlist.
- Require request cmd equals basename(exec_abs); else 403 with:
  - "only executable basename 'X' is accepted (got 'Y')"

Tests (Phase 2)
- Unit: Allowlist env extension "say,notify-send" with exec_abs="/usr/bin/notify-send" → allowed.
- Unit: cmd mismatch (cmd="say" but exec_abs="/usr/bin/notify-send") → 403 with message.

PHASE 3 – Argument policy and limits
- If has_trailing_args_placeholder:
  - Truncate request argv to MAX_ARGS; default 8; override via env clamped to
    [1, 32].
  - final_args = fixed_args + request_argv_truncated
- Else:
  - Require exact match: argv == fixed_args; else 403 with mismatch message.

Tests (Phase 3)
- Unit: Exact-match required when no placeholder; mismatch rejected.
- Unit: Placeholder present; with MAX_ARGS=2 and request 3 args; only first 2 used.
- Unit: Body cap honors HTTP soft limit; locally only arg count enforced.

PHASE 4 – Spawn + timeout + capture + error mapping
- Implement execution with Command::spawn; capture stdout/stderr.
- Wait loop: try_wait() at ~25–50 ms cadence; deadline computed from timeout.
- On timeout, signal per 4.2; ensure child wait/reap; return Timeout error
  signalable by caller for HTTP 408 mapping.
- Return (exit_code, stdout+stderr) on success.
- Map spawn errors to Err("host 'X' execution failed: <io-error>").

Tests (Phase 4)
- Unit: Stub that prints and exits 0 -> 200 path verifies combined output.
- Unit: Stub that sleeps longer than timeout -> 408 semantics at proxy level
  and exit code 124; ensure process is terminated (best-effort).

PHASE 5 – Proxy semantics and noauth hardening
- /notify endpoint:
  - Enforce X-Aifo-Proto="2" even when AIFO_NOTIFICATIONS_NOAUTH=1; otherwise
    426 Upgrade Required.
  - Map timeout to 408 with "timeout\n", X-Exit-Code: 124.
  - Map policy/config errors to 403 with X-Exit-Code: 86; include message + "\n".
  - Map spawn errors to 500 with X-Exit-Code: 86 and error body.
- Logging remains consistent; keep small nudge sleep for ordering.

Tests (Phase 5)
- Integration (TCP + optional UDS on Linux):
  - Noauth=1 + X-Aifo-Proto=2 with absolute stub config -> 200, X-Exit-Code:0.
  - Noauth=1 + missing/invalid proto -> 426.
  - Auth path with valid Bearer and proto -> 200, as above.
  - Timeout stub with AIFO_NOTIFICATIONS_TIMEOUT_SECS=1 -> 408, 124, "timeout\n".

PHASE 6 – Feature freeze and negative policy coverage
- Disallowed cmd (allowlist default ["say"] while exec_abs is notify-send) -> 403.
- Basename mismatch (exec_abs ends with "say"; request cmd="not-say") -> 403.
- Exact args required (no placeholder) -> mismatch 403 with mismatch detail.

Tests (Phase 6)
- Integration tests extending Phase 5:
  - Disallowed cmd: 403 with message.
  - Basename mismatch: 403.
  - Exact args mismatch: 403, message includes both vectors.

PHASE 7 – Wrapper alignment and documentation notes
- Keep public wrapper notifications_handle_request(argv, ...) behaving as:
  - Equivalent to cmd=basename(exec_abs); without placeholder, argv must equal
    fixed_args; with placeholder, argv appended (after truncation).
- Update docs (README/TOOLCHAINS) with absolute path requirement, allowlist env,
  "{args}" placeholder semantics/limits, and proto requirement in noauth.

Tests (Phase 7)
- Unit wrapper tests:
  - Absolute stub + fixed args -> success.
  - With "{args}" config; dynamic args appended -> success.
  - Mismatch without placeholder -> Err.

8. Test harness organization

8.1 Unit tests
- src/toolchain/notifications.rs:
  - Parsing, placeholder validation, allowlist/basename, arg policy, max args.
  - Timeout behavior: return indicative error mapped by proxy to HTTP 408.
- src/toolchain/auth.rs and http.rs remain unchanged except proto gating reused.

8.2 Integration tests
- tests/notify_proxy.rs (update to absolute paths; already includes X-Aifo-Proto).
- tests/notify_policy.rs (new): negative policy cases per Phase 6.
- tests/notify_timeout.rs (new): spawn long-running stub and verify timeout.

8.3 Backward-compat guard tests
- Legacy non-absolute config rejected with canonical message.

8.4 UDS transport (Linux-only)
- tests/notify_proxy_uds.rs (optional/ignored): parity with TCP behavior.

9. Performance considerations
- Short-lived processes; spawn+wait is adequate.
- No thread-per-request leak on timeout; child always reaped.
- YAML parse overhead negligible.

10. Observability and toggles
- AIFO_NOTIFICATIONS_ALLOWLIST: extend basenames.
- AIFO_NOTIFICATIONS_MAX_ARGS: clamp 1–32; default 8.
- AIFO_NOTIFICATIONS_TIMEOUT_SECS: per-notify timeout; default 5 (or global
  tool timeout when set).
- AIFO_NOTIFY_PROXY_NUDGE_MS: small sleep to aid log ordering (default 15 ms).

11. Rollout and migration
- Phase-by-phase PRs recommended; keep commits small and tests green.
- Update any existing tests using PATH-based "say" to absolute stubs.
- Bump minor version to 0.6.0 after Phase 6 freezes policy behavior.

Appendix A – Data structures and pseudocode

struct NotifCfg {
    exec_abs: PathBuf,                  // absolute path to executable
    fixed_args: Vec<String>,            // arguments before {args} or all args
    has_trailing_args_placeholder: bool // true iff last token is "{args}"
}

parse_config() -> Result<NotifCfg, String> {
    // read file path from env or ~/.aider.conf.yml
    // serde_yaml::from_str to Value; accept String or Seq<String>
    // normalize to Vec<String>, verify !empty
    // enforce absolute argv[0]; error if not starting with '/'
    // detect "{args}" trailing only; error if elsewhere or multiple occurrences
    // split into exec_abs, fixed_args, has_placeholder
}

handle_request(cmd, argv_req, timeout) -> Result<(i32, Vec<u8>), ErrorKind> {
    let cfg = parse_config()?;
    let basename = cfg.exec_abs.file_name().unwrap().to_string_lossy();
    let allow = compute_allowlist();
    if !allow.contains(basename) {
        return Err(Policy("command 'X' not allowed for notifications"));
    }
    if cmd != basename {
        return Err(Policy(format!("only executable basename '{}' is accepted (got '{}')", basename, cmd)));
    }

    let mut exec_args = cfg.fixed_args.clone();
    if cfg.has_trailing_args_placeholder {
        let cap = clamp_env(AIFO_NOTIFICATIONS_MAX_ARGS, 1, 32, 8);
        exec_args.extend(argv_req.iter().take(cap).cloned());
    } else if exec_args != argv_req {
        return Err(Policy(format!("arguments mismatch: configured {:?} vs requested {:?}", exec_args, argv_req)));
    }

    let mut child = Command::new(&cfg.exec_abs)
        .args(&exec_args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| ExecError(format!("host '{}' execution failed: {}", basename, e)))?;

    let deadline = Instant::now() + Duration::from_secs(timeout);
    loop {
        if let Some(status) = child.try_wait().map_err(|e| ExecError(...))? {
            let mut out = read_all(child.stdout.take());
            let mut err = read_all(child.stderr.take());
            out.extend_from_slice(&err);
            return Ok((status.code().unwrap_or(1), out));
        }
        if Instant::now() >= deadline {
            // timeout; cooperative termination
            term_kill_gracefully(&mut child);
            let _ = child.wait();
            return Err(Timeout); // proxy maps to 408 + 124 + "timeout\n"
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

Appendix B – Canonical error mapping in proxy
- Policy/config Err -> 403; X-Exit-Code: 86; "<message>\n"
- ExecError spawn -> 500; X-Exit-Code: 86; "<message>\n"
- Timeout -> 408; X-Exit-Code: 124; "timeout\n"
- Success -> 200; X-Exit-Code: <child-exit>; "<stdout><stderr>"
