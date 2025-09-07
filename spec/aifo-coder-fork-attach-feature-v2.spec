AIFO Coder: Fork Attach Feature (v2) – Comprehensive, Production-Ready Specification

Status
- Stage: v2 specification (refined, production-ready design)
- Scope: CLI, behavior, metadata, orchestration, error handling, compatibility, testing
- Compatibility: Backward compatible with v1 sessions; clarifies and hardens override semantics; adds small QoL flags

Motivation
When users create a fork session with aifo-coder --fork N and choose --fork-merge-strategy none, they often close all agent panes to pause work. There must be a built-in mechanism to re-open the session and continue working on the created branches in their respective clone directories. The fork attach feature enables users to re-attach to an existing fork session and relaunch coding agents per pane without re-cloning or altering branches.

Goals
- Detect an existing fork session and its pane clones.
- Launch a new multi-pane agent session bound to the discovered clones (reuse directories; no new clones).
- Preserve per-pane context (path, branch, env, container naming pattern, session id).
- Provide a dry-run mode with clear plan output (text, and optionally JSON).
- Be robust across platforms:
  - Linux/macOS: tmux orchestration.
  - Windows: Windows Terminal (preferred), PowerShell, or Git Bash/mintty fallback.
- Persist minimal metadata to make attach faithful to original fork runs.
- Tolerant-by-default: proceed when at least one pane is valid; strict mode opt-in.

Non-Goals (v2)
- Any mutation of clones: no clone creation, no checkout, no fetch by default, no merges, no deletions.
- Restoring prior toolchain sidecars or merging; attach always starts fresh agents (and toolchains if requested by user).
- Persisting or replaying every root CLI flag from the original run beyond what is explicitly specified (see Metadata and Override Semantics).

Terminology
- Session id (sid): Unique identifier of a fork session (e.g., aifo-<random>).
- Pane: One clone directory in the session (pane-1..pane-N).
- Agent: Aider, Codex, or Crush (or other supported agent).
- Root flags: The subset of child args prior to the agent token (e.g., --image, --toolchain flags).
- Agent args: The argument vector after the agent token, including the optional “--” boundary and any trailing args.

Supported Platforms & Prerequisites
- Linux/macOS: require tmux for attach (same as fork run).
- Windows: prefer Windows Terminal (wt.exe); fallback PowerShell; second fallback: Git Bash or mintty.
- Docker: used by agents as usual; attach itself does not require Docker; agents will.

CLI Specification
- Command
  aifo-coder fork attach <session>
  [--layout <tiled|even-h|even-v>]
  [--agent <aider|codex|crush>]
  [--session-name <name>]
  [--strict]
  [--dry-run]
  [--json]         # for dry-run plan output
  [--verbose]
  [--] [AGENT-ARGS...]

- Semantics
  - <session> is the session directory name under .aifo-coder/forks. v2 additionally supports:
    - latest or last: resolves to the most recently active session using last_attached (desc); fall back to created_at, then directory mtime if last_attached is missing.
    - Unique prefix match: if the provided string uniquely matches a session directory prefix; error if ambiguous.
  - --layout overrides the layout; defaults to recorded layout in .meta.json; fallback tiled. Mapped as:
    - tiled -> tmux “tiled”
    - even-h -> tmux “even-horizontal”
    - even-v -> tmux “even-vertical”
  - --agent overrides the agent binary to launch (aider|codex|crush).
  - --session-name overrides the UI session name; default “aifo-<sid>-attach”. If a tmux/WT session with the same name already exists, attach aborts unless the user overrides the name.
  - --strict switches discovery to strict mode (all panes must be valid repos on non-detached branches); default is tolerant (skip invalid/missing).
  - --dry-run prints the attach plan and exits 0; no windows are opened, no toolchains started.
  - --json is valid only with --dry-run; prints a machine-readable plan.
  - --verbose increases logging (discovery details, orchestrator previews, metadata read/write).
  - [--] [AGENT-ARGS...] replaces the recorded agent arguments (see Override Semantics).

Behavior Overview
- Resolve the current repository root; resolve <session> (exact, latest/last, or unique prefix).
- Discover pane directories pane-<n> (numeric n), sorted ascending; tolerant by default (skip invalid); strict mode requires all valid.
- Read .meta.json (best-effort), derive effective configuration (agent, args, layout) with precise override semantics:
  - Recorded root flags are preserved; agent token and agent args are independently overridable.
- Preflight cleanup: best-effort stop of stale toolchain sidecars and session network for sid.
- Launch a new UI session with one pane per discovered clone:
  - Linux/macOS: tmux session “aifo-<sid>-attach” (default) with requested/effective layout.
  - Windows: Windows Terminal preferred; fallback to PowerShell or Git Bash/mintty.
  - For each pane: export AIFO_CODER_* env vars, cd into pane dir, and exec “aifo-coder [effective child args]”.
  - In attach, shells stay open by default after agent exit (PowerShell -NoExit; Git Bash “&& exec bash”; tmux prompt to open shell).
