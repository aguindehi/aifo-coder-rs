AIFO-Coder Fork Support v2 Specification

Status
- Version: 2
- Date: 2025-08-31
- Owner: AIFO Coder maintainers
- State: Draft (ready for implementation)

Overview
The --fork feature runs N concurrent aifo-coder agents (Codex, Crush, Aider) in one terminal using tmux panes. Each agent operates in an isolated Git worktree on its own fork branch, sharing the repository object database while avoiding .git and workspace file contention. v2 strengthens concurrency guarantees beyond v1 by ensuring:
- Per-pane workspace isolation via Git worktrees (unchanged from v1).
- Per-pane agent state isolation for mutable agent home directories (.aider, .codex, .crush), preventing cross-influence of configs and metadata.
- Per-pane unique container names and hostnames to avoid Docker resource collisions.
- Robust rollback on failures and safer argument handling.

Why v2?
v1 chose Git worktrees and tmux, which is correct. However, several edge cases can still cause cross-interference:
- Agent home state (e.g., ~/.aider) was shared and could race under concurrent writes.
- Container names were derived from static defaults and could collide.
- Worktree creation failure and tmux orchestration errors could leave partial state.
v2 addresses these with stronger isolation, robust cleanup on failures, and precise argument handling.

Motivation
- Safe, ergonomic concurrency for multiple agents.
- Zero race conditions in .git/index, workspace files, agent state files, and Docker resources.
- Preserve performance benefits of shared object stores and shared content-addressed caches.

Goals
- N >= 2 concurrent agents, each:
  - In its own Git worktree and branch.
  - With its own container name/hostname.
  - With isolated agent state directories (home-level mutable state).
- tmux session with N panes in a tiled layout.
- Global lock preserved for single runs; bypassed intentionally for forked panes.
- Clear errors and robust rollback on failures.
- Minimal user prerequisites (git, tmux, docker).

Non-Goals
- Automatic merging/rebasing of fork branches back to the base branch.
- Advanced tmux layout customization (tiled only in v2).
- Reuse of an existing tmux window (always creates a new session/window in v2).

User Experience

CLI
- New flag: --fork <N>
  - Enables forking when N >= 2. If N < 2, behaves as a normal single-agent run.
  - Applies to any subcommand (codex, crush, aider, toolchain, etc.).
  - Example: aifo-coder --fork 4 aider -- --model my-model --verbose

Optional flags (planned in v2)
- --fork-include-dirty
  - Snapshot and include current uncommitted changes into each fork via a safe mechanism (see Uncommitted Changes Policy).
- --fork-session-name <name>
  - Set tmux session name explicitly; default is aifo-<session-id>.
- --fork-layout <tiled|even-h|even-v>
  - Choose pane layout; default is tiled.

User Flow

1) Preconditions
- tmux must be installed and in PATH. If missing: error and exit 127.
- Must be inside a Git repository (git rev-parse --show-toplevel). If not: error and exit 1.
- Recommend clean working tree; see Uncommitted Changes Policy.

2) Discover base
- repo root: git rev-parse --show-toplevel
- base branch: git rev-parse --abbrev-ref HEAD
  - If detached HEAD, record the commit SHA and set base label “detached”; branch creation will be from that SHA.

3) Session id
- Create a short unique session id from time ⊕ pid (base36). Used for directory and branch uniqueness.

4) Worktree setup
- Create directories: <repo-root>/.aifo-coder/forks/<session-id>/pane-1..N
- For pane i (1..N):
  - Branch: fork/<base-label>/<session-id>-<i>
    - If base is a branch: base_label = <branch>, create from <branch>.
    - If base is detached: base_label = detached, create from <sha>.
  - Run:
    - Detached base: git worktree add -b <branch> <path> <sha>
    - Normal base:   git worktree add -b <branch> <path> <base-branch>
- If any worktree creation fails, perform best-effort rollback:
  - git worktree remove --force <created-paths>
  - git branch -D <created-branches> (when safe)
  - Exit non-zero.

