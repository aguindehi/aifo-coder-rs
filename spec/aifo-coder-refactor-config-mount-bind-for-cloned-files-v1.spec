# AIFO Coder – Refactor: Config Mounts via Cloned Read-Only Copies
# Date: 2025-11-20

Summary

This specification defines a secure, consistent, and concurrency-safe way to make host-side coding agent configuration available inside AIFO Coder agent containers without bind-mounting original host files directly. We introduce a standardized host config schema, mount the host config directory read-only into each agent container, and, at container entry, copy a curated subset of files into a private in-container config directory. This mirrors our hardened GnuPG handling, prevents write-back to host, avoids symlink traversal attacks, and ensures fork N and concurrent runs are safe.

Scope

Applies to all agent containers:
- Agents: aider, codex, crush, openhands, opencode, plandex
- Image flavors: full and slim
- Platforms: Linux, macOS (Docker Desktop/Colima), Windows (Docker Desktop)

Toolchain sidecars (rust/node/python/cpp/go) are not consumers of these agent configs and are out of scope unless explicitly required by a future agent-tool need.

Goals

- Do not bind-mount original host config files directly into containers.
- Mount a read-only host config directory once per agent container.
- On entrypoint, copy a sanctioned subset of regular files into a private in-container directory with strict permissions.
- Provide uniform schema, env variables, and policy knobs.
- Ensure concurrency safety for multiple aifo-coder instances and fork N panes.

Threat model and mitigations

- Integrity: prevent in-container processes from writing to host configuration (ro mount + copy).
- Symlink/device traversal: reject non-regular files (symlinks/devices/FIFOs) and sanitize names.
- Size/format controls: allowlist extensions and cap per-file size to avoid accidental large data ingestion.
- Permission hygiene: enforce 0600 for secrets, 0644 for general config.
- Concurrency: per-container copy-on-entry yields isolation across multiple instances and fork panes.
- Back-compat: if no host config dir exists, entrypoint no-ops; agents continue to run.

Standardized host config schema

- Host config root:
  - Default: $HOME/.config/aifo-coder
  - Alternate legacy fallback: $HOME/.aifo-coder (if the default is absent)
  - Overridable by host env AIFO_CONFIG_HOST_DIR
- Layout:
  - $HOST_CFG_ROOT/global/           (shared config for any agent)
  - $HOST_CFG_ROOT/aider/            (Aider-specific config)
  - $HOST_CFG_ROOT/codex/
  - $HOST_CFG_ROOT/crush/
  - $HOST_CFG_ROOT/openhands/
  - $HOST_CFG_ROOT/opencode/
  - $HOST_CFG_ROOT/plandex/
- File naming rules:
  - Depth=1 only (no recursion)
  - Filenames: ASCII [A-Za-z0-9._-]+ (no path separators)
  - Extensions (default allowlist): json, toml, yaml, yml, ini, conf, crt, pem, key, token
  - Per-file size cap (default): 262144 bytes (256 KiB)
