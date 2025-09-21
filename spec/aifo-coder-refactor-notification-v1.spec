# AIFO Coder – Notification Refactor (v1) – Specification

Status: draft-for-implementation
Target version: 0.5.x
Date: 2025-09-21

Summary
- Refactor notification execution to be secure-by-default, robust, and well-tested.
- Replace ad hoc YAML parsing with serde_yaml.
- Require an absolute executable path in configuration.
- Keep a strict allowlist (default: "say"), configurable via env, still enforced.
- Support controlled dynamic arguments via a simple pattern with a trailing "{args}" placeholder.
- Implement hard timeouts with cooperative termination (TERM then KILL).
- Improve HTTP status semantics and proto gating, even in noauth mode.
- Add comprehensive unit/integration tests and freeze the feature via this spec.

Non-goals
- Designing a generic templating language beyond a single optional trailing "{args}" placeholder.
- Cross-platform process tree termination for all OSes; we will terminate the direct child process.
- Implementing additional notifiers beyond the allowlist default; we only enable configuration paths.

Definitions
- Config file: AIFO_NOTIFICATIONS_CONFIG env override; else ~/.aider.conf.yml.
- notifications-command: YAML node describing the command to run.
- Allowlist: List of permitted command basenames (default: ["say"]), further restricted by absolute path requirement.

1. Configuration format

1.1 Accepted YAML types for notifications-command
- Sequence (YAML array): e.g., ["<abs-path>/say", "--title", "AIFO", "{args}"]
- Flow-style sequence: same as above.
- Block sequence ("- item" style): items converted to strings, no further splitting.
- Single string (discouraged): Interpreted as a shell-like token list, with single/double quotes. Use only as compatibility. Prefer YAML sequences.

1.2 Tokens and placeholders
- Each token must be a string.
- The first token MUST be an absolute path (starts with "/") to the executable. Relative/bare names are rejected.
- The special trailing placeholder "{args}" is optional; when present, all request-supplied arguments are appended at this position.
  - Only a single trailing "{args}" placeholder is allowed and must be the last token if used.
  - If "{args}" is not present, the executed arguments must match exactly the tokens after the executable (no dynamic args allowed).

1.3 Backward compatibility and migration
- Legacy configurations that specify ["say","--title","AIFO"] (no absolute path) are invalid under the new policy and will be rejected with a clear error:
  - "notifications-command executable must be an absolute path"
- Migration guidance:
  - Determine the absolute path of your notifier and set it explicitly, for example:
    - macOS: ["/usr/bin/say", "--title", "AIFO", "{args}"]
    - Linux: ["/usr/bin/notify-send", "--app-name=AIFO", "{args}"]
- For tests, construct a stub in a temp dir and reference it with an absolute path in the config.

2. Security and policy

2.1 Absolute path requirement
- The configured executable must be an absolute path; PATH resolution is not used.
- Rejection on non-absolute executable reduces PATH-based hijacking risk.

2.2 Allowlist enforcement
- Default allowlist: ["say"] (the basename of the executable path).
- The request must set cmd equal to the basename of the configured executable.
- Optional environment override:
  - AIFO_NOTIFICATIONS_ALLOWLIST: comma-separated basenames to extend the allowlist (e.g., "say,notify-send,terminal-notifier").
  - The allowlist still requires an absolute path in config; allowlist only broadens which cmd values are permitted.

2.3 Argument validation
- When config does not include "{args}":
  - The request MUST provide arguments that exactly match the configured argument vector (strict equality).
- When config includes trailing "{args}":
  - The request-supplied arguments are appended in that position (zero or more).
  - Limits:
    - Max argument count from request: default 8; override via AIFO_NOTIFICATIONS_MAX_ARGS (1–32; values outside are clamped).
    - Max total encoded body size: 16 KiB (already enforced at HTTP layer effectively; keep a local guard).
  - For safety, do not permit request to change pre-configured flags preceding "{args}".

2.4 Environment for the child process
- Inherit current process environment (no env_clear), but do not rely on PATH for command resolution (absolute path used).
- Optionally in future, a sanitized environment can be introduced; out-of-scope for v1.

3. Request handling and HTTP semantics

3.1 Endpoint and method
- POST /notify only.

3.2 Headers
- Authorization: Bearer <token> required unless noauth mode is enabled.
- X-Aifo-Proto: must be "2". This is required even in noauth mode as an extra guard.

3.3 Form encoding
- Content-Type: application/x-www-form-urlencoded.
- Fields:
  - cmd: required (basename like "say").
  - arg: zero or more fields; order preserved.

