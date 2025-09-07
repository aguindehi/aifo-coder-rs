AIFO Coder: Fork Attach Feature (v3) – Compact, Production‑Ready Specification

Status
- Stage: v3 specification (refined, production-ready design)
- Scope: CLI, behavior, metadata, orchestration, override semantics, error handling, compatibility, testing
- Compatibility: Backward compatible with v1/v2 sessions; clarifies stable pane index semantics; prefers child_argv; enriches dry-run JSON; clarifies Windows Terminal behavior

Motivation
After creating a fork session with aifo-coder --fork N and --fork-merge-strategy none, users often close all panes and later want to resume work on the existing clones/branches without re-cloning or changing branches. The fork attach feature re-discovers the session and launches a new multi-pane agent session bound to the existing clone directories.

Key Principles (v3)
- No mutation: attach never clones, fetches, checks out, merges, or deletes.
- Stable index: the fork index is the numeric suffix of pane-n; never renumber. Use this index for env, state dirs, and container names.
- Lossless override semantics: preserve recorded root flags; allow independent overrides of agent token and agent args.
- Tolerant by default: proceed with any valid panes; strict mode requires all panes valid and non-detached.
- Cross-platform orchestration: tmux on Linux/macOS; Windows Terminal (preferred), PowerShell, or Git Bash/mintty on Windows.
- Metadata-light but precise: prefer child_argv (array), keep child_args (flat string) for back-compat; update last_attached on successful attach.

CLI Specification
- Command
  aifo-coder fork attach <session>
  [--layout <tiled|even-h|even-v>]
  [--agent <aider|codex|crush>]
  [--session-name <name>]    # tmux only; ignored on Windows (cosmetic)
  [--strict]                 # require all panes valid and non-detached
  [--dry-run]
  [--json]                   # only valid with --dry-run
  [--verbose]
  [--] [AGENT-ARGS...]

- Semantics
  - <session> resolves under <repo_root>/.aifo-coder/forks. Supported selectors:
    - Exact directory name match.
    - latest or last: choose the session with greatest last_attached; fallback to greatest created_at; fallback to latest directory mtime.
    - Unique prefix: select if exactly one directory starts with the given prefix; otherwise list candidates and error.
  - --layout overrides layout; default to metadata “layout”; fallback tiled. tmux mapping:
    - tiled -> “tiled”; even-h -> “even-horizontal”; even-v -> “even-vertical”
  - --agent overrides recorded agent token (case-insensitive).
  - --session-name sets the tmux session name; default “aifo-<sid>-attach”. On tmux, error on name conflict; on Windows, this is cosmetic only (no conflict detection).
  - --strict: all discovered panes must be valid Git worktrees and not detached; otherwise exit 1.
  - --dry-run: print plan and exit 0. --json requires --dry-run.
  - [--] [AGENT-ARGS...]: replace recorded agent args (tokens after the agent). Root flags remain recorded.

Supported Platforms & Prerequisites
- Linux/macOS: require tmux (exit 127 if missing).
- Windows: prefer Windows Terminal (wt.exe); fallback PowerShell; second fallback Git Bash or mintty (exit 127 if none found).
- Docker: not required for attach itself; agents may require Docker at launch time per normal logic.

Behavior Overview
- Resolve <repo_root> (must be inside a Git repository).
- Resolve <session> by selector under <repo_root>/.aifo-coder/forks.
- Discover pane directories pane-<n> (n in [1..∞]); indices are the numeric suffix n (stable). Validate panes via git -C <dir> rev-parse --is-inside-work-tree = true. Determine branch via rev-parse --abbrev-ref HEAD; “HEAD” indicates detached.
- Tolerant mode (default): skip invalid or detached panes with warnings; proceed if at least one valid pane remains. Strict mode: require all to be valid and non-detached.
- Read .meta.json (best-effort) and compute effective configuration:
  - Prefer child_argv (array), else parse child_args (string). Split into recorded_root_flags, recorded_agent_token, recorded_agent_args.
  - Effective agent token = CLI --agent if provided else recorded_agent_token else “aider”.
  - Effective agent args = CLI AGENT-ARGS... (tokens after --) if provided else recorded_agent_args.
  - Root flags = recorded_root_flags (always preserved).
