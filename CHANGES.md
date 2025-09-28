2025-09-28 User <user@example.com>

Refactor: unify stderr logging via color-aware helpers (log_*)

- Adopt aifo_coder::log_info_stderr/log_warn_stderr/log_error_stderr for stderr
  one-liners across the codebase.
- Converted previews/info/warn/error lines in sidecar, mounts, CLI, registry,
  fork preflight/runner, and optional notices.
- Kept structured multi-line outputs and proxy/shim streaming unchanged per spec.
- Documented logging policy in src/color.rs; centralized use_err per function
  scope; preserved exact message strings.