3.4 Responses
- Success (execution finished): HTTP 200 with X-Exit-Code: <code> and output body (stdout+stderr).
- Policy violation (forbidden cmd or arg mismatch or non-absolute executable): HTTP 403; X-Exit-Code: 86; body: reason + "\n".
- Bad request (missing fields): HTTP 400; X-Exit-Code: 86.
- Unauthorized: HTTP 401.
- Unsupported proto: HTTP 426.
- Method not allowed: HTTP 405.
- Not found: HTTP 404 (not applicable here).
- Exec error (spawn failed): HTTP 500; X-Exit-Code: 86; body includes error message.
- Timeout: HTTP 408; X-Exit-Code: 124 (conventional timeout exit); body: "timeout\n".

4. Execution and timeout behavior

4.1 Process spawning and output capture
- Use Command::spawn with piped stdout/stderr, not Command::output inside a background thread.
- Read output to a buffer concurrently or serially (a simple join onto a single thread after exit is acceptable since the child buffers will drain fully once the process exits).
- Combine stdout and stderr in response (stderr appended after stdout as in current behavior).

4.2 Timeout and termination
- Timeout value:
  - Use AIFO_NOTIFICATIONS_TIMEOUT_SECS > 0 if set; else if global toolexec timeout is set, reuse it; else default 5 seconds.
- Strategy on timeout:
  - Send SIGTERM (Unix) or Child::kill() directly on non-Unix (which behaves like TerminateProcess on Windows).
  - Wait a short grace period (250 ms) for exit.
  - If still running and on Unix, send SIGKILL.
  - Ensure the child handle is waited on (reaped) to prevent zombie processes.
- Return HTTP 408 and X-Exit-Code: 124 on timeout. If the process dies during grace before we respond, still treat as timeout if the deadline was exceeded.

5. Auth and noauth

5.1 Authorization
- Use existing Bearer auth; case-insensitive scheme; token must match exactly.

5.2 Proto requirement (even in noauth)
- In noauth mode (AIFO_NOTIFICATIONS_NOAUTH=1) we still require X-Aifo-Proto: "2".
- If missing/invalid, return 426 Upgrade Required.

6. Backward compatibility

- Existing tests that relied on PATH-resolved "say" will need to set notifications-command to an absolute path to the stub, and maintain "cmd=say" in the request (cmd equals basename).
- The existing library wrapper notifications_handle_request(argv, ...) will now resolve using the configured absolute executable and behave per the new policy:
  - When called with argv it implies config has no "{args}" placeholder.
  - If "{args}" exists in config, wrapper semantics: append provided argv at "{args}" position.

7. Implementation plan

7.1 Dependencies
- Add serde_yaml = "0.9" (or the current compatible version).
- No other third-party deps required.

7.2 Module changes
- src/toolchain/notifications.rs:
  - Replace parse_notifications_command_config() with serde_yaml-based parsing:
    - Accept String or Seq<String>.
    - Validate absolute path for the first token; return Err otherwise.
    - Validate optional trailing "{args}" placeholder; else disallow placeholder elsewhere.
    - Return a struct:
      - exec_abs: PathBuf (absolute).
      - fixed_args: Vec<String> (arguments before placeholder or all args if no placeholder).
      - has_trailing_args_placeholder: bool.
  - Replace notifications_handle_request signature to:
    - Keep existing signature but internally:
      - Read and validate config (absolute path, allowlist).
      - Enforce cmd equals basename(exec_abs).
      - If has placeholder: cap request.argv per max args and append to exec args.
      - Else: require exact argv match to fixed_args.
      - Spawn and manage timeout with termination as specified.
      - Return (exit_code, stdout+stderr) or Err(reason) for policy/validation failures.
- src/toolchain/proxy.rs:
  - /notify path:
    - Require proto "2" also when noauth mode is active.
    - Map timeout error to HTTP 408 with "timeout\n", exit code 124.
    - Map policy/config errors to 403 with exit code 86; include message body with newline.
- src/toolchain/auth.rs:
  - No change beyond ensuring validate_auth_and_proto behavior is reused.

7.3 Allowlist resolution
- Compute allowed basenames:
  - Start with ["say"].
  - If AIFO_NOTIFICATIONS_ALLOWLIST is set and non-empty, split by comma; trim; keep non-empty basenames; dedup; limit to 16 entries.
- Check that basename(exec_abs) is contained within the allowlist.
- Compare request cmd to basename(exec_abs); else reject 403.

7.4 Limits and constants
- Max request args: default 8; env AIFO_NOTIFICATIONS_MAX_ARGS to override (clamp 1–32).
- Max total request body size: rely on existing HTTP parser cap (1 MiB body cap is present), but enforce logical args count limit locally.

7.5 Errors and messages
- Use concise, user-facing error messages; always terminate body with newline.
  - "notifications-command executable must be an absolute path"
  - "command 'X' not allowed for notifications"
  - "only executable basename 'X' is accepted (got 'Y')"
  - "arguments mismatch: configured [...] vs requested [...]"
  - "host 'X' execution failed: <io-error>"
  - "timeout"

