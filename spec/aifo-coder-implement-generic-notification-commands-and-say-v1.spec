# Spec: Generic notifications over /notify with `say` shim (v1)

Summary
- Implement a generic notifications execution pipeline via POST /notify for host-side commands defined in ~/.aider.conf.yml at notifications-command.
- Provide a `say` shim in the agent container that triggers /notify.
- Generalize the request format to support multiple notification commands (cmd + args).
- Remove the legacy notifications-cmd shim.
- Provide verbose logging for /notify comparable to /exec.
- Support both transports: TCP (http://) and Linux Unix Domain Sockets (unix://).
- Maintain strong validation and security constraints.

Non-goals
- Implement any specific host commands beyond validating and executing the configured notifications-command.
- Add rate limiting or auditing (can be addressed in a follow-up).

Terminology
- Agent container: container hosting the coding agent and shims.
- Proxy: in-process HTTP server bridging shims to sidecars or host (notifications).
- Notification command: host executable configured in ~/.aider.conf.yml under notifications-command (e.g., ["say","--title","AIFO","hello"]).
- Notification tool: a shim name in the agent container (e.g., say) routed to /notify instead of /exec.

Functional requirements
1) Say shim
   - Invoking say … inside the agent container must send a POST to /notify with:
     - Headers:
       - Authorization: Bearer <token>
       - X-Aifo-Proto: 2
       - Content-Type: application/x-www-form-urlencoded
     - Body (urlencoded):
       - cmd=<program>, e.g., "say"
       - arg=<value> for each argument (repeated, preserving order)
   - Transport support:
     - TCP: http://host:port/notify
     - Linux UDS: unix://<path>, request path /notify, Host: localhost
   - Output/exit:
     - Stream proxy response body to stdout (combined stdout/stderr from host command).
     - Exit with value from X-Exit-Code header (default 1 if missing).

2) Generic notifications
   - Extend the pipeline so additional notification tools can be added by:
     - Adding the tool name to a list of “notification tools” in the shim writer and Docker symlink loops.
     - The shim posts cmd=<that-tool> with arg=… to /notify.
     - The proxy validates program and args exactly against ~/.aider.conf.yml.

3) Security and validation
   - Proxy enforces Authorization and X-Aifo-Proto (unless AIFO_NOTIFICATIONS_NOAUTH=1 in controlled test environments).
   - Proxy validates:
     - cmd must match the configured executable in notifications-command.
     - argv must exactly match the configured arguments (strict equality).
   - Optional allowlist of safe executables (start with ["say"]); fail closed for unknown commands.
   - No shell interpretation in the proxy; spawn directly with argv; no environment injection.

4) Observability
   - Verbose mode:
     - Shim prints “variant”, “transport”, “notify cmd”, “argv joined”, and “preparing request to … (proto=2)”.
     - Proxy logs:
       - Parsed notify request line (cmd, argv, cwd for consistency).
       - Result line using existing uniform format and including kind=notify and duration.

5) Deprecation/removal
   - Remove notifications-cmd shim entirely (shim writer and Docker images).
   - Update tests accordingly.

Design

A) Native Rust shim changes (src/bin/aifo-shim.rs)
- Detect notification mode:
  - Extract tool name from argv[0] basename.
  - If the tool is in NOTIFY_TOOLS (initial: ["say"]), follow the notify path; otherwise run existing /exec path.
- Implement try_notify_native(url, token, cmd, args, verbose) -> Option<i32>:
  - Honor AIFO_SHIM_NATIVE_HTTP=0 to disable native HTTP and force curl fallback.
  - Build application/x-www-form-urlencoded body:
    - Percent-encode per standard:
      - ' ' => '+'
      - Alnum, '-', '_', '.', '~' unescaped
      - Other bytes => %HH uppercase
    - Format: "cmd=<...>&arg=<...>&arg=<...>" (preserve arg order).
  - For unix://…:
    - Connect to the socket; send “POST /notify HTTP/1.1”, Host: localhost, add Authorization, X-Aifo-Proto, Content-Type, Content-Length, Connection: close.
  - For http://…:
    - Parse host[:port], path = "/notify"; same headers with Host set to host.
  - Read response:
    - Tolerant header detection (CRLFCRLF or LFLF).
    - Parse “X-Exit-Code:” case-insensitively; default to 1 if missing.
    - Stream body to stdout; return Some(exit_code).
  - On connection/protocol errors, return None to fall back to curl.
- Curl fallback for notify:
  - Build: curl -sS -D <hdr> -X POST -H Authorization -H "X-Aifo-Proto: 2" -H "Content-Type: application/x-www-form-urlencoded".
  - For unix://, add --unix-socket and URL http://localhost/notify.
  - For http://, derive base by trimming trailing /exec from AIFO_TOOLEEXEC_URL if present; post to base/notify.
  - Add --data-urlencode "cmd=$tool" and one --data-urlencode "arg=$a" per arg.
  - Parse X-Exit-Code from header; default to 1 if missing.
  - Exit with parsed code.