5) Build child args (minus --fork)
- Construct child arguments from the parsed CLI to avoid mis-removal across “--”.
  - Drop both “--fork N” and “--fork=N” from the top-level args only.
  - Preserve everything after the subcommand’s “--”.

6) Per-pane environment isolation
- For each pane i:
  - AIFO_CODER_SKIP_LOCK=1 to bypass top-level lock.
  - AIFO_CODER_CONTAINER_NAME=aifo-coder-<agent>-<session-id>-<i>
  - AIFO_CODER_HOSTNAME=aifo-coder-<agent>-<session-id>-<i>
  - AIFO_CODER_FORK_SESSION=<session-id>
  - AIFO_CODER_FORK_INDEX=<i>
  - AIFO_CODER_FORK_STATE_DIR=<host-state-base>/<session-id>/pane-<i>
    - host-state-base defaults to ~/.aifo-coder/state
    - Implementation must mount per-pane state directories:
      - Map <state-dir>/.aider  -> /home/coder/.aider
      - Map <state-dir>/.codex  -> /home/coder/.codex
      - Map <state-dir>/.crush  -> /home/coder/.crush
    - Rationale: avoid concurrent writes to the same host ~/.aider/.codex/.crush between panes.
- Shared content-addressed caches remain shared and global:
  - npm, pip, cargo, ccache, go named volumes are safe to share and improve performance.

7) tmux orchestration
- Create a new session (detached), window “aifo-fork”.
- First pane:
  - tmux new-session -d -s <session> -n aifo-fork -c <pane1-path> '<envs> aifo-coder <child-args>'
- Additional panes 2..N:
  - tmux split-window -t <session>:0 -c <paneX-path> '<envs> aifo-coder <child-args>'
- Apply layout:
  - tmux select-layout -t <session>:0 tiled (or per flag)
  - tmux set-window-option -t <session>:0 synchronize-panes off
- Attach:
  - If TMUX env present: tmux switch-client -t <session>
  - Else: tmux attach-session -t <session>
- If tmux session startup fails, attempt to rollback created worktrees (best effort), then exit non-zero.

8) Agent concurrency
- Each pane runs in its worktree directory; agent mounts that as /workspace.
- Each pane’s agent uses unique container name/hostname via env vars (no collisions).
- Each pane’s agent uses its own proxy session id and network (created by each child run).
- Shared named caches (npm/pip/cargo/ccache/go) remain shared.

9) Cleanup
- Worktrees remain after the session for review and merging.
- A future helper will provide listing and cleanup (fork list/clean).
- On errors during setup (worktree creation or tmux orchestration), rollback is attempted.

Uncommitted Changes Policy
- Default: proceed even if the main workspace is dirty; forks start from base HEAD (or detached SHA). Print a clear warning recommending commit/stash if users want those changes included.
- Optional v2 flag: --fork-include-dirty
  - Implementation strategies (choose one, behind the flag):
    1) Stash-and-apply:
       - git stash push --include-untracked --message "aifo-fork-<sid>"
       - For each worktree: git -C <pane> stash apply --index
       - At the end, leave the main workspace unchanged; stashes remain (document command to drop).
       - Caveat: merge conflicts may occur per pane; surface clearly.
    2) Snapshot commit:
       - Create a temporary WIP commit on the base (no push), then branch forks from that commit.
       - Avoids stash semantics; modifies history locally; requires user consent (via the flag).
- v2 default remains: do not include dirty changes automatically.

Git Branch and Directory Layout
- Branch name: fork/<base-label>/<session-id>-<i>
  - Examples: fork/main/abc123-1, fork/feature-x/abc123-2, fork/detached/abc123-3
- Worktrees:
  - <repo-root>/.aifo-coder/forks/<session-id>/pane-1..N