- Update .meta.json with last_attached and effective agent/args/layout (best-effort).

Override Semantics (Precise, Lossless Root Flags)
- Metadata persistence at fork creation must include:
  - agent: "aider"|"codex"|"crush"
  - child_args: string; a flat, shell-joined arg vector representing the complete child argv executed by fork panes (JSON-escaped).
  - Optional (forward-looking): child_argv: array of strings; if present, it supersedes child_args to avoid quoting ambiguities.

- Parsing recorded child args:
  - Tokenize recorded child_args using the same tokenizer used by the launcher (e.g., shell_like_split_args).
  - Identify the first token among {"aider","codex","crush"} as the agent token (recorded_agent_token).
  - recorded_root_flags = tokens before the agent token.
  - recorded_agent_args = tokens after the agent token (may include “--” and trailing args verbatim).

- Effective values:
  1) Agent token = CLI --agent if provided; else recorded_agent_token; else “aider”.
  2) Agent args = tokens after “--” in the attach CLI if provided; else recorded_agent_args (verbatim).
  3) Root flags = recorded_root_flags (always preserved as-is from metadata).

- Final child command per pane:
  aifo-coder [recorded_root_flags…] <effective_agent_token> [effective_agent_args…]

- Edge cases:
  - If metadata lacks child_args and child_argv, synthesize minimal argv:
    - recorded_root_flags = []
    - recorded_agent_token = agent from metadata if present, else “aider”
    - recorded_agent_args = []
    - Apply overrides as defined above.

Phased Plan

Phase 0 – Platform & Orchestrator Preconditions
- Linux/macOS: require tmux; exit 127 if missing.
- Windows: detect in this preference order: Windows Terminal (wt.exe), PowerShell (pwsh|powershell.exe), Git Bash/mintty (git-bash.exe|bash.exe|mintty.exe). If none found, exit 127.
- Docker availability is not required to run attach; agents may require Docker at launch time.

Phase 1 – Resolve Repository Root
- Use repo_root() to detect the current Git repository top-level (canonicalized).
- If not inside a Git repository, abort with exit 1 and a clear message.

Phase 2 – Resolve Session Directory
- Base directory: <repo_root>/.aifo-coder/forks.
- Resolution:
  - Exact: if <session> matches a directory name, use it.
  - latest/last: choose the session with greatest last_attached; else greatest created_at; else latest directory mtime.
  - Unique prefix: match directories starting with the given prefix; error if ambiguous or none found.
- If no session directory is found, abort with exit 1: “No such session ‘<sid>’.”

Phase 3 – Discover Panes and Branches
- Enumerate pane directories pane-<n> with numeric suffix (strictly match ^pane-[1-9][0-9]*$).
- Sort panes by n ascending; index = 1..N in this order (stable).
- Validation per pane:
  - Must contain a .git directory (or valid Git worktree metadata).
  - Determine the current branch: git -C <pane_dir> rev-parse --abbrev-ref HEAD.
  - Detached HEAD (HEAD) is allowed in tolerant mode (emit warning); in strict mode, detached HEAD is invalid.
- Tolerant mode (default):
  - Skip invalid/missing panes with warnings; proceed if at least one valid pane is discovered.
- Strict mode:
  - Abort if any pane is invalid or detached; exit 1.
- If the resulting list is empty, abort with exit 1: “no valid pane directories found under session.”

Phase 4 – Read Session Metadata and Compute Effective Configuration
- Read <session_dir>/.meta.json (best-effort).
- Supported fields (case-sensitive keys):
  - created_at: number (unix secs)
  - base_label: string
  - base_ref_or_sha: string
  - base_commit_sha: string
  - panes: number (declared plan)
  - pane_dirs: [string]
  - branches: [string]
  - layout: string ("tiled"|"even-h"|"even-v")
  - snapshot_sha: string (optional)
  - agent: string ("aider"|"codex"|"crush") – v1+ addition
  - child_args: string (flat shell-joined arg vector) – v1+ addition
  - child_argv: [string] (preferred when present) – v2 optional
  - last_attached: number (unix secs) – v1+ addition, updated by attach
- Precedence for effective config:
  1) CLI overrides (layout, agent, AGENT-ARGS…).
  2) Recorded metadata (“layout”, “agent”, “child_argv”|“child_args”).
  3) Defaults: layout=tiled; agent=aider; args=[].

- Layout mapping:
  - tiled -> tmux “tiled”
  - even-h -> tmux “even-horizontal”
  - even-v -> tmux “even-vertical”