- Shim verbose logging:
  - “aifo-shim: variant=rust transport={native|curl}”
  - “aifo-shim: notify cmd=<cmd> argv=<joined>”
  - “aifo-shim: preparing request to <url-or-unix> (proto=2)”
- Notifications mode intentionally skips ExecId/signal/streaming/disconnect logic.

B) POSIX shim writer changes (src/toolchain/shim.rs)
- SHIM_TOOLS:
  - Remove "notifications-cmd".
  - Add "say".
- Generated aifo-shim script:
  - Early branching on tool="$(basename "$0")":
    - If tool is in NOTIFY_TOOLS (initially "say"):
      - Optional verbose block when AIFO_TOOLCHAIN_VERBOSE=1:
        - Print variant=curl, notify cmd, argv, “preparing request to … (proto=2)”.
      - Invoke curl similarly to native fallback:
        - Headers: Authorization, X-Aifo-Proto: 2, Content-Type: application/x-www-form-urlencoded.
        - Data: --data-urlencode "cmd=$tool" and repeated “arg=$a”.
        - Transport:
          - unix://… => --unix-socket "$SOCKET"; URL="http://localhost/notify"
          - http://… => URL="${AIFO_TOOLEEXEC_URL%/exec}/notify"
      - Capture headers via -D "$tmp/h"; extract "X-Exit-Code:" (strip CR) and default to 1 if missing.
      - Exit with the parsed code.
      - return (so the /exec flow isn’t executed).
  - Keep existing /exec behavior for non-notify tools (incl. ExecId/signals).

C) Proxy changes (src/toolchain/proxy.rs)
- Endpoint /notify:
  - Already classified in http.rs as Endpoint::Notifications.
- Request parsing for /notify:
  - Merge query and body pairs.
  - Extract cmd=<program> (string).
  - Collect arg=<value> pairs to Vec<String> in order.
- Verbose logging for notify:
  - Immediately after parsing and before execution:
    - log_stderr_and_file("\r\naifo-coder: proxy notify parsed cmd=<cmd> argv=<joined> cwd=<cwd>\r\n\r")
  - Record started = Instant::now().
  - After execution:
    - log_request_result(verbose, cmd, "notify", code, &started)
      - This prints a uniform result line including code and duration in ms.
- Auth and timeout:
  - If AIFO_NOTIFICATIONS_NOAUTH=1, skip auth; otherwise validate auth/proto.
  - Timeout for notify:
    - AIFO_NOTIFICATIONS_TIMEOUT_SECS if set (>0),
    - else session/global timeout (>0),
    - else default 5 seconds.
- Call notifications::notifications_handle_request(cmd, &argv, verbose, timeout).
- Responses:
  - Success: 200 OK, X-Exit-Code: <status>, Content-Length, body with combined stdout/stderr.
  - Error (mismatch, missing config, disallowed cmd, timeout, etc.): 403 Forbidden with X-Exit-Code: 86 and a reason message; other statuses (400/401/426) as appropriate.

D) Notifications executor changes (src/toolchain/notifications.rs)
- API:
  - Change to notifications_handle_request(cmd: &str, argv: &[String], verbose: bool, timeout_secs: u64) -> Result<(i32, Vec<u8>), String>
  - Optionally keep a backward-compatible thin wrapper mapping to cmd="say" if any external call sites depend on the old signature; prefer updating all internal call sites.
- Validation:
  - Read config from AIFO_NOTIFICATIONS_CONFIG or ~/.aider.conf.yml.
  - Parse to argv (program + args).
  - Enforce exact equality: cfg_prog == cmd AND cfg_args == argv, else Err(“… mismatch …”).
  - Optional allowlist: reject cmd not in ["say"] with a clear reason.
- Execution:
  - Spawn Command::new(cmd).args(argv).
  - Join output with a timeout: recv_timeout(Duration::from_secs(timeout_secs)).
  - On success, return (status_code, stdout ||+stderr).
  - On failure to spawn => Err("failed to execute host '<cmd>': …").
  - On timeout => Err("host '<cmd>' execution timed out").
- Security:
  - No shell execution (no “sh -c”), no env injection, no path expansion.

E) HTTP helpers (src/toolchain/http.rs)
- Ensure "/notify" is mapped to Endpoint::Notifications (already present).
- Keep tolerant header parsing and 64 KiB header cap; 1 MiB soft cap for bodies.

F) Dockerfile changes
- In both “base” and “base-slim” stages:
  - Remove notifications-cmd from the symlink loop.
  - Add say to the symlink loop mapping to /opt/aifo/bin/aifo-shim.
  - Example:
    - for t in cargo rustc node npm npx tsc ts-node python pip pip3 gcc g++ cc c++ clang clang++ make cmake ninja pkg-config go gofmt say; do ln -sf aifo-shim "/opt/aifo/bin/$t"; done

