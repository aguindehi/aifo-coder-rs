AIFO Coder: Fork Attach Feature (v1) – Comprehensive Specification

Status
- Stage: v1 specification (production-ready design)
- Scope: CLI, behavior, metadata, orchestration, error handling, compatibility, testing

Motivation
When users create a fork session with aifo-coder --fork N and choose --fork-merge-strategy none, they often close all agent panes to pause work. Currently, there is no built-in mechanism to re-open the session and continue working on the created branches in their respective clone directories. The fork attach feature enables users to re-attach to an existing fork session and relaunch coding agents per pane without re-cloning or altering branches.

Goals
- Detect an existing fork session and its pane clones.
- Launch a new multi-pane agent session bound to the discovered clones (reuse directories; no new clones).
- Preserve per-pane context (branch, env, container naming pattern, session id).
- Provide a dry-run mode to preview actions.
- Be robust across platforms (Linux/macOS using tmux; Windows using Windows Terminal/PowerShell/Git Bash).

Non-Goals (v1)
- Automatic merging or branch manipulation (attach is non-mutating).
- Restoring toolchain sidecars to a prior state; attach starts fresh agents and (if enabled by user) new toolchains.
- Persisting or replaying every root CLI flag from the original run; v1 focuses on re-opening agents with the same agent and arguments (see Metadata).

Terminology
- Session id (sid): Unique identifier of a fork session (e.g., aifo-<random>).
- Pane: One clone directory in the session (pane-1..pane-N).
- Agent: Aider, Codex, or Crush (or other supported agent).
- Child args: The argument vector executed per pane after stripping fork flags; typically includes the agent subcommand and args.

CLI Specification
- Command
  aifo-coder fork attach <session> [--layout <tiled|even-h|even-v>] [--agent <aider|codex|crush>] [--dry-run] [--verbose] [--] [AGENT-ARGS...]

- Semantics
  - <session> is the session id directory name under .aifo-coder/forks. A future enhancement may add “latest/last” and unique-prefix matching; v1 requires exact dir name.
  - --layout overrides the layout; defaults to recorded layout in .meta.json; fallback tiled.
  - --agent overrides the agent type to launch; defaults to recorded agent in .meta.json; fallback aider.
  - --dry-run prints plan (panes, paths, branches, agent, args, layout) and exits 0.
  - --verbose increases logging.
  - [--] [AGENT-ARGS...] overrides recorded agent arguments; if omitted, use recorded child args or empty (see Metadata).

Behavior Overview
- Discover the specified fork session (sid) under the current repository.
- Derive effective configuration (agent, args, layout) from .meta.json with CLI overrides.
- Preflight checks and cleanup:
  - Verify panes exist and are valid Git repos; reject if none are valid.
  - Stop any stale toolchain sidecars and session network for sid (best-effort).
- Build and execute a new UI session with one pane per discovered clone:
  - On *nix: tmux session “aifo-<sid>-attach” (or similar) with requested layout.
  - On Windows: launch Windows Terminal (preferred), otherwise fall back to PowerShell windows or Git Bash/mintty.
  - Per pane, set env (AIFO_CODER_FORK_SESSION, AIFO_CODER_FORK_INDEX, etc.), cd into pane dir, and exec “aifo-coder <child-args>”.
- Update .meta.json with last_attached timestamp and effective agent/args/layout (best-effort).

Phased Plan

Phase 0 – Prerequisites & Platform Support
- Linux/macOS: Require tmux for fork attach (same as fork run).
- Windows: Prefer Windows Terminal (wt.exe). Fallback: PowerShell. Second fallback: Git Bash/mintty.
- Docker is used by the agents as usual. Attach itself does not require Docker, but agents will.

Phase 1 – Resolve Repository Root
- Use repo_root() to detect the current Git repository top-level.
- If not inside a Git repository, abort with exit 1 and a clear message.

Phase 2 – Resolve Session Directory
- Session directory: <repo_root>/.aifo-coder/forks/<sid>.
- If the directory is missing, abort with exit 1: “No such session ‘<sid>’.”
- v1 requires exact match of <sid>. (Future enhancement may support “latest” and unique-prefix.)

Phase 3 – Discover Panes and Branches
- Enumerate pane directories using pane_dirs_for_session(session_dir) (pane-N subdirectories).
- For each pane directory:
  - Validate pane_dir/.git exists (best-effort).
  - Determine current branch: git -C <pane_dir> rev-parse --abbrev-ref HEAD; if HEAD, branch may be considered detached (warn but continue).
