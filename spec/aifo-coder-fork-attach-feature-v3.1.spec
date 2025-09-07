AIFO Coder: Fork Attach Feature (v3.1) – Compact, Production‑Ready Specification

Status
- Stage: v3.1 specification (refined clarifications; production-ready)
- Scope: CLI, behavior, metadata, override semantics, orchestration, preflight reuse/cleanup policy, error handling, compatibility, testing
- Compatibility: Backward compatible with v1/v2/v3 sessions; prefers child_argv; clarifies stable pane index semantics and Windows Terminal behavior; enriches dry-run JSON; clarifies sidecar/network reuse versus cleanup and container collision policy

Motivation
After creating a fork session with aifo-coder --fork N and --fork-merge-strategy none, users often close all panes and later want to resume work on the existing clones/branches without re‑cloning or changing branches. The fork attach feature re‑discovers the session and launches a new multi‑pane agent session bound to the existing clone directories.

Key Principles
- No mutation: attach never clones, fetches, checks out, merges, or deletes Git state.
- Stable index: the fork index is the numeric suffix of pane-n; never renumber. Use this index consistently for env, state dirs, and container names.
- Lossless override semantics: preserve recorded root flags; allow independent overrides of agent token and agent args.
- Tolerant by default: proceed with any valid panes; strict mode requires all panes valid and non‑detached.
- Cross‑platform orchestration: tmux on Linux/macOS; Windows Terminal (preferred), PowerShell, or Git Bash/mintty on Windows.
- Metadata‑light but precise: prefer child_argv (array); keep child_args (flat string) for back‑compat; update last_attached on successful attach.
- Reuse over teardown: toolchain sidecars and the session network are reused when present; best‑effort cleanup only for stale items. Designed for one active attach per sid, but reuse is allowed.

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

- <session> selectors (scoped to <repo_root>/.aifo-coder/forks):
  - Exact directory name match.
  - latest or last: choose the session with greatest last_attached; fallback to greatest created_at; fallback to latest directory mtime.
  - Unique prefix: select if exactly one directory starts with the given prefix; otherwise list candidates and error.

- Layout mapping:
  - tiled -> tmux “tiled”
  - even-h -> tmux “even-horizontal”
  - even-v -> tmux “even-vertical”

- Override one‑liner (authoritative):
  Final argv = recorded_root_flags + (CLI --agent or recorded_agent or aider) + (CLI AGENT-ARGS… or recorded_agent_args)

Supported Platforms & Prerequisites
- Linux/macOS: require tmux (exit 127 if missing).
- Windows: prefer Windows Terminal (wt.exe); fallback PowerShell; second fallback Git Bash or mintty (exit 127 if none found).
- Docker: not required for attach orchestration itself; agents may require Docker at launch time as usual.

Behavior Overview
- Resolve <repo_root> (must be inside a Git repository).
- Resolve <session> under <repo_root>/.aifo-coder/forks via selector rules.
- Discover pane directories named pane-<n> (n in [1..∞]). Index = the numeric suffix n (stable). Validate panes via git -C <dir> rev-parse --is-inside-work-tree = true. Determine branch via rev-parse --abbrev-ref HEAD; “HEAD” indicates detached. For detached, compute sha7 for display.
- Tolerant mode (default): skip invalid or detached panes with warnings; proceed if at least one valid pane remains. Strict mode: require all to be valid and non‑detached; else exit 1.
- Read .meta.json (best‑effort) and compute effective configuration:
  - Prefer child_argv (array), else parse child_args (string) with the same tokenizer used by the launcher (shell_like_split_args).
  - Split tokens into recorded_root_flags, recorded_agent_token, recorded_agent_args. If no agent token found, treat all tokens as recorded_root_flags and use recorded agent (or aider) with empty recorded_agent_args.
  - Effective agent token = CLI --agent if provided else recorded_agent_token else “aider”.
  - Effective agent args = CLI AGENT-ARGS... (tokens after --) if provided else recorded_agent_args.
  - Root flags = recorded_root_flags (always preserved).