Phase 5 – Preflight and Cleanup
- Best-effort cleanup of stale sidecars and session network for sid:
  - toolchain_cleanup_session(sid, verbose=false|CLI).
- No mutation of pane clones is performed: attach must not create, checkout, fetch, merge, or delete branches.

Phase 6 – Build Attach Plan
- Compute N = number of valid panes discovered.
- Determine UI session/window name:
  - Default: “aifo-<sid>-attach” (cross-platform).
  - Override with --session-name.
  - If a session with that name already exists, abort with exit 1 and a clear message (no reuse in v2).
- Build a structured plan:
  - sid, N, layout (requested/effective), agent, child args (effective), session_name.
  - Per-pane tuple: index (1..N), path, branch (or “(detached HEAD)”).
- If --dry-run:
  - Print plan in text; include all panes.
  - If --json, print a JSON plan with fields: { sid, session_name, layout_requested, layout_effective, agent, child_args, panes: [{ index, path, branch, valid, reason? }], errors: [] }
  - Exit 0.

Phase 7 – Launch UI Orchestrator and Panes
- Shared per-pane environment:
  - AIFO_CODER_SKIP_LOCK=1
  - AIFO_CODER_CONTAINER_NAME=aifo-coder-<agent>-<sid>-<index>
  - AIFO_CODER_HOSTNAME=aifo-coder-<agent>-<sid>-<index>
  - AIFO_CODER_FORK_SESSION=<sid>
  - AIFO_CODER_FORK_INDEX=<index>
  - AIFO_CODER_FORK_STATE_DIR=<state_base>/<sid>/pane-<index>  (ensure directories exist; create .aider/.codex/.crush)
  - AIFO_CODER_SUPPRESS_TOOLCHAIN_WARNING=1
  - Optional envs reserved: AIFO_SESSION_NETWORK (not required by attach).

- Child command executed in each pane:
  - cd <pane_dir>; exec aifo-coder [recorded_root_flags…] <effective_agent_token> [effective_agent_args…]
  - Effective values from Phase 4 (Override Semantics).

- Orchestrators:
  - Linux/macOS (tmux):
    - Create a detached tmux session named session_name.
    - Split window into N panes; apply layout (tiled/even-horizontal/even-vertical).
    - In each pane, send-keys a launch script that exports env, runs child command, then offers:
      - A short prompt: press ‘s’ to open a shell; otherwise auto-close the pane to keep tmux clean (same UX as fork run).
    - Attach or switch to the session as in fork run.
  - Windows:
    - Windows Terminal (wt.exe):
      - new-tab for pane 1; split-pane for additional panes; choose orientation to mimic requested layout.
      - Each pane runs PowerShell with -NoExit, sets env vars, Set-Location to pane_dir, then executes the agent command.
      - Attach never merges; shells remain open after agent exit for continued work.
    - Fallback: PowerShell via Start-Process (keep window open; after all panes start, optionally wait when needed).
    - Second fallback: Git Bash or mintty (bash -lc), exporting env vars; append “&& exec bash” to keep shell open.

Phase 8 – Metadata Update (Best-Effort)
- Update .meta.json in <session_dir> to record attach activity and effective configuration:
  - last_attached: <unix_secs>
  - agent: "<aider|codex|crush>"
  - child_args: "<flat string>" (shell-joined final arg vector)
  - layout: "<tiled|even-h|even-v>"
  - panes: <N> (actual count; upsert)
- Use fork_meta_append_fields (or equivalent) to upsert minimal fields without strict JSON parsing.
- Failures to write should not abort attach (log at verbose level).

Error Handling & Exit Codes
- Not in a Git repo: exit 1.
- Session not found: exit 1.
- Orchestrator not found (tmux missing on *nix; no WT/PowerShell/Git Bash on Windows): exit 127.
- Tolerant mode: if no valid panes found: exit 1; otherwise continue with valid panes and warn.
- Strict mode: any invalid pane (non-repo, detached) aborts: exit 1.
- UI session name conflict: exit 1 with guidance to use --session-name.
- Dry-run always exits 0.

Security & Isolation
- Attach does not change privileges or security posture:
  - AppArmor/seccomp/cgroupns continue to apply inside agents per normal launch logic (desired_apparmor_profile).
- No additional mounts beyond agent needs.
- Attach performs no git writes; read-only discovery and orchestration.

Logging
- --verbose prints:
  - Discovered panes and branches (including skipped/invalid reasons).
  - Effective agent, args, layout; final composed child command per pane (quotes preserved).
  - Orchestrator commands (previews) for tmux/WT/PowerShell/Git Bash.
  - Metadata read/write status (best-effort upserts).

Metadata Model (Session .meta.json)
- Existing fields:
  - created_at: number (unix secs)
  - base_label: string
  - base_ref_or_sha: string
  - base_commit_sha: string
  - panes: number
  - pane_dirs: [string]
  - branches: [string]
  - layout: "tiled"|"even-h"|"even-v"
  - snapshot_sha: string (optional)