G) Logging formats (verbose)
- Shim:
  - “aifo-shim: variant=rust transport=native” or “transport=curl”
  - “aifo-shim: notify cmd=<cmd> argv=<joined>”
  - “aifo-shim: preparing request to <url> (proto=2)”
- Proxy parsed (notify):
  - “\r\naifo-coder: proxy notify parsed cmd=<cmd> argv=<joined> cwd=<cwd>\r\n\r”
- Proxy result:
  - “\r\n\raifo-coder: proxy result tool=<cmd> kind=notify code=<code> dur_ms=<ms>\r\n\r”

Error handling and exit codes
- Authorization/proto missing or invalid: 401 or 426; X-Exit-Code: 86; body with reason.
- Config read/parse error: 403; X-Exit-Code: 86; reason includes file and error.
- Program mismatch / args mismatch: 403; X-Exit-Code: 86; reason includes configured vs requested.
- Timeout: 403; X-Exit-Code: 86; reason "host '<cmd>' execution timed out".
- Missing X-Exit-Code header on notify responses: shim defaults to exit 1 to avoid masking errors.
- Transport-level errors:
  - Native returns None to curl fallback; if curl also fails or headers missing, return exit 1.

Potential issues and mitigations
- Endpoint name drift (/notifications vs /notify):
  - Canonicalize to "/notify" in code comments and request builders; remove old mentions.
- Platform variance (“say” availability):
  - Tests should inject a stub “say” earlier in PATH to avoid platform dependence.
- Security drift when adding new commands:
  - Require explicit entries in allowlist and tests; preserve strict equality validation.

Tests

1) Unit tests
- HTTP classifier (src/toolchain/http.rs):
  - classify_endpoint("/notify") == Some(Endpoint::Notifications)
- Notifications validation (src/toolchain/notifications.rs):
  - With config: ["say","--title","AIFO","ok"]
    - Ok: notifications_handle_request("say", ["--title","AIFO","ok"], …)
    - Err: program mismatch notifications_handle_request("notify-send", ["--title","AIFO","ok"], …)
    - Err: args mismatch notifications_handle_request("say", ["--title","WRONG"], …)
  - Timeout: configure a stub that sleeps > timeout; expect Err("timed out")

2) Shim presence and basic behavior
- tests/shim_writer.rs (extend or add):
  - toolchain_write_shims(tmpdir) creates “say” shim with exec permissions.
  - aifo-shim content still contains "--data-urlencode" and "Authorization: Bearer " and not “Proxy-Authorization:” nor “X-Aifo-Token:”.
- tests/shims_notifications.rs (update):
  - Replace notifications-cmd test with say:
    - Verify existence of say shim.
    - Running say without AIFO_TOOLEEXEC_URL/TOKEN exits 86.

3) Integration tests (optional but recommended)
- TCP transport:
  - Start proxy via toolexec_start_proxy(session, verbose=false).
  - Set AIFO_NOTIFICATIONS_NOAUTH=1 for simplicity (or use token).
  - AIFO_NOTIFICATIONS_CONFIG pointing to a temp .aider.conf.yml:
    - notifications-command: ["say","--title","AIFO","hello"]
  - Ensure a fake “say” is available in PATH that echoes args and exits 0.
  - Use toolchain_write_shims(tmpdir) and run <tmp>/say --title AIFO hello with URL/TOKEN envs.
  - Expect exit 0 and expected output.
- UDS transport (Linux only):
  - Set AIFO_TOOLEEXEC_USE_UNIX=1; repeat the above; expect success.
- Verbose logging assertions:
  - Set AIFO_TEST_LOG_PATH to a temp file and enable verbose.
  - Ensure logs include:
    - “proxy notify parsed cmd=…” line
    - “proxy result tool=say kind=notify code=… dur_ms=…” line

Implementation checklist (files to change)
- src/bin/aifo-shim.rs:
  - Add notification mode (native + curl fallback), verbose lines.
- src/toolchain/shim.rs:
  - Add say to SHIM_TOOLS, remove notifications-cmd, add POSIX notify branch.
- src/toolchain/notifications.rs:
  - Generalize notifications_handle_request to accept cmd + argv; enforce strict match.
- src/toolchain/proxy.rs:
  - Parse cmd + arg for /notify; verbose parsed log; call generalized API; log result (kind=notify).
- src/toolchain/http.rs:
  - Confirm /notify classification; normalize comments to /notify.
- Dockerfile:
  - Remove notifications-cmd; add say in both symlink loops.
- tests/shims_notifications.rs:
  - Update to test the say shim (existence and exit 86 without env).
- Optional tests:
  - Unit tests for classifier and notifications validation.
  - Integration tests for TCP and unix socket transports with verbose logging assertions.

Rollout and migration
- Removing notifications-cmd is a breaking change for any consumer referencing it directly inside containers; advise switching to say (or future additions).
- Ensure images are rebuilt so PATH exposes say and not notifications-cmd.
- Ensure docs and examples use /notify consistently.
