# AIFO Coder Cleanup Codebase – Iteration 1 (v1)
#
# Scope: Tests consolidation + foundational helpers + targeted correctness hardening.
# Included (this revision): core security hardening for proxy/http/notifications/warn,
# since those files are now available.
#
# Non-goals (this iteration):
# - No large behavior changes outside input validation and security hardening.
# - No new dependencies in build.rs.
# - Keep user-facing output stable unless required for security (document any differences).

## Guiding Principles
- Prefer small, mechanically verifiable refactors.
- Consolidate duplicated logic into `tests/support/mod.rs` and remove per-test clones.
- Enforce explicit input validation at trust boundaries.
- Avoid PATH-based command execution where feasible (prefer absolute paths or OS APIs).
- Lines <= 100 where possible (tests may opt out with ignore-tidy-linelength).

## Files In Scope (must be the only edited files)
- build.rs
- src/toolchain/http.rs
- src/toolchain/proxy.rs
- src/toolchain/notifications.rs
- src/ui/warn.rs
- tests/support/mod.rs
- tests/e2e_proxy_smoke.rs
- tests/e2e_config_copy_policy.rs
- spec/aifo-coder-cleanup-codebase-interation-1-v1.spec

## Risk Review / Issues to Fix (validated against current sources)

### 1) Proxy: user-controlled cwd is not validated (security/correctness)
`src/toolchain/proxy.rs` accepts `cwd` from query/form and uses it to:
- build `PathBuf::from(cwd)` and probe host FS (`pwd.join("node_modules/.bin/tsc")`)
- pass to `build_sidecar_exec_preview_with_exec_id` (workdir handling)
This allows:
- host filesystem probing outside repo (`cwd=/` etc.)
- inconsistent behavior (tool routing changes based on attacker-chosen cwd)

Fix:
- Parse/validate cwd:
  - Accept "." and "/workspace" and "/workspace/<subpath...>" only.
  - Normalize (collapse repeated slashes, remove trailing slash, reject ".." segments).
  - Convert "." to "/workspace".
  - If invalid: 400 Bad Request.

### 2) Proxy: tool + argv need caps beyond body cap (DoS control)
Even with a 1 MiB body cap, an attacker can supply:
- too many `arg` fields
- extremely long single arguments
- large `tool` or `cmd` values

Fix:
- Enforce limits in proxy handler before spawning:
  - max args count (e.g. 128)
  - max tool length (e.g. 64)
  - max arg length (e.g. 4096)
  - max cwd length (e.g. 4096)
  - max cmd length (e.g. 128 for notifications)
- If exceeded: 400 Bad Request.

### 3) Proxy: notifications noauth path bypasses token but not command policy (ok),
but should not allow empty cmd or invalid proto (already enforced). Need to cap argv too.
Fix:
- Apply the same argv caps to notifications as to exec.
- Ensure notif_cmd basename compare is constant-time not required; but ensure normalized.

### 4) HTTP parser: duplicate Transfer-Encoding headers are not handled correctly
In `src/toolchain/http.rs`, `parse_headers` stores only the last value per header name.
This means multiple TE headers collapse, which is ok, but the tests in the file expect:
- last TE wins
Currently it lowercases TE then checks contains("chunked"), so last header wins.
However: for `Transfer-Encoding: chunked, identity` (single header) current code
contains "chunked" and will treat as chunked even if "chunked" is not last.
This is acceptable given RFC semantics (presence of chunked implies chunked coding),
but if a later coding is not supported it should be rejected or treated carefully.

Fix:
- Parse transfer-encoding tokens split by ',' and detect "chunked" in any position.
- If there are other codings besides "chunked" and "identity", reject with 400
  (unsupported encoding) to avoid request smuggling ambiguity.

### 5) HTTP parser: read_line_from uses windows(2).position for CRLF and falls back to LF,
but for long buffers it repeatedly scans from start (O(n^2)).
Fix:
- Keep it simple but reduce scans by using `position` on bytes once per loop is ok given caps,
but ensure BODY_CAP and HDR_CAP are enforced early. Already enforced; acceptable.