- Merge strategy:
  - Users merge fork/* branches back into base via normal Git flows.

System Requirements
- tmux in PATH.
- git in PATH; current directory inside a Git repo.
- Docker available for agents and toolchains.

Failure Modes and Messages
- tmux not found: “Error: --fork requires 'tmux' but it was not found in PATH.” Exit 127.
- Not a Git repo: “Error: --fork requires running inside a Git repository to create worktrees safely.” Exit 1.
- Worktree creation failure: print failed pane index; best-effort rollback of prior worktrees/branches; exit non-zero.
- tmux session start failure: attempt rollback of created worktrees; “Error: failed to start tmux session.” Exit 1.

Concurrency and Safety

Workspace isolation
- Each agent operates in its own worktree directory -> no .git/index lock contention, no overlapping temp files, no editor/backups collision.

Agent state isolation (NEW in v2)
- Per-pane state directories on the host are mounted for .aider/.codex/.crush:
  - Host: ~/.aifo-coder/state/<sid>/pane-<i>/{.aider,.codex,.crush}
  - Container: /home/coder/{.aider,.codex,.crush}
- Prevents concurrent writes to shared host ~/.aider/.codex/.crush, avoiding subtle cross-influence and races.

Container isolation (strengthened)
- AIFO_CODER_CONTAINER_NAME and AIFO_CODER_HOSTNAME are set uniquely per pane to avoid name collisions.
- Each agent run creates its own network/session id; no port/name collisions for proxies or sidecars.

Caches
- Named Docker volumes (cargo, npm, pip, ccache, go) remain shared and are safe (content-addressed); they improve performance.

Global lock
- Parent process operates normally (no agent run).
- Child panes set AIFO_CODER_SKIP_LOCK=1 so single-run lock is bypassed intentionally.
- Normal (non-fork) runs continue to enforce one-at-a-time execution.

Security
- Pane commands constructed via shell_join with conservative POSIX escaping.
- Fork branches and worktrees avoid shared mutable state across panes.
- No elevated privileges beyond standard Docker usage.

Performance
- Worktrees are fast and space-efficient (shared object store).
- Shared caches accelerate cold starts across panes.
- Each pane uses separate containers; resource usage scales with N; users should choose N according to host capacity.

Implementation Plan (High-Level)

CLI Additions (src/main.rs)
- Extend Cli:
  - #[arg(long)]
    fork: Option<usize>,
  - #[arg(long)] fork_include_dirty: bool (optional in v2)
  - #[arg(long)] fork_session_name: Option<String> (optional in v2)
  - #[arg(long)] fork_layout: Option<String> with validation (optional in v2)
- Behavior:
  - If Some(n) and n >= 2 -> enter fork orchestrator path early and return ExitCode from there (parent does not run an agent).

Skip-Lock Mechanism (src/main.rs)
- Around acquire_lock(): if env AIFO_CODER_SKIP_LOCK == "1", skip acquiring the lock.

Fork Orchestrator (src/main.rs)
- fork_run(n, &Cli) -> ExitCode:
  1) Preflight: which::which("tmux"); error 127 if missing.
  2) git rev-parse --show-toplevel; error if not inside repo.
  3) Identify base:
     - branch with git rev-parse --abbrev-ref HEAD
     - if “HEAD”, read commit sha with git rev-parse --verify HEAD and set base label “detached”.
  4) Session id creation (time ⊕ pid, base36). Compute session name from CLI or default aifo-<sid>.
  5) Create forks base dir: <root>/.aifo-coder/forks/<sid>.
  6) For i in 1..=n:
     - Create pane dir.
     - Create branch fork/<base>/<sid>-<i> from base (branch or sha).
     - git worktree add -b <branch> <path> <base-or-sha>
     - If any failure -> rollback (worktree remove and branch delete) and exit 1.
  7) Build child args:
     - Rebuild from parsed Cli (not argv scanning), dropping --fork and its value (both --fork N and --fork=N).
     - Preserve agent subcommand and tail args after the subcommand’s “--”.
  8) Build per-pane env:
     - Always set AIFO_CODER_SKIP_LOCK=1.
     - AIFO_CODER_CONTAINER_NAME / AIFO_CODER_HOSTNAME unique per pane.
     - AIFO_CODER_FORK_SESSION=<sid>, AIFO_CODER_FORK_INDEX=<i>.
     - AIFO_CODER_FORK_STATE_DIR=~/.aifo-coder/state/<sid>/pane-<i>.
     - If --fork-include-dirty is set, either stash-and-apply per pane or snapshot commit (see policy).
  9) tmux orchestration:
     - tmux new-session -d -s <session> -n aifo-fork -c <pane1-path> '<envs> aifo-coder <args>'
     - For panes 2..N: tmux split-window -t <session>:0 -c <paneX-path> '<envs> aifo-coder <args>'
     - tmux select-layout tiled (or per flag)
     - tmux set-window-option -t <session>:0 synchronize-panes off
     - Attach or switch to the new session.
     - If tmux fails, attempt rollback of worktrees and exit non-zero.
  10) Return ExitCode::from(0).

Agent Launcher Changes (src/lib.rs build_docker_cmd)
- When AIFO_CODER_FORK_STATE_DIR is set:
  - Replace host mounts for ~/.aider, ~/.codex, ~/.crush with:
    - <dir>/.aider  -> /home/coder/.aider
    - <dir>/.codex  -> /home/coder/.codex
    - <dir>/.crush  -> /home/coder/.crush
  - Ensure directories exist (create on host).
- Container naming:
  - Honor AIFO_CODER_CONTAINER_NAME and AIFO_CODER_HOSTNAME if set (already supported); v2 requires the fork orchestrator to set them per pane.

Diagnostics
- On starting fork mode, print a summary line:
  - “aifo-coder: fork session <sid> on base <base-label>; created N worktrees under .aifo-coder/forks/<sid>”
  - If dirty and not including dirty: print warning.

Environment Variables
- AIFO_CODER_SKIP_LOCK=1 (child panes)
- AIFO_CODER_FORK_SESSION / AIFO_CODER_FORK_INDEX (for diagnostics/telemetry)
- AIFO_CODER_FORK_STATE_DIR (host path for per-pane agent state)
- AIFO_CODER_CONTAINER_NAME / AIFO_CODER_HOSTNAME (unique per pane)
- TMUX presence determines attach vs switch

Testing Strategy

Unit
- Argument stripping: ensure both --fork N and --fork=N are removed; ensure tokens after “--” (agent args) are unchanged.
- Branch name generation and detached HEAD handling.
- Construction of per-pane env vars and derived container names.
- build_docker_cmd honors AIFO_CODER_FORK_STATE_DIR mounts.

E2E (manual or CI with tmux)
- Verify creation of N worktrees and branches.
- Verify tmux session with N panes, each running aifo-coder in the correct pane path.
- Verify each agent gets a unique container name and hostname.
- Verify /workspace mount points to the correct worktree per pane.
- Verify per-pane agent state directories on host are created and mounted.
- Confirm concurrent runs succeed without file/.git lock contention.

Negative
- Missing tmux: exit 127.
- Outside Git repo: exit non-zero with clear error.
- Worktree creation failure mid-way: prior worktrees removed; exit non-zero.
- tmux session start failure: worktrees removed; exit non-zero.

Acceptance Criteria
- For N >= 2:
  - Creates N worktrees under .aifo-coder/forks/<sid>/pane-1..N.
  - Creates N branches fork/<base|detached>/<sid>-<i> from the correct base.
  - Starts a new tmux session with N panes; each runs aifo-coder in its own worktree directory.
  - Sets AIFO_CODER_SKIP_LOCK=1 for child panes; normal runs still acquire a global lock.
  - Each pane uses unique container name and hostname; no Docker name collisions occur.
  - Each pane mounts its own agent state directory; no concurrent writes to the same host ~/.aider/.codex/.crush.
  - Shared caches (npm, pip, cargo, ccache, go) remain enabled and shared.
  - On setup failure, prior worktrees are rolled back and the exit code is non-zero.
  - Worktrees persist after successful runs for inspection and merging.

Open Questions / Future Enhancements
- fork list / fork clean commands to enumerate and remove sessions and their worktrees.
- Layout presets and custom arrangements (--fork-layout).
- Optional stacking of forks onto a transient snapshot commit vs stash-apply (controlled by flags).
- Optional reuse of an existing tmux window instead of creating a new session.
- Additional per-agent state isolation (if new agents/tools are added later).

End of Specification