8. Test plan

8.1 Unit tests (Rust)
- src/toolchain/notifications.rs:
  - Parse sequence absolute path w/o placeholder:
    - ["/bin/echo","-n"] -> exec_abs="/bin/echo", fixed_args=["-n"], has_placeholder=false.
  - Parse sequence with placeholder:
    - ["/bin/echo","--","{args}"] -> exec_abs="/bin/echo", fixed_args=["--"], has_placeholder=true.
  - Reject non-absolute:
    - ["echo","ok"] -> Err.
  - Reject placeholder not trailing:
    - ["/bin/echo","{args}","--"] -> Err.
  - Single string legacy:
    - "/bin/echo -n" -> parsed accordingly.
  - Allowlist:
    - AIFO_NOTIFICATIONS_ALLOWLIST="say,notify-send" with exec_abs="/usr/bin/notify-send" -> allowed.
    - cmd mismatch: cmd="say" but exec_abs="/usr/bin/notify-send" -> 403.
  - Max args clamp:
    - Set AIFO_NOTIFICATIONS_MAX_ARGS=2; "{args}" + 3 request args -> only first 2 used (or reject? Choose: truncate to 2 and proceed). Document truncation in code comments and test it.

8.2 Integration tests (proxy)
- tests/notify_proxy.rs (adapt/extend):
  - Configure config file with absolute path to a stub "say" script (write to temp dir, absolute path).
  - Set PATH to include that directory, but rely on absolute path in config.
  - Noauth=1 and X-Aifo-Proto=2:
    - POST /notify with cmd=say&arg=--title&arg=AIFO; expect 200 and X-Exit-Code: 0, body contains stub output.
  - Auth required (no noauth):
    - Start proxy normally; build Authorization header from token; include X-Aifo-Proto=2; expect 200 and body.
  - Proto missing:
    - With noauth=1 but omit X-Aifo-Proto; expect 426 Upgrade Required.

8.3 Negative policy cases (proxy)
- Disallowed cmd:
  - Allowlist default ["say"]; set config exec_abs="/usr/bin/notify-send"; request cmd=notify-send; expect 403.
- Basename mismatch:
  - exec_abs ends with "say"; request cmd="not-say"; 403.
- Exact args required:
  - config ["/abs/say","--title","AIFO"] (no placeholder); request arg differs; 403 and message includes mismatch.

8.4 Timeout test (proxy)
- Stub that sleeps longer than timeout then prints something; configure timeout env to 1 second.
- Expect 408 and X-Exit-Code: 124; body "timeout\n".
- Optionally ensure the process was killed by verifying it doesn’t run beyond test completion (best-effort).

8.5 Wrapper function tests
- tests/notifications_unit.rs should:
  - Write config with absolute path to stub; call library wrapper aifo_coder::notifications_handle_request with args matching fixed args -> success.
  - Same with "{args}" config; pass dynamic args; success and output contains them.
  - Mismatch without placeholder -> Err.

8.6 Backward-compat guard tests
- Ensure legacy non-absolute config returns clear Err message.

9. Documentation and migration notes

- Update README/TOOLCHAINS docs (future PR) to:
  - Show absolute path examples for notifications-command.
  - Explain allowlist and how to extend via env.
  - Document "{args}" placeholder and limits.
  - Mention that X-Aifo-Proto: 2 is required even in noauth mode.

10. Performance considerations

- Notifications are short-lived; the spawn+wait path is fine.
- No background threads remain on timeout (child is terminated and reaped).
- YAML parsing cost is negligible.

11. Rollout plan

- Implement code changes.
- Update tests shown above (adapt existing ones to absolute path).
- Ensure CI green across Linux/macOS.
- Bump minor patch version and document change.

12. Open questions (to revisit later)

- Should we support environment variable substitution inside config args? (Out-of-scope.)
- Should we allow a fixed set of notifiers per platform by default? (For now, require absolute path and allowlist opt-in.)

Appendix A – Pseudocode for notifications_handle_request

- Read config -> {exec_abs, fixed_args, has_placeholder}
- Build allowlist basenames from default + env.
- Require basename(exec_abs) in allowlist.
- Require request cmd equals basename(exec_abs).
- If has_placeholder:
  - Take up to MAX_ARGS from request argv; args = fixed_args + request_argv_truncated
- Else:
  - Require request argv == fixed_args; else Err(mismatch)
- Spawn Command::new(exec_abs)
  - args(args)
  - stdout/stderr piped
- Wait with deadline:
  - Loop try_wait at 25–50ms cadence; break when exit or deadline.
  - On timeout:
    - TERM (Unix) or kill(); wait 250ms; KILL if Unix and still alive; wait.
    - Return Timeout (HTTP maps to 408, exit code 124).
- On success:
  - Read stdout/stderr; combine; return (exit_code, combined_bytes).