### 6) Notifications: allowlist is basename-based only (security)
`src/toolchain/notifications.rs` allows any absolute path whose basename is on allowlist.
This allows executing `/tmp/say` if configured.

Fix:
- Strengthen policy:
  - Require exec_abs to be under a safe directory allowlist by default:
    `/usr/bin`, `/bin`, `/usr/local/bin`, `/opt/homebrew/bin` (mac)
  - Allow overriding safe dirs via env `AIFO_NOTIFICATIONS_SAFE_DIRS` (comma-separated),
    but only if `AIFO_NOTIFICATIONS_UNSAFE_ALLOWLIST=1` is set; otherwise ignore overrides.
  - Keep basename allowlist as an additional constraint.

### 7) Notifications: environment trimming allowlist is too permissive
`AIFO_NOTIFICATIONS_ENV_ALLOW` allows any variable name; could leak secrets into child process.
Fix:
- Cap list length (e.g. max 16 vars) and name length (e.g. 64).
- Allow only variables matching `^[A-Z0-9_]+$`.
- If invalid entries are present, ignore them (do not error).

### 8) warn_input_unix executes "stty" from PATH (PATH injection)
Fix:
- Resolve stty path once:
  - Try "/bin/stty", "/usr/bin/stty"
  - If neither exists, fall back to line-based input (no stty).
- Keep behavior/messages identical.

### 9) build.rs: rustc-env injection via newlines/control chars
Fix:
- Introduce sanitize helper to reject values containing \n, \r, \0 and trim.
- Apply to endpoint, transport, build_date, rustc_ver, target, profile.
- For rustc/date output: take first line only.

## Phased Plan

### Phase A — Tests consolidation
A1) Extend `tests/support/mod.rs`:
- docker_runtime()
- unique_name(prefix) includes pid + nanos
- stop_container()
- docker_exec_sh()
- wait_for_config_copied()
- http_post_form_tcp(port, path, headers, body_kv) using aifo_coder::find_header_end
- Update existing http_post_tcp to call new helper (or remove if unused)

A2) Update tests:
- `tests/e2e_proxy_smoke.rs` uses support::http_post_form_tcp
- `tests/e2e_config_copy_policy.rs` uses support helpers, sets creds.token perms 0600 on unix

### Phase B — Production hardening
B1) `src/toolchain/proxy.rs`:
- Validate and normalize cwd (see above)
- Apply caps for tool/cwd/argv/cmd
- Ensure verbose logging never prints Authorization header (already not logged; keep invariant)
- For logs printing argv, redact common secrets:
  - replace values following `--token`, `--password`, `--api-key`, `--key` with "***"
  - redact substrings matching `(?i)bearer\s+[A-Za-z0-9._-]+` when present in argv strings

B2) `src/toolchain/http.rs`:
- Transfer-Encoding parsing:
  - split by ',' and trim tokens
  - allow only identity and chunked
  - if unknown token present, return InvalidData
- Keep existing caps.

B3) `src/toolchain/notifications.rs`:
- Add safe-dir policy checks (default dirs)
- Harden env allowlist parsing (caps + regex)
- Keep existing timeout behavior.

B4) `src/ui/warn.rs`:
- Use absolute stty path or fallback input.
- Preserve existing user prompt messages.

B5) `build.rs`:
- Sanitize env emission as described.

## Acceptance Criteria
- `make check` passes.
- No new clippy warnings in touched files.
- Tests compile; ignored tests remain ignored.
- Proxy rejects invalid cwd and too-large argv with 400.
- Notifications rejects executables outside safe dirs unless explicitly unsafe override is enabled.
- warn prompt does not execute PATH-based stty when absolute stty is available.

## Status (Implementation Notes)
- Phase A (tests consolidation): completed. Tests now prefer `tests/support/mod.rs` helpers for URL/port
  parsing, docker helpers, and raw HTTP helpers. The large-payload test intentionally uses raw HTTP
  to craft an oversized `arg=` flood to validate rejection behavior.
- Phase B (production hardening): completed as specified.

## Run Commands
- Run formatting: ./x fmt (if available in this repo) otherwise cargo fmt
- Run tests: make check