- Mapping to in-container paths:
  - Destination base: $HOME/.aifo-config
  - Copy $HOST_CFG_ROOT/global/* → $HOME/.aifo-config/global/
  - Copy $HOST_CFG_ROOT/<agent>/* → $HOME/.aifo-config/<agent>/
  - Special bridging for Aider root-level files (see below)

Special bridging: Aider root-level files

Aider expects certain filenames at $HOME:
- .aider.conf.yml
- .aider.model.settings.yml
- .aider.model.metadata.json

Bridging rules:
- If these files exist in $HOST_CFG_ROOT/aider/, after copying them to $HOME/.aifo-config/aider/, also copy (or symlink best-effort) to $HOME/<filename> so Aider continues to find them without changes.
- If symlinks are undesirable, prefer copying (overwrite) with mode 0644 for these specific files.

In-container copy policy (entrypoint)

Entry-point script /usr/local/bin/aifo-entrypoint (already present and used by all agent images) must:
- Determine directories:
  - HOST_DIR="${AIFO_CONFIG_HOST_DIR:-$HOME/.aifo-config-host}"
  - DST_DIR="${AIFO_CONFIG_DST_DIR:-$HOME/.aifo-config}"
- Determine policy knobs:
  - ENABLE="${AIFO_CONFIG_ENABLE:-1}"
  - MAX_SIZE="${AIFO_CONFIG_MAX_SIZE:-262144}"
  - ALLOW_EXT="${AIFO_CONFIG_ALLOW_EXT:-json,toml,yaml,yml,ini,conf,crt,pem,key,token}"
  - SECRET_HINTS="${AIFO_CONFIG_SECRET_HINTS:-token,secret,key,pem}"
  - COPY_ALWAYS="${AIFO_CONFIG_COPY_ALWAYS:-0}"
- Behavior:
  - If ENABLE != 1: skip copying; export AIFO_CODER_CONFIG_DIR="$DST_DIR"; continue.
  - Create $DST_DIR (mode 0700).
  - If HOST_DIR exists:
    - Copy depth-1 files from HOST_DIR/global and HOST_DIR/<agent> subdirs:
      - Only process regular files (-f); skip non-regular files (symlinks/devices/FIFOs).
      - Name sanitization: must match ^[A-Za-z0-9._-]+$.
      - Extension check: ext in ALLOW_EXT (case-insensitive).
      - Size check: size <= MAX_SIZE.
      - Mode:
        - 0600 if filename contains any SECRET_HINTS substrings (case-insensitive) or extension in {pem,key,token}.
        - 0644 otherwise.
      - Copy via install -m MODE "$src" "$dst".
    - For Aider bridging files: after copy into $HOME/.aifo-config/aider/, also copy into $HOME/<filename> with mode 0644.
  - Export AIFO_CODER_CONFIG_DIR="$DST_DIR".
  - Stamp file: touch "$DST_DIR/.copied" (best-effort).
    - Optional optimization: If COPY_ALWAYS=0 and "$DST_DIR/.copied" exists, you may skip re-copy unless policy or source mtime changed. For simplicity and determinism, Phase 1 can always re-copy; optimization can be added in Phase 3.

Container mounts (agent runtime)

- Always mount the host config directory read-only for agent containers:
  - Resolve host dir on the launcher side:
    - If AIFO_CONFIG_HOST_DIR set and points to a directory: use it.
    - Else prefer $HOME/.config/aifo-coder if it exists; fallback to $HOME/.aifo-coder if it exists; else skip the mount.
  - Bind mount to /home/coder/.aifo-config-host:ro
- Pass-through env knobs (if set on host) to container:
  - AIFO_CONFIG_ENABLE, AIFO_CONFIG_MAX_SIZE, AIFO_CONFIG_ALLOW_EXT, AIFO_CONFIG_SECRET_HINTS, AIFO_CONFIG_COPY_ALWAYS
  - Optionally AIFO_CONFIG_DST_DIR to change the in-container base (default $HOME/.aifo-config)

Concurrency and fork-mode validation

- Multiple concurrent aifo-coder instances:
  - Each container performs its own copy-on-entry into its private $HOME/.aifo-config. No shared state; inherently race-free.
- Fork N:
  - N panes → N agent containers. Each container sees the same read-only host config directory and independently copies into its own $HOME/.aifo-config. No conflicts; each pane is isolated.

Security considerations

- Host write-back blocked (ro mount).
- Symlink traversal blocked (reject non-regular files).
- Filename sanitization narrows injection risk.
- Per-file size cap avoids accidental multi-MB mounts.
- Permissions: 0600 secrets; 0644 config. Directory 0700.
- Logging: Only minimal warnings printed on skips (invalid name/ext/size/symlink) when AIFO_TOOLCHAIN_VERBOSE=1.

Environment variables (complete list)

- AIFO_CONFIG_ENABLE: 1 (default enable), 0 to disable.
- AIFO_CONFIG_HOST_DIR: host directory to mount; default resolved by launcher; in-container default: $HOME/.aifo-config-host.
- AIFO_CONFIG_DST_DIR: in-container destination directory; default $HOME/.aifo-config.
- AIFO_CODER_CONFIG_DIR: exported by entrypoint; points to $HOME/.aifo-config for agent processes and tools.
- AIFO_CONFIG_MAX_SIZE: default 262144 bytes.
- AIFO_CONFIG_ALLOW_EXT: comma list (case-insensitive) default: json,toml,yaml,yml,ini,conf,crt,pem,key,token.
- AIFO_CONFIG_SECRET_HINTS: comma list of substrings; default: token,secret,key,pem.
- AIFO_CONFIG_COPY_ALWAYS: 0 (default), 1 to force re-copy each start.

Gaps and resolutions

- Unknown agent-specific config names:
  - Resolution: Copy all allowed files under $HOST_CFG_ROOT/<agent>/ to $HOME/.aifo-config/<agent>/; agent tools should consult AIFO_CODER_CONFIG_DIR or their conventional XDG config paths. Bridging symlinks/copies can be added per agent in later phases once exact expectations are catalogued.
- Aider root-level file expectation:
  - Resolution: special bridging (copy to $HOME/<filename>) included from Phase 1.
- Optimization for large configurations or frequent restarts:
  - Resolution: Phase 3 can add mtime/stamp-based skip logic when COPY_ALWAYS=0.
- Windows path behaviors:
  - Resolution: Launcher must handle path quoting and platform mounts as done elsewhere; no special limitations for read-only directory mounts.

Phased implementation plan

Phase 1 – Foundation (entrypoint copy + agent mount)
- Update /usr/local/bin/aifo-entrypoint to implement the copy policy described.
- Add the read-only host config directory mount for agent containers:
  - Bind mount host config directory to /home/coder/.aifo-config-host:ro
  - Pass env knobs if set.
- Implement Aider bridging: copy aider root-level files into $HOME after copying into $HOME/.aifo-config/aider/.
- Testing:
  - Unit-level: container exec verifies that copies exist with correct modes.
  - Dry-run preview: ensure docker preview shows the new directory mount (host → /.aifo-config-host:ro).

Phase 2 – Schema consolidation and documentation

Authoritative schema documentation

- Host config root resolution:
  - Default: $HOME/.config/aifo-coder
  - Legacy fallback: $HOME/.aifo-coder (only if default absent)
  - Override: AIFO_CONFIG_HOST_DIR must point to an existing directory
- Directory layout under $HOST_CFG_ROOT:
  - $HOST_CFG_ROOT/global/                shared config for any agent
  - $HOST_CFG_ROOT/aider/                 Aider-specific config
  - $HOST_CFG_ROOT/codex/                 Codex-specific config
  - $HOST_CFG_ROOT/crush/                 Crush-specific config
  - $HOST_CFG_ROOT/openhands/             OpenHands-specific config
  - $HOST_CFG_ROOT/opencode/              OpenCode-specific config
  - $HOST_CFG_ROOT/plandex/               Plandex-specific config
- Filename and format rules:
  - Depth=1 only (no recursion); subdirectories are only per-agent names and “global”
  - Filenames: ASCII [A-Za-z0-9._-]+ (no path separators)
  - Extensions allowlist (case-insensitive): json, toml, yaml, yml, ini, conf, crt, pem, key, token
  - Per-file size cap: 256 KiB (configurable via AIFO_CONFIG_MAX_SIZE)
- In-container mapping:
  - Destination base: $HOME/.aifo-config
  - Copy $HOST_CFG_ROOT/global/* → $HOME/.aifo-config/global/
  - Copy $HOST_CFG_ROOT/<agent>/* → $HOME/.aifo-config/<agent>/
  - Export AIFO_CODER_CONFIG_DIR="$HOME/.aifo-config" for agent processes

Agent-specific notes

- Aider bridging (implemented in Phase 1):
  - If .aider.conf.yml, .aider.model.settings.yml, .aider.model.metadata.json exist in $HOST_CFG_ROOT/aider/, after copying to $HOME/.aifo-config/aider/, also copy into $HOME/<filename> (mode 0644) so Aider finds them at legacy paths.
- Other agents (codex, crush, openhands, opencode, plandex):
  - No root-level bridging required at this time; agents should consult their XDG paths or read from AIFO_CODER_CONFIG_DIR/<agent> where applicable.
  - Future phases may add additional bridging if a tool requires non-XDG locations.

Migration notes (host-side)

- Create the new config root and subdirectories:
  - Linux/macOS:
    - mkdir -p "$HOME/.config/aifo-coder/global" "$HOME/.config/aifo-coder/aider" "$HOME/.config/aifo-coder/codex" "$HOME/.config/aifo-coder/crush" "$HOME/.config/aifo-coder/openhands" "$HOME/.config/aifo-coder/opencode" "$HOME/.config/aifo-coder/plandex"
  - Windows (PowerShell example):
    - New-Item -ItemType Directory -Force -Path "$Env:APPDATA\aifo-coder", "$Env:APPDATA\aifo-coder\global", "$Env:APPDATA\aifo-coder\aider", "$Env:APPDATA\aifo-coder\codex", "$Env:APPDATA\aifo-coder\crush", "$Env:APPDATA\aifo-coder\openhands", "$Env:APPDATA\aifo-coder\opencode", "$Env:APPDATA\aifo-coder\plandex"
- Move legacy Aider files:
  - If you had Aider root files in $HOME:
    - mv "$HOME/.aider.conf.yml"        "$HOME/.config/aifo-coder/aider/.aider.conf.yml"        2>/dev/null || true
    - mv "$HOME/.aider.model.settings.yml" "$HOME/.config/aifo-coder/aider/.aider.model.settings.yml" 2>/dev/null || true
    - mv "$HOME/.aider.model.metadata.json" "$HOME/.config/aifo-coder/aider/.aider.model.metadata.json" 2>/dev/null || true
  - Bridging ensures Aider continues to work; the entrypoint will copy into $HOME as needed.
- Create global or agent-specific config files with allowed names/extensions; keep individual file size ≤ 256 KiB (default policy).

Environment knobs (documented for ops)

- AIFO_CONFIG_ENABLE: 1 (default enable), 0 to disable copying.
- AIFO_CONFIG_HOST_DIR: host directory to mount; launcher resolves default; in-container default is $HOME/.aifo-config-host.
- AIFO_CONFIG_DST_DIR: in-container destination directory; default $HOME/.aifo-config.
- AIFO_CODER_CONFIG_DIR: exported by entrypoint; points to $HOME/.aifo-config.
- AIFO_CONFIG_MAX_SIZE: default 262144 bytes (256 KiB).
- AIFO_CONFIG_ALLOW_EXT: comma list (case-insensitive) default: json,toml,yaml,yml,ini,conf,crt,pem,key,token.
- AIFO_CONFIG_SECRET_HINTS: comma list of substrings; default: token,secret,key,pem.
- AIFO_CONFIG_COPY_ALWAYS: 0 (default), 1 to force re-copy each start.

Troubleshooting and validation

- If a file does not appear in $HOME/.aifo-config:
  - Name may be invalid (must match ^[A-Za-z0-9._-]+$); rename to a safe filename.
  - Extension may be disallowed; update AIFO_CONFIG_ALLOW_EXT to include it or change the extension.
  - File may be a symlink or non-regular file; ensure it is a regular file.
  - File may exceed MAX_SIZE; reduce size or increase AIFO_CONFIG_MAX_SIZE.
- Verify permissions:
  - Secrets (.pem/.key/.token or names containing SECRET_HINTS) should be 0600; other config 0644; directories 0700.
- Concurrency/fork mode:
  - Multiple instances and fork N panes are isolated; each container copies into its private $HOME/.aifo-config.
- Disable/override:
  - Temporarily disable copying via AIFO_CONFIG_ENABLE=0 for debugging.
  - Override source/destination via AIFO_CONFIG_HOST_DIR/AIFO_CONFIG_DST_DIR when necessary.

Documentation updates

- Update project docs to reference:
  - Host schema layout and resolution order.
  - Allowed filename patterns, extensions, and size limits.
  - Environment knobs and their defaults.
  - Aider bridging behavior and legacy migration.
  - Troubleshooting steps above.
- Include examples and quickstart commands for creating directories and populating initial config files.

Phase 3 – Optimization and per-agent bridging enhancements
- Optional: mtime/stamp check to skip re-copy when COPY_ALWAYS=0 and source unchanged.
- Add per-agent bridging where specific tools demand non-XDG locations (catalog actual needs; keep Aider bridging as implemented).
- Add policy logging when verbose; silent otherwise.

Phase 4 – Comprehensive test coverage
- Concurrency tests:
  - Two simultaneous runs with identical host configs; verify isolated copies and no interference.
- Fork N tests:
  - N panes; all containers have independent $HOME/.aifo-config; Aider bridging works in each pane.
- Negative tests:
  - Symlink in host dir → skipped.
  - Oversized file → skipped.
  - Disallowed extension → skipped.
  - Missing host dir → mount skipped; no copies; agent proceeds.

Phase 5 – Rollout and guardrails
- Enable by default (AIFO_CONFIG_ENABLE=1).
- Allow temporary opt-out via AIFO_CONFIG_ENABLE=0 for debugging.
- Monitor logs and adjust allowlist/size cap if necessary.

Acceptance criteria

- No agent container bind-mounts original host config files; only the read-only host config directory is mounted.
- Entry-point copy enforces format, size, and permission rules; Aider bridging present.
- Multiple aifo-coder instances and fork N panes operate without conflicts.
- Documentation defines final schema and troubleshooting steps.
- Tests pass via `make check` (cargo nextest).

Notes

- This spec intentionally parallels our robust GNUPG runtime preparation block in aifo-entrypoint and uses the same centralization strategy for security and consistency.
- Sidecars remain unchanged in this iteration to minimize scope and risk.