- Preflight: best-effort stop stale agent containers with the planned names and stop stale toolchain sidecars + session network for sid (safe best-effort; reuse is allowed).
- Launch new UI session (tmux or Windows launcher) with one pane per valid clone:
  - Export AIFO_CODER_* envs, cd to pane dir, exec “aifo-coder [recorded_root_flags…] <effective_agent_token> [effective_agent_args…]”.
  - Shell lifetime:
    - tmux: after agent exit, show a prompt “press ‘s’ to open a shell”; otherwise auto-close the pane to keep the session tidy (same UX as fork run).
    - Windows: PowerShell uses -NoExit; Git Bash/mintty append “&& exec bash” so panes remain open.
- Update .meta.json with last_attached and effective agent/args/layout (best-effort).

Override Semantics (Lossless; child_argv preferred)
- Metadata recorded at fork creation (or earlier v1/v2 sessions):
  - agent: "aider"|"codex"|"crush" (lowercase preferred)
  - child_argv: [string]  # preferred canonical argv
  - child_args: string     # flat, shell-joined; fallback if child_argv absent
  - layout, panes, pane_dirs, branches, created_at, base_* (unchanged)
  - last_attached: number (best-effort, updated on attach)
- Parsing recorded argv:
  - If child_argv present: tokens = child_argv.
  - Else: tokenize child_args using the same shell_like_split_args used by the launcher.
  - Identify recorded_agent_token as the first token case-insensitively matching aider|codex|crush.
  - recorded_root_flags = tokens before the agent token; recorded_agent_args = tokens after it (including “--” and trailing args verbatim).
- Effective argv:
  - agent_token = CLI --agent if provided; else recorded_agent_token; else “aider”.
  - agent_args = CLI AGENT-ARGS... if provided; else recorded_agent_args.
  - root_flags = recorded_root_flags (unchanged).
- Final command executed per pane:
  aifo-coder [root_flags…] <agent_token> [agent_args…]
- Quoting:
  - tmux/Git Bash: POSIX shell quoting (shell_escape) per token; join.
  - PowerShell: single-quote per token; double any single quotes inside.
  - If child_argv present, quote each token independently according to target shell.

Phased Plan

Phase 0 – Orchestrator Preconditions
- Linux/macOS: require tmux; exit 127 if missing.
- Windows: detect Windows Terminal (wt.exe), PowerShell (pwsh|powershell.exe), Git Bash/mintty (git-bash.exe|bash.exe|mintty.exe). If none found, exit 127.
- Attach is designed for a single active attach per sid. If sidecars/network already exist, reuse is allowed (log at verbose).

Phase 1 – Resolve Repository Root
- Use repo_root() to find the Git top-level (canonical).
- If not inside a Git repository, exit 1 with a clear message.

Phase 2 – Resolve Session Directory
- Base directory: <repo_root>/.aifo-coder/forks.
- Select session:
  - Exact directory name match.
  - latest/last: greatest last_attached; else greatest created_at; else latest dir mtime.
  - Unique prefix: if exactly one match; else error listing candidates.
- If not found, exit 1: “No such session ‘<sid>’.”

Phase 3 – Discover Panes and Branches
- Enumerate subdirectories matching ^pane-([1-9][0-9]*)$.
- Stable index = captured numeric suffix n; sort ascending by n for launch order; do not renumber.
- For each pane dir:
  - Validate with git -C <pane_dir> rev-parse --is-inside-work-tree; valid if true.
  - Determine branch: git -C <pane_dir> rev-parse --abbrev-ref HEAD. “HEAD” means detached.
- Tolerant mode (default): skip invalid/missing; mark detached HEAD as valid=false with reason “detached”; proceed if at least one valid pane remains; else exit 1.
- Strict mode: require all panes valid and non-detached; else exit 1.

Phase 4 – Read Metadata and Compute Effective Configuration
- Read <session_dir>/.meta.json (best-effort).
- Fields consulted:
  - created_at, base_label, base_ref_or_sha, base_commit_sha, layout
  - panes, pane_dirs, branches
  - agent, child_argv, child_args, last_attached
- Compute effective layout (requested/effective mapping).
- Parse recorded argv (child_argv preferred; else child_args) into (root_flags, agent_token, agent_args); apply CLI overrides as defined above.