- Gather a list of (pane_dir: PathBuf, branch: String) entries.
- If the resulting list is empty, abort with exit 1 (“no pane directories found under session”).

Phase 4 – Read Session Metadata and Compute Effective Configuration
- Read .meta.json (best-effort).
- Metadata fields (see Metadata Model below):
  - created_at, base_label, base_ref_or_sha, base_commit_sha, panes, pane_dirs, branches, layout, snapshot_sha, etc. (existing).
  - agent (optional, v1 addition).
  - child_args (optional, v1 addition; a flat string representing joined child args).
- Determine effective values with precedence:
  1) CLI overrides (layout, agent, AGENT-ARGS...).
  2) Recorded metadata (“layout”, “agent”, “child_args”).
  3) Defaults: layout=tiled; agent=aider; child_args empty.
- If AGENT-ARGS are provided on CLI, replace the recorded child args’ trailing agent arguments. If not provided, reuse recorded child args verbatim.

Phase 5 – Preflight and Cleanup
- Best-effort cleanup of stale sidecars and session network for this sid:
  - toolchain_cleanup_session(sid, verbose=false|CLI).
- No further mutation of pane clones is performed: attach must not create, checkout, merge, or delete branches.

Phase 6 – Build Attach Plan
- Compute N = number of panes discovered.
- Determine a UI session/window name:
  - On *nix (tmux): “aifo-<sid>-attach”
  - On Windows: the Terminal tab title or use the same name in logs.
- Build a structured plan containing:
  - sid, N, layout, agent, child_args (effective).
  - per-pane tuple: index (1..N), path, branch (for display only).
- If --dry-run, print plan; exit 0.

Phase 7 – Launch UI Orchestrator and Panes
- Shared per-pane environment exports (consistent with fork run):
  - AIFO_CODER_SKIP_LOCK=1
  - AIFO_CODER_CONTAINER_NAME=aifo-coder-<agent>-<sid>-<index>
  - AIFO_CODER_HOSTNAME=aifo-coder-<agent>-<sid>-<index>
  - AIFO_CODER_FORK_SESSION=<sid>
  - AIFO_CODER_FORK_INDEX=<index>
  - AIFO_CODER_FORK_STATE_DIR=<state_base>/<sid>/pane-<index>  (ensure directories exist)
  - AIFO_CODER_SUPPRESS_TOOLCHAIN_WARNING=1
  - Optionally set networking envs like AIFO_SESSION_NETWORK if needed by future enhancements (v1: not required).
- Child command executed in each pane:
  - cd <pane_dir>; exec aifo-coder <child_args>
  - child_args comes from effective configuration (see Phase 4).
- Orchestrators
  - Linux/macOS (tmux):
    - Create a detached tmux session with session/window name (e.g., aifo-<sid>-attach).
    - Split window into N panes; apply layout (tiled/even-horizontal/even-vertical).
    - In each pane, send-keys with the launch script that exports env, runs child command, and optionally presents a post-exit shell prompt (same UX as fork run).
    - Attach or switch to the session as in fork run.
  - Windows:
    - Preferred: Windows Terminal (wt.exe):
      - new-tab for pane 1; split-pane for additional panes.
      - Each pane runs PowerShell that sets env, Set-Location to pane_dir, then executes the agent command.
      - If merge strategy is none (typical for attach), keep shell open after agent exit to allow continued work.
    - Fallback: PowerShell windows via Start-Process, waiting behavior similar to fork run when needed.
    - Second fallback: Git Bash or mintty, running bash -lc with exports and agent command; keep shell open when appropriate.

Phase 8 – Metadata Update (Best-Effort)
- Update .meta.json to record attach activity and effective configuration:
  - last_attached: <unix_secs>
  - agent: "<aider|codex|crush>"
  - child_args: "<flat string>" (shell-escaped join)
  - layout: "<tiled|even-h|even-v>"
  - panes: <N> (actual; optional upsert)
- Use fork_meta_append_fields to upsert minimal fields without strict JSON parsing.
- Failures to write should not abort attach (log at verbose level).

Metadata Model (Session .meta.json)
- Existing fields (already produced during fork run):
  - created_at: number (unix secs)
  - base_label: string
  - base_ref_or_sha: string
  - base_commit_sha: string
  - panes: number (declared plan)
  - pane_dirs: [string]
  - branches: [string]
  - layout: string ("tiled"|"even-h"|"even-v")
  - snapshot_sha: string (optional)
- New/Updated fields (v1 attach support):
  - agent: string ("aider"|"codex"|"crush"), default “aider”
  - child_args: string (flat shell-joined arg vector suitable for re-exec)
  - last_attached: number (unix secs); set/updated on successful attach
  - panes: may be updated to actual count on attach (best-effort)