- Preflight: best‑effort stop stale agent containers with the planned names; toolchain sidecars + session network are reused when present (log reuse). If clearly stale, cleanup is allowed; do not fail on cleanup errors.
- Launch a new UI session (tmux or Windows launcher) with one pane per valid clone:
  - Export AIFO_CODER_* envs, cd to pane dir, exec “aifo-coder [recorded_root_flags…] <effective_agent_token> [effective_agent_args…]”.
  - Shell lifetime:
    - tmux: after agent exit, show a prompt “press ‘s’ to open a shell”; otherwise auto‑close the pane to keep the session tidy (same UX as fork run).
    - Windows: PowerShell uses -NoExit; Git Bash/mintty append “&& exec bash” so panes remain open.
- Update .meta.json with last_attached and effective agent/args/layout (best‑effort).

Override Semantics (Lossless; child_argv preferred)
- Metadata recorded at fork creation (existing v1+/v2 sessions may have partial):
  - agent: "aider"|"codex"|"crush" (normalize lowercase on update)
  - child_argv: [string]  # preferred canonical argv
  - child_args: string     # flat, shell-joined; fallback if child_argv absent
  - layout, panes, pane_dirs, branches, created_at, base_* (unchanged)
  - last_attached: number (best‑effort, updated on attach)
- Parsing recorded argv:
  - If child_argv present: tokens = child_argv.
  - Else: tokenize child_args using the launcher’s shell_like_split_args.
  - Identify recorded_agent_token as the first token case‑insensitively matching aider|codex|crush.
  - recorded_root_flags = tokens before the agent token; recorded_agent_args = tokens after it (including “--” and trailing args verbatim).
  - If no agent token is found: recorded_root_flags = all tokens; recorded_agent_token = agent field or “aider”; recorded_agent_args = [].
- Effective argv:
  - agent_token = CLI --agent if provided; else recorded_agent_token; else “aider”.
  - agent_args = CLI AGENT-ARGS... if provided; else recorded_agent_args.
  - root_flags = recorded_root_flags (unchanged).
- Final command executed per pane:
  aifo-coder [root_flags…] <agent_token> [agent_args…]
- Quoting:
  - tmux/Git Bash: POSIX shell quoting (shell_escape) per token; join.
  - PowerShell: single‑quote per token; double any single quotes inside.
  - If child_argv present, quote each token independently according to target shell.
- Update policy (best‑effort):
  - When writing metadata post‑attach, prefer updating child_argv with the effective argv; rewrite child_args as shell_join(child_argv) for back‑compat.

Preflight Policy (Reuse, stale cleanup, collisions)
- Toolchain sidecars/network (aifo‑tc‑<kind>‑<sid> and aifo‑net‑<sid>):
  - If present/running: reuse (log “reusing” in --verbose). If absent: create on demand later by agents as usual.
  - If clearly stale (e.g., stopped containers): best‑effort remove; proceed regardless of errors.
  - Designed for one active attach per sid; if concurrency occurs, reuse gracefully; do not fail.
- Agent containers per pane (aifo-coder-<agent>-<sid>-<index>):
  - If a container with the planned name exists and is stopped, remove it.
  - If running, attempt to stop it with a short timeout; proceed regardless; warn on failure (verbose).
  - When switching agents between recorded and effective (e.g., aider->codex), also check the recorded agent’s container name for the same sid/index and best‑effort stop/remove to avoid collisions.

Phased Plan

Phase 0 – Orchestrator Preconditions
- Linux/macOS: require tmux; exit 127 if missing.
- Windows: detect Windows Terminal (wt.exe), PowerShell (pwsh|powershell.exe), Git Bash/mintty (git-bash.exe|bash.exe|mintty.exe). If none found, exit 127.
- Attach is designed for a single active attach per sid. If sidecars/network already exist, reuse is allowed (log at verbose).

Phase 1 – Resolve Repository Root
- Use repo_root() to find the Git top‑level (canonical).
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
- Stable index = captured numeric suffix n; sort ascending by n for launch order; do not renumber; gaps are allowed.
- Validation per pane:
  - Valid if git -C <pane_dir> rev-parse --is-inside-work-tree returns true (accepts worktrees; do not check for .git).
  - Determine branch via git -C <pane_dir> rev-parse --abbrev-ref HEAD.
  - HEAD means detached; compute sha7 for display.
- Tolerant mode (default):
  - Skip invalid/missing; mark detached HEAD as valid=false with reason “detached”; proceed if at least one valid pane remains; else exit 1.