Phase 5 – Preflight and Cleanup (Best-Effort)
- toolchain_cleanup_session(sid, verbose?) to stop stale sidecars and remove session network (no-op if absent).
- For each planned pane, check agent container name “aifo-coder-<agent>-<sid>-<index>”:
  - If a container with that name exists and is stopped, remove it.
  - If running, attempt to stop it with a short timeout; proceed regardless; warn on failure (verbose).
- Attach does not mutate Git state.

Phase 6 – Build Attach Plan
- Compute:
  - sid, session_name (tmux: default “aifo-<sid>-attach”), layout_requested, layout_effective
  - recorded_agent (if any), effective agent token
  - recorded_root_flags, recorded_agent_args, effective_agent_args
  - panes: [{ index, path, branch or “(detached HEAD @ <sha7>)”, valid, reason? }]
- If --dry-run:
  - Text plan (color-aware): include all panes, flag invalid and reasons.
  - If --json: print JSON:
    {
      sid, session_name, layout_requested, layout_effective,
      recorded_agent, agent, recorded_root_flags, recorded_agent_args,
      effective_root_flags, effective_agent_token, effective_agent_args,
      panes: [{ index, path, branch, valid, reason? }],
      errors: []
    }
  - Exit 0.
- If --json without --dry-run: exit 1 with a clear message.

Phase 7 – Launch Orchestrator and Panes
- Environment variables per pane:
  - AIFO_CODER_SKIP_LOCK=1
  - AIFO_CODER_CONTAINER_NAME=aifo-coder-<agent>-<sid>-<index>
  - AIFO_CODER_HOSTNAME=aifo-coder-<agent>-<sid>-<index>
  - AIFO_CODER_FORK_SESSION=<sid>
  - AIFO_CODER_FORK_INDEX=<index>         # index = numeric suffix n
  - AIFO_CODER_FORK_STATE_DIR=<state_base>/<sid>/pane-<index> (ensure it exists; create .aider/.codex/.crush)
  - AIFO_CODER_SUPPRESS_TOOLCHAIN_WARNING=1
  - Optional reserved: AIFO_SESSION_NETWORK (not required by attach)
- Child command per pane:
  - cd <pane_dir>; exec aifo-coder [recorded_root_flags…] <effective_agent_token> [effective_agent_args…]
- Linux/macOS (tmux):
  - Create detached tmux session named session_name. If a tmux session with that name already exists, abort with exit 1 suggesting --session-name.
  - Split window into as many panes as valid clones; apply layout mapping (tiled/even-horizontal/even-vertical).
  - In each pane, send-keys a launch script that exports env, runs child command, then shows “press ‘s’ to open a shell; otherwise auto-close the pane.”
  - If already in tmux (TMUX set), switch-client to the session; else attach-session.
- Windows:
  - Windows Terminal (wt.exe) preferred: new-tab for first pane, then split-pane for rest; orientation:
    - even-h: use -H consistently; even-v: use -V; tiled: alternate -H/-V starting with -H.
  - PowerShell fallback: Start-Process windows; keep -NoExit when no merge; otherwise wait on PIDs to detect completion (matching fork run pattern).
  - Git Bash/mintty fallback: bash -lc with exports and agent command; append “&& exec bash” to keep shell open.
  - --session-name is cosmetic (used in logs/tab titles only); no conflict detection.

Phase 8 – Metadata Update (Best-Effort)
- Upsert minimally into <session_dir>/.meta.json:
  - last_attached: <unix_secs>
  - agent: "<aider|codex|crush>"
  - child_argv: [<tokens...>]          # preferred
  - child_args: "<flat-string>"         # shell-joined child_argv for back-compat
  - layout: "<tiled|even-h|even-v>"
  - panes: <N_valid>                    # current valid count
  - Optionally refresh pane_dirs and branches to the discovered set
- Failures to write must not abort attach; log in --verbose mode only.

Error Handling & Exit Codes
- Not in a Git repo: exit 1.
- Session not found: exit 1.
- Orchestrator not found (tmux on *nix; WT/PowerShell/Git Bash on Windows): exit 127.
- Tolerant mode: if no valid panes found: exit 1; otherwise proceed and warn per skipped/invalid.
- Strict mode: any invalid or detached pane aborts: exit 1.
- tmux session name conflict: exit 1 with guidance to use --session-name.
- --json without --dry-run: exit 1.
- --layout invalid: default to tiled; warn in --verbose; continue with exit 0 on success.