- New/Updated fields (v1+):
  - agent: string ("aider"|"codex"|"crush"), default “aider” when absent
  - child_args: string (flat, shell-joined child argv)
  - child_argv: [string] (optional; preferred if present)
  - last_attached: number (unix secs); set/updated on successful attach
  - panes: may be upserted to actual count on attach

Backwards Compatibility
- Sessions created before v1 (no agent/child_args/layout) still attach using defaults: layout=tiled; agent=aider; child args empty.
- If child_argv is absent, use child_args; if both absent, synthesize minimal argv per Override Semantics.

Testing Plan (E2E/Integration)
1) Happy path (Linux/macOS tmux):
   - Create fork with N=2, strategy=none; close panes; run attach; verify tmux session with 2 panes; cwd = pane dir; agent runs; shells behave per design (prompt to open shell).
2) Windows Terminal:
   - Same as above on Windows; verify wt.exe launched panes; shells remain open; env vars set.
3) Dry-run:
   - Attach with --dry-run and with --dry-run --json; verify printed plan; exit 0; no windows opened.
4) Missing session:
   - Attach non-existent sid; clear error; exit 1.
5) Tolerant vs strict:
   - Remove one pane dir; tolerant mode proceeds with remaining valid panes; strict mode errors.
   - Create a detached HEAD pane; tolerant warns; strict errors.
6) Back-compat meta:
   - Simulate old session without agent/child_args; ensure defaults used; attach succeeds.
7) Override args:
   - Attach with new agent args via [--] ...; verify panes use overridden args while preserving recorded root flags.
8) Layout override:
   - Attach with --layout even-h; verify layout mapping to even-horizontal applies.
9) Session resolution:
   - Attach latest/last works; unique prefix match resolves; ambiguous prefix errors.

Implementation Notes (Informative)
- CLI: add ForkCmd::Attach {
    session: String,
    layout: Option<String>,
    agent: Option<String>,
    session_name: Option<String>,
    strict: bool,
    dry_run: bool,
    json: bool,
    verbose: bool (reuse root),
    #[arg(trailing_var_arg = true)] agent_args: Vec<String>,
  }.
- main.rs: resolve repo_root(); call fork_attach(); return ExitCode from its result.
- fork.rs:
  - Implement fork_attach(repo_root, sid_or_selector, layout_override, agent_override, agent_args_override, session_name_override, strict, verbose, dry_run, json) -> io::Result<i32>.
  - Session resolution helpers: exact + latest/last + unique-prefix; sort by last_attached/created_at/mtime.
  - pane_dirs_for_session(): filter names matching ^pane-[1-9][0-9]*$; sort by numeric suffix.
  - Determine branches; enforce tolerant/strict behavior.
  - Override semantics: parse child_argv/child_args; compute final argv; preserve recorded root flags.
  - Orchestrator commands follow fork_run patterns (tmux/Windows). Keep shells open on attach.
  - Upsert meta via fork_meta_append_fields with last_attached/agent/child_args/layout/panes.
- guidance.rs: unchanged; attach prints plan and does not attempt merges.

Acceptance Criteria
- Users can re-open a session created with --fork N and --fork-merge-strategy none using aifo-coder fork attach <sid>.
- Attach performs no clone, checkout, fetch, merge, or delete operations.
- Behavior consistent across Linux/macOS (tmux) and Windows (WT/PowerShell/Git Bash).
- Dry-run provides a clear plan; JSON output available with --json.
- Recorded root flags are preserved; agent and agent args are independently overridable.
- Tolerant-by-default; strict mode opt-in; errors and exit codes are consistent and documented.

Examples
- Re-attach with recorded agent/args/layout:
  aifo-coder fork attach aifo-123abc
- Override layout and pass new agent args (preserving recorded root flags):
  aifo-coder fork attach aifo-123abc --layout even-h -- --model anthropic/claude-3.5-sonnet
- Override agent (switch from aider to codex) and args:
  aifo-coder fork attach aifo-123abc --agent codex -- --help
- Use latest session and a custom UI name:
  aifo-coder fork attach latest --session-name dev-work
- Strict mode (require all panes valid; fail on detached HEAD or missing .git):
  aifo-coder fork attach aifo-123abc --strict
- Preview only (text and JSON):
  aifo-coder fork attach aifo-123abc --dry-run
  aifo-coder fork attach aifo-123abc --dry-run --json

Versioning
- This document defines v2 of the fork attach feature.
- v1 sessions remain fully supported; v2 clarifies override semantics, adds tolerant/strict behavior, session resolution conveniences, and optional JSON dry-run output.
- Future revisions should append -v3, -v4, etc., and specify backward-compat implications.