- Strict mode:
  - Require all panes valid and non‑detached; else exit 1.

Phase 4 – Read Metadata and Compute Effective Configuration
- Read <session_dir>/.meta.json (best‑effort).
- Fields consulted:
  - created_at, base_label, base_ref_or_sha, base_commit_sha, layout
  - panes, pane_dirs, branches
  - agent, child_argv, child_args, last_attached
- Compute effective layout (requested/effective mapping); invalid requested layout falls back to tiled with a verbose warning.
- Parse recorded argv (child_argv preferred; else child_args) into (root_flags, agent_token, agent_args); apply CLI overrides as defined above.

Phase 5 – Preflight and Cleanup (Best‑Effort)
- toolchain sidecars/network: reuse when present; if clearly stale, best‑effort cleanup; do not fail.
- For each planned pane, check agent container name “aifo-coder-<agent>-<sid>-<index>”:
  - If stopped, remove; if running, attempt to stop; proceed regardless; warn on failure (verbose).
  - Also check recorded agent’s name for the same sid/index and stop/remove to prevent collisions when switching agents.

Phase 6 – Build Attach Plan
- Compute:
  - sid, session_name (tmux: default “aifo-<sid>-attach”; Windows: cosmetic only), layout_requested, layout_effective
  - recorded_agent (if any), effective agent token
  - recorded_root_flags, recorded_agent_args, effective_agent_args
  - panes: [{ index, path, branch or “(detached HEAD @ <sha7>)”, valid, reason? }]