Security & Isolation
- Attach does not elevate privileges or change security posture.
- AppArmor/seccomp/cgroupns as applied by the agent launcher remain unchanged.
- No additional mounts beyond agent requirements. No Git writes by attach.

Logging
- --verbose prints:
  - Session resolution details and chosen sid
  - Discovered panes: path, branch or “detached HEAD @ sha7”, valid/invalid reason
  - Recorded vs effective config (root flags, agent token/args); final composed child command per pane (properly quoted)
  - Orchestrator command previews (tmux/WT/PowerShell/Git Bash)
  - Metadata read/write status (best-effort upserts) and paths
  - Preflight cleanup actions (sidecars/network/container reuse/stops)

Metadata Model (Session .meta.json)
- Existing fields: created_at, base_label, base_ref_or_sha, base_commit_sha, panes, pane_dirs, branches, layout, snapshot_sha (optional)
- v1+/v2 fields: agent, child_args, child_argv (if present, preferred), last_attached
- v3 behavior:
  - Prefer child_argv; if absent, parse child_args; if both present, child_argv is authoritative and child_args should be rewritten on update using shell_join(child_argv).
  - Normalize agent to lowercase on update.

Backwards Compatibility
- Sessions without child_argv/child_args/agent/layout attach with defaults: layout=tiled; agent=aider; args=[].
- latest/last selector works even when last_attached is missing by falling back to created_at and then directory mtime.
- Pane index remains the numeric suffix across all versions.

Acceptance Criteria
- Users can re-open a session created with --fork N and --fork-merge-strategy none using aifo-coder fork attach <sid> without cloning or Git mutations.
- Tolerant-by-default attach proceeds with any valid panes; strict mode enforces all valid/non-detached.
- Root flags are preserved; agent token and agent args are independently overridable.
- Behavior is consistent across Linux/macOS (tmux) and Windows (WT/PowerShell/Git Bash).
- Dry-run text and JSON plans are clear, accurate, and exit 0.

Testing Plan (E2E/Integration)
1) Happy path (tmux): create fork (N=3, strategy=none); close panes; attach; verify 3 panes; cwd per pane; agents launch; tmux shell prompt behavior.
2) Windows Terminal fallback chain: WT present -> panes open; if only PowerShell -> windows open; if only Git Bash/mintty -> windows open and shells persist.
3) Stable index: delete pane-1; attach; verify pane-2 launches with AIFO_CODER_FORK_INDEX=2, state dir .../pane-2, container name suffix -2.
4) Tolerant vs strict: make one pane non-repo or detached; tolerant proceeds with remaining; strict exits 1.
5) Metadata back-compat: remove child_argv; keep child_args; attach reconstructs argv; then add child_argv; attach prefers it; on update, both child_argv and child_args written.
6) Session resolution: create multiple sessions; set last_attached on one; verify latest/last picks it; unique-prefix selects uniquely; ambiguous prefix lists candidates and fails.
7) JSON dry-run: verify fields include recorded/effective splits and pane validity/warnings; exit 0.
8) Container collision: leave agent container running; attach stops it best-effort before launching; verify no naming conflict remains.
9) Worktree validation: make pane a git worktree without .git dir; attach accepts via rev-parse --is-inside-work-tree.

Examples
- Re-attach with recorded config:
  aifo-coder fork attach aifo-123abc
- Override layout and pass new agent args (root flags preserved):
  aifo-coder fork attach aifo-123abc --layout even-h -- --model anthropic/claude-3.5-sonnet
- Override agent (switch from aider to codex) and args:
  aifo-coder fork attach aifo-123abc --agent codex -- --help
- Attach latest with a custom tmux session name:
  aifo-coder fork attach latest --session-name dev-work
- Strict mode (require all panes valid and non-detached):
  aifo-coder fork attach aifo-123abc --strict
- Preview only (text and JSON):
  aifo-coder fork attach aifo-123abc --dry-run
  aifo-coder fork attach aifo-123abc --dry-run --json

Versioning
- This document defines v3 of the fork attach feature.
- v1/v2 sessions remain fully supported.
- v3 clarifies stable indexing, Windows Terminal session-name behavior, child_argv preference, and enriched JSON plan output.
- Future revisions should append -v4, etc., and document backward-compatibility.