Data Handling Notes
- child_args should reflect exactly what fork panes executed (minus fork flags); use shell_join on the result of fork_build_child_args(cli) in the original fork path and persist it.
- On attach, if CLI provides AGENT-ARGS, override the trailing agent arguments (simplest approach: if AGENT-ARGS present, ignore recorded child_args and build new: "<agent> <AGENT-ARGS...>"). If not, use recorded child_args verbatim.
- Backward compatibility: sessions created before v1 (no agent/child_args/layout) still attach using defaults.

Security & Isolation
- Attach does not elevate privileges or change security posture.
- AppArmor/seccomp/cgroupns continue to apply inside agents per normal launch logic.
- No mounts beyond what the agent would normally use.
- No git writes performed by attach; read-only discovery.

Error Handling & Exit Codes
- Not in a Git repo: exit 1.
- Session not found: exit 1.
- No panes found: exit 1.
- Orchestrator not found: exit 127 (match fork run behavior where applicable).
- Partial failures launching windows result in non-zero exit and clear diagnostics; do not delete any pane directories.
- Dry-run always exits 0.

Logging
- --verbose prints:
  - Discovered panes and branches
  - Effective agent, args, layout
  - Orchestrator commands (previews)
  - Metadata read/write status (best-effort)

Compatibility & Migration
- v1 attach works with existing sessions that lack new fields by applying reasonable defaults.
- For new sessions, persist agent and child_args at fork creation time to improve attach fidelity.

Testing Plan (E2E/Integration)
1) Happy path (Linux/macOS tmux):
   - Create fork with N=2, strategy=none; close panes; run attach; verify tmux session with 2 panes; each pane cwd = pane dir; agent runs.
2) Windows Terminal:
   - Same as above but on Windows; verify wt.exe launched panes; keep shells open post-agent.
3) Dry-run:
   - Attach with --dry-run; verify printed plan; exit 0; no windows opened.
4) Missing session:
   - Attach non-existent sid; expect clear error; non-zero exit.
5) No panes:
   - Manually remove pane dirs; attach; error about no panes.
6) Back-compat meta:
   - Simulate old session without agent/child_args; ensure defaults used; attach succeeds.
7) Override args:
   - Attach with new agent args via [--] ...; verify panes use overridden args.
8) Layout override:
   - Attach with --layout even-h; verify layout applied.

Future Enhancements (Post-v1)
- Support <session> = latest/last, or unique-prefix matching with disambiguation.
- Tolerant mode for partially missing panes (skip missing; attach remaining).
- Persist and replay selected root flags (toolchain, flavor) on attach.
- Optional best-effort git fetch per pane before launch (--fetch).
- Named tmux session override (--fork-session-name) for attach.
- JSON output for --dry-run plan.

Implementation Notes (Informative)
- CLI: add ForkCmd::Attach { session, layout, agent, dry_run, agent_args }.
- main.rs: resolve repo_root(); call fork_attach(); return ExitCode from its result.
- fork.rs:
  - Implement fork_attach(repo_root, sid, layout_override, agent_override, agent_args_override, verbose, dry_run) -> io::Result<i32>.
  - Use pane_dirs_for_session() and collect current branches (like collect_pane_branches).
  - Call toolchain_cleanup_session(sid, verbose) before launching.
  - Build orchestrator commands following fork_run patterns to avoid duplication errors.
  - Upsert meta via fork_meta_append_fields with last_attached/agent/child_args/layout.
- guidance.rs: unchanged; optionally print guidance after attach (purely additive).

Acceptance Criteria
- Users can successfully re-open a session created with --fork N and --fork-merge-strategy none using aifo-coder fork attach <sid>.
- No clone, branch, or merge operations are performed by attach.
- Behavior is consistent across Linux/macOS (tmux) and Windows (WT/PowerShell/Git Bash).
- Dry-run provides a clear, actionable plan.
- Backward compatibility preserved; failures yield clear diagnostics without data loss.

Examples
- Re-attach with recorded agent/args/layout:
  aifo-coder fork attach aifo-123abc
- Override layout and pass new agent args:
  aifo-coder fork attach aifo-123abc --layout even-h -- --model anthropic/claude-3.5-sonnet
- Preview only:
  aifo-coder fork attach aifo-123abc --dry-run

Versioning
- This document defines v1 of the fork attach feature. Subsequent revisions should append -v2, -v3, etc., and note backward-compat implications.