- If --dry-run:
  - Text plan (color‑aware): include all panes, flag invalid and reasons.
  - If --json: print JSON:
    {
      sid, session_name, layout_requested, layout_effective,
      recorded_agent, agent,
      recorded_root_flags: [string],
      recorded_agent_args: [string],
      effective_root_flags: [string],
      effective_agent_token: string,
      effective_agent_args: [string],
      panes: [{ index: number, path: string, branch: string, valid: boolean, reason?: string }],
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
  - AIFO_CODER_FORK_INDEX=<index>  # index = numeric suffix n
  - AIFO_CODER_FORK_STATE_DIR=<state_base>/<sid>/pane-<index> (ensure it exists; create .aider/.codex/.crush)
  - AIFO_CODER_SUPPRESS_TOOLCHAIN_WARNING=1
  - Optional reserved: AIFO_SESSION_NETWORK (not required by attach)
- Child command per pane:
  - cd <pane_dir>; exec aifo-coder [recorded_root_flags…] <effective_agent_token> [effective_agent_args…]
- Linux/macOS (tmux):
  - Create detached tmux session named session_name. If a tmux session with that name already exists, abort with exit 1 suggesting --session-name.
  - Split window into as many panes as valid clones; apply layout mapping (tiled/even-horizontal/even-vertical).
  - In each pane, send-keys a launch script that exports env, runs child command, then shows “press ‘s’ to open a shell; otherwise auto‑close the pane.”
  - If already in tmux (TMUX set), switch-client to the session; else attach-session.
- Windows:
  - Windows Terminal (wt.exe) preferred: new-tab for first pane, then split-pane for rest; orientation:
    - even-h: use -H consistently; even-v: use -V; tiled: alternate -H/-V starting with -H.
  - PowerShell fallback: Start-Process windows; keep -NoExit when no merge (attach always keeps shells open).
  - Git Bash/mintty fallback: bash -lc with exports and agent command; append “&& exec bash” to keep shell open.
  - --session-name is cosmetic (used in logs/tab titles only); no conflict detection on Windows.

Phase 8 – Metadata Update (Best‑Effort)
- Upsert minimally into <session_dir>/.meta.json:
  - last_attached: <unix_secs>
  - agent: "<aider|codex|crush>" (lowercase)
  - child_argv: [<tokens...>]          # preferred
  - child_args: "<flat-string>"         # shell-joined child_argv for back‑compat
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
- tmux session name conflict: exit 1 with guidance to use --session-name (tmux only; Windows ignores).
- --json without --dry-run: exit 1.
- --layout invalid: default to tiled; warn in --verbose; continue with exit 0 on success.

Security & Isolation
- Attach does not elevate privileges or change security posture.
- AppArmor/seccomp/cgroupns as applied by the agent launcher remain unchanged.
- No additional mounts beyond agent requirements. No Git writes by attach.

Logging
- Honor color policy (AIFO_CODER_COLOR auto|always|never and NO_COLOR).
- --verbose prints:
  - Session resolution details and chosen sid
  - Discovered panes: path, branch or “(detached HEAD @ sha7)”, valid/invalid reason
  - Recorded vs effective config (root flags, agent token/args); final composed child command per pane (properly quoted)
  - Orchestrator command previews (tmux/WT/PowerShell/Git Bash)
  - Metadata read/write status (best‑effort upserts) and paths
  - Preflight actions (sidecars/network reuse/stops; agent container stops/removals)

Metadata Model (Session .meta.json)
- Existing fields: created_at, base_label, base_ref_or_sha, base_commit_sha, panes, pane_dirs, branches, layout, snapshot_sha (optional)
- v1+/v2 fields: agent, child_args, child_argv (if present, preferred), last_attached
- v3.1 behavior:
  - Prefer child_argv; if absent, parse child_args; if both present, child_argv is authoritative and child_args should be rewritten on update using shell_join(child_argv).
  - Normalize agent to lowercase on update.

Acceptance Criteria
- Users can re‑open a session created with --fork N and --fork-merge-strategy none using aifo-coder fork attach <sid> without cloning or Git mutations.
- Tolerant‑by‑default attach proceeds with any valid panes; strict mode enforces all valid/non‑detached.
- Root flags are preserved; agent token and agent args are independently overridable.
- Toolchain sidecars/network are reused when present; stale agent containers are stopped/removed best‑effort.
- Behavior is consistent across Linux/macOS (tmux) and Windows (WT/PowerShell/Git Bash).
- Dry‑run text and JSON plans are clear, accurate, and exit 0.

Testing Plan (E2E/Integration)
1) Happy path (tmux): create fork (N=3, strategy=none); close panes; attach; verify valid panes; cwd per pane; agents launch; tmux shell prompt behavior.
2) Windows Terminal fallback chain: WT present -> panes open with layout mapping; if only PowerShell -> windows open; if only Git Bash/mintty -> windows open and shells persist.
3) Stable index: delete pane-1; attach; verify pane-2 launches with AIFO_CODER_FORK_INDEX=2, state dir .../pane-2, container name suffix -2.
4) Tolerant vs strict: make one pane non‑repo or detached; tolerant proceeds with remaining; strict exits 1.
5) Metadata back‑compat: remove child_argv; keep child_args; attach reconstructs argv; then add child_argv; attach prefers it; on update, both child_argv and child_args written.
6) Session resolution: create multiple sessions; set last_attached on one; verify latest/last picks it; unique‑prefix selects uniquely; ambiguous prefix lists candidates and fails.
7) JSON dry‑run: verify fields include recorded/effective splits and pane validity/warnings; exit 0; reject --json without --dry-run.
8) Container collision: leave agent container running (recorded agent), then switch agent on attach; attach stops both recorded and effective agent containers best‑effort before launching; verify no naming conflict remains.
9) Worktree validation: make pane a git worktree without .git dir; attach accepts via rev‑parse --is‑inside‑work‑tree.

Examples
- Re‑attach with recorded config:
  aifo-coder fork attach aifo-123abc
- Override layout and pass new agent args (root flags preserved):
  aifo-coder fork attach aifo-123abc --layout even-h -- --model anthropic/claude-3.5-sonnet
- Override agent (switch from aider to codex) and args:
  aifo-coder fork attach aifo-123abc --agent codex -- --help
- Attach latest with a custom tmux session name:
  aifo-coder fork attach latest --session-name dev-work
- Strict mode (require all panes valid and non‑detached):
  aifo-coder fork attach aifo-123abc --strict
- Preview only (text and JSON):
  aifo-coder fork attach aifo-123abc --dry-run
  aifo-coder fork attach aifo-123abc --dry-run --json

Versioning
- This document defines v3.1 of the fork attach feature.
- v1/v2/v3 sessions remain fully supported.
- v3.1 refines sidecar/network reuse policy, container collision handling, agent token fallback, child_argv preference, and the dry‑run JSON schema.
- Future revisions should append -v4, etc., and document backward compatibility.
