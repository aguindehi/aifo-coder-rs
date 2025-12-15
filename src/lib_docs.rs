AIFO Coder crate: architecture overview, environment invariants and module map.

Architecture
- Binary glue (src/main.rs) orchestrates CLI, banner/doctor, fork lifecycle and toolchain session.
- Library exports are stable and used across modules and tests; most helpers live under fork::* and toolchain::*.

Key modules
- fork::*: repo detection, snapshot/clone/merge/cleanup, orchestrators, guidance and summaries.
- toolchain::*: sidecar lifecycle, proxy/shim, routing/allowlists, notifications and HTTP helpers.
- util::*: small helpers (shell/json escaping, URL decoding, Docker security parsing, fs utilities).
- color.rs: color mode and paint/log wrappers (exact strings preserved).
- apparmor.rs: host AppArmor detection and profile selection helpers.

Environment invariants (documented for contributors)
- AIFO_TOOLEEXEC_URL/TOKEN: exported by proxy start; injected into agent env; respected by shims.
- AIFO_SESSION_NETWORK: session network name (aifo-net-<id>) to join; removed on cleanup.
- AIFO_TOOLEEXEC_ADD_HOST (Linux): when "1", add host-gateway entry; used for troubleshooting.
- AIFO_CODER_CONTAINER_NAME/HOSTNAME: stable container name/hostname per pane/session.
- AIFO_CODER_FORK_*: pane/session metadata exported to orchestrated shells/sessions.
- AIFO_CODER_COLOR / NO_COLOR: crate-wide color control; wrappers always preserve message text.

Style guidance
- Prefer lines <= 100 chars where feasible in non-golden code; never change user-visible strings.
- Module-level docs should summarize purpose and invariants to aid contributors.
