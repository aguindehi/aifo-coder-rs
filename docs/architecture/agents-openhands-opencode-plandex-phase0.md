# Phase 0 — Architecture and image requirements (planning)

This document defines the planned architecture and image requirements for integrating
three CLI coding agents: OpenHands, OpenCode, and Plandex. It aligns with the v3
specification and existing repository conventions for image naming, PATH policy,
entrypoint contracts, and security posture.

Overview
- Agents: openhands, opencode, plandex
- Executable paths in containers:
  - /usr/local/bin/openhands
  - /usr/local/bin/opencode
  - /usr/local/bin/plandex
- Images: full and slim flavors for each agent (consistent naming and tags).

PATH policy
- Shims-first PATH for these agents:
  PATH="/opt/aifo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:$PATH"
- In container images, set ENV PATH="/opt/aifo/bin:${PATH}" to guarantee shims-first at process start.
- No node-first special casing needed for openhands/opencode/plandex.
- Compatible with existing sidecars, proxy and shims; preserve current behavior.

OS base
- Debian Bookworm slim for runtime images (predictable CA/curl/openssl behavior).

Non-root execution
- Containers run via `docker --user UID:GID` mapping to host user.
- Prepare non-root user:
  - Username: coder
  - HOME: /home/coder
- Ensure HOME and runtime directories are writable; include libnss-wrapper to map
  UID/GID correctly when missing from /etc/passwd.

Entrypoint contract
- Set HOME and GNUPGHOME (GNUPGHOME="$HOME/.gnupg").
- Prepare XDG_RUNTIME_DIR.
- Configure pinentry-curses.
- Launch gpg-agent at startup (default-cache-ttl=7200, max-cache-ttl=86400).
- Mount host $HOME/.gnupg read-only to /home/coder/.gnupg-host and copy first-run essentials.
- Export SHELL=/opt/aifo/bin/sh to prefer shims.
- Preserve invariants (e.g., dumb-init if present) and avoid root-owned writes in
  /workspace.

Security posture
- No privileged mode.
- Do not mount host Docker socket.
- AppArmor compatible.
- Minimal mounts; avoid unnecessary host exposure.
- If an AppArmor profile is specified, apply it best-effort and continue without if unsupported.

Dependencies
- Shared minimum for both flavors:
  - curl, ca-certificates, bash/dash/sh, coreutils, gpg, pinentry-curses, git,
    libnss-wrapper
- Full adds editors and tools:
  - emacs-nox, vim, nano, mg, nvi, ripgrep
- Slim retains minimal editors:
  - mg, nvi
- Cleanup mirrors existing images: remove apt caches and docs/locales by default.
- Default KEEP_APT=0; drop apt/procps in final layers; retain curl only in full images.

Image naming and flavors
- Full: aifo-coder-<agent>:<tag>
- Slim: aifo-coder-<agent>-slim:<tag>
- Registry prefix selection and normalization follow preferred_registry_prefix[_quiet]
  and environment overrides (AIFO_CODER_IMAGE*, AIFO_CODER_REGISTRY_PREFIX).
- Registry prefix is normalized to "<host>/".

Build stage names (reserved)
- Full stages: openhands, opencode, plandex
- Slim stages: openhands-slim, opencode-slim, plandex-slim
- Each stage will export ENV PATH="/opt/aifo/bin:${PATH}" during build to enforce shims-first.

Consistency with existing code
- agent_images.rs composes "<prefix>-<agent>{-slim}:{tag}" with registry prefix; no
  changes required.
- docker.rs uses shims-first PATH for these agents by default; node-first remains for
  codex/crush; aider keeps venv path handling.

Validation in later phases
- Docker previews show PATH with /opt/aifo/bin first for openhands/opencode/plandex.
- Published images will run “--help” successfully; entrypoint prepares GNUPGHOME/XDG
  runtime.
- Tests remain preview-only; no network pulls.

Notes
- Phase 0 captures contracts and requirements to implement across Phases 1–4.
- Avoid agent-specific UX or flags beyond established patterns.
