AIFO-Coder Fork Support v1 Specification

Status
- Version: 1
- Date: 2025-08-31
- Owner: AIFO Coder maintainers
- State: Draft (ready for implementation)

Overview
The --fork feature enables users to start multiple, concurrent instances of aifo-coder agents (Codex, Crush, Aider) in one terminal session by leveraging tmux, the terminal multiplexer. Each concurrent agent runs in its own tmux pane and operates on an isolated Git worktree (branch) derived from the current repository, avoiding file and .git lock contention while sharing the repository’s object store and shared tool caches.

Example
- aifo-coder --fork 4 aider -- <args>
  - Creates 4 Git worktrees under <repo-root>/.aifo-coder/forks/<session-id>/pane-1..4
  - Each worktree checks out a unique branch fork/<base-branch>/<session-id>-<i>
  - Spawns a tmux session with 4 panes, each pane runs aifo-coder aider -- <args> in its own worktree directory
  - Each agent runs in its own container(s) and toolexec proxy, concurrently and safely

Motivation
- Single-workspace concurrency is unsafe: simultaneous edits and Git operations across multiple agents cause conflicts and corruption (.git/index.lock, temp files, editor backups, agent metadata).
- Multiple independent clones are safe but heavy and slow (download, disk usage).
- Git worktrees provide the best trade-off: safety, speed, space efficiency, and straightforward merging back into the base branch.

Goals
- Provide an ergonomic and safe way to run N concurrent agents with a single command.
- Avoid race conditions across agents by isolating each into its own worktree and branch.
- Use tmux to provide a unified terminal UI with multiple panes.
- Preserve default single-run protections (global lock) while allowing intentional concurrency in fork mode.
- Require minimal host prerequisites beyond what users already have.

Non-Goals
- Automated merging/rebasing of fork branches back into the base branch (user will perform Git merges/PRs).
- Reuse of existing tmux panes/window; v1 always creates a new session/window.
- Advanced tmux pane layout customization; v1 uses a tiled layout.
- Automatically stashing/porting uncommitted changes into fork worktrees.

User Experience

CLI
- New flag: --fork <N>
  - N >= 2 to enable forking. If N < 2, it behaves as a normal single-agent run.
  - Applies to all agents (codex, crush, aider).
  - Example: aifo-coder --fork 4 aider -- --model my-model --verbose

User Flow
1) Preconditions checked:
   - tmux must be installed (in PATH). If missing: error and exit 127.
   - Must be inside a Git repository (git rev-parse --show-toplevel succeeds). If not: error and exit 1.
   - Optional policy: recommend (or require) a clean working tree for reproducibility; see “Uncommitted changes policy.”

2) Worktree setup:
   - Determine repo root and current base branch (git rev-parse --abbrev-ref HEAD). If detached HEAD, use “detached” as placeholder.
   - Generate a session id: a short hex-like token combining time and pid.
   - Create worktree directories: <repo-root>/.aifo-coder/forks/<session-id>/pane-1..N
   - For each pane i (1..N), create a new branch:
     branch = fork/<base-branch>/<session-id>-<i>
     Run: git worktree add -b <branch> <path> <base-branch>
   - Note: Worktrees share the object database for speed and space savings.

3) tmux session orchestration:
   - Create a new session (detached), window named aifo-fork.
   - First pane: tmux new-session -d -s <session> -n aifo-fork -c <pane1-path> 'AIFO_CODER_SKIP_LOCK=1 aifo-coder <original-args-without-fork>'
   - Additional panes: tmux split-window -t <session>:0 -c <paneX-path> 'AIFO_CODER_SKIP_LOCK=1 aifo-coder <args>'
   - Apply tiled layout: tmux select-layout -t <session>:0 tiled
   - Disable synchronized input: tmux set-window-option -t <session>:0 synchronize-panes off
   - If already inside tmux (TMUX env set), switch-client to the new session; otherwise, attach-session.

4) Agent concurrency:
   - Each pane runs aifo-coder in its own working directory (worktree), which aifo-coder then mounts into its own container as /workspace.
   - Each child invocation starts its own sidecars and toolexec proxy. Session identifiers ensure no container/network/proxy collisions.
   - Shared Docker named caches (cargo, npm, pip, ccache, go) remain shared across agents to improve performance; they are safe to share as content-addressed caches.

5) Cleanup:
   - Worktrees are left in place intentionally so users can review and merge.
   - A future command may provide listing and cleanup helpers.

Uncommitted Changes Policy
- Default recommendation: Users should commit or stash changes before forking.
- Rationale: git worktree add -b <branch> <path> <base_branch> checks out from base_branch HEAD. Uncommitted changes in the original workspace don’t propagate to forks by default, which avoids silent divergence.
- v1 behavior:
  - Proceed even if the main workspace is dirty; fork branches start from base branch HEAD.
  - Print a clear warning if the main working tree is dirty, suggesting commit/stash to include those changes into forks.
- Future option:
  - A --fork-include-dirty or --fork-stash flag could stash and apply the stash into each fork branch during creation.

Git Branch and Directory Layout
- Branch names: fork/<base-branch>/<session-id>-<i>
  - Examples:
    - fork/main/abc123-1
    - fork/feature-x/abc123-2
- Worktrees:
  - <repo-root>/.aifo-coder/forks/<session-id>/pane-1
  - <repo-root>/.aifo-coder/forks/<session-id>/pane-2
  - ...
- Merge strategy:
  - Users can merge each fork/* branch back into the base branch via standard Git merges or PRs.
  - Users may rebase fork branches over time to keep them up-to-date.

System Requirements
- tmux: must be installed and in PATH.
- git: must be installed and the current directory must be within a Git repository.
- Docker: as required by aifo-coder; unchanged by this feature.

Failure Modes and Messages
- tmux not found: “Error: --fork requires 'tmux' but it was not found in PATH.” Exit 127.
- Not a Git repo: “Error: --fork requires running inside a Git repository to create worktrees safely.” Exit 1.
- Worktree creation failure: report the failing pane index and bail out. Best-effort rollback of already-created worktrees for that session.
- tmux session start failure: “Error: failed to start tmux session.” Exit 1.

Concurrency and Safety
- Workspace isolation: Each agent works in a distinct worktree directory, eliminating .git/index lock contention, temp-file collisions, and agent metadata conflicts.
- Container isolation: Each child run triggers its own sidecar session and proxy, using unique session/network names; no port or container-name conflicts.
- Caches: Named Docker volumes (cargo, npm, pip, ccache, go) are shared and safe to share; they improve performance and are designed for concurrency.
- Global lock: By default, aifo-coder takes a single-process lock to prevent accidental concurrent runs.
  - Fork mode intentionally bypasses this lock by setting AIFO_CODER_SKIP_LOCK=1 for child invocations.
  - Normal (non-fork) runs keep the locking behavior.

Implementation Plan (High-Level)

CLI Additions (src/main.rs)
- Extend Cli:
  - #[arg(long)]
    fork: Option<usize>,
- Behavior: If Some(n) and n >= 2, enter fork orchestrator path early in main and return ExitCode after orchestrating tmux; do not run the single-agent flow in the parent process.

Skip-Lock Mechanism (src/main.rs)
- Around the existing acquire_lock() call, add an environment override:
  - Read AIFO_CODER_SKIP_LOCK; if "1", skip the lock and proceed.
  - In normal runs, keep existing lock behavior.
- Child invocations launched by tmux set AIFO_CODER_SKIP_LOCK=1 to allow intentional concurrency.

Fork Orchestrator (src/main.rs)
- Steps performed by fork_run(n, &Cli):
  1) Verify tmux availability via which::which("tmux").
  2) Verify we are inside a Git repo and identify:
     - repo root: git rev-parse --show-toplevel
     - base branch: git rev-parse --abbrev-ref HEAD (fallback “detached”)
     - optionally detect dirty state (git diff --quiet; git diff --cached --quiet) to warn user.
  3) Create session id (time xor pid).
  4) Create worktree directories under .aifo-coder/forks/<sid>/pane-i and branches fork/<base>/<sid>-<i> with:
     git worktree add -b <branch> <path> <base_branch>
     On failure, rollback prior worktrees and exit non-zero.
  5) Build child argument vector as the original CLI args minus the --fork flag and its value.
  6) Build command string for tmux panes:
     "AIFO_CODER_SKIP_LOCK=1 aifo-coder <args...>"
     Use existing conservative shell joiner (lib.rs shell_join) to quote safely.
  7) tmux orchestration:
     - new-session (detached) for the first pane with -c <pane1-path> and command string
     - for remaining panes: split-window with -c <paneX-path> and command string
     - select-layout tiled
     - set-window-option synchronize-panes off
     - attach or switch to the session
  8) Return ExitCode::from(0) to the OS.

Environment Variables
- AIFO_CODER_SKIP_LOCK=1 set for child panes to bypass top-level lock.
- TMUX presence is used to detect whether to attach or switch to the new session.

Interactions With Toolchain/Proxy (existing code)
- Each pane’s aifo-coder invocation runs in a different current working directory (the worktree path), thus:
  - Docker mount: host worktree path is mounted to /workspace (no cross-pane collisions).
  - toolexec proxy: each run has its own session id and network; no collisions.
  - Toolchain cache volumes remain shared; safe and beneficial.

Security Considerations
- The tmux command strings are constructed using conservative shell escaping via shell_join, minimizing injection risks.
- Worktrees avoid sharing mutable workspace state across panes.
- The lock override is opt-in and only used for fork; normal runs remain serialized to minimize risk of unintended concurrent execution.

Performance Considerations
- Worktrees are fast to create; they share the object store with the main repo.
- Shared caches speed up cold-starts across concurrent runs.
- Each pane runs separate containers; resource usage scales with N; users should pick N according to host capacity.

Open Questions / Future Enhancements
- Optional flags:
  - --fork-include-dirty / --fork-stash to include local changes in forks.
  - --fork-session-name to set tmux session name.
  - --fork-layout to choose layout (tiled, even-horizontal, even-vertical).
  - --fork-clean to remove a session’s worktrees.
- Allow running arbitrary subcommands with --fork, not just agents (already supported implicitly).
- Integrate a status/merge helper to guide users merging fork branches.

Acceptance Criteria
- TMUX requirement: If tmux absent, aifo-coder reports a clear error and exits 127.
- Git requirement: If outside of a Git repo, aifo-coder reports a clear error and exits non-zero.
- Worktrees: For N >= 2, creates N worktrees and branches as specified under .aifo-coder/forks/<session-id>.
- tmux orchestration: Creates a new tmux session with N panes, each starting aifo-coder in the corresponding worktree directory.
- Locking: Child panes bypass the single-run lock; normal runs still enforce it.
- Concurrency: Multiple agents run concurrently without workspace/file contention; shared caches function normally.
- Persistence: Worktrees and branches remain after the session for user inspection and merging.

Testing Strategy (Outline)
- Unit-test argument stripping of --fork from child args (pure function).
- E2E (manual or CI with tmux available):
  - Verify session creation and pane commands.
  - Verify per-pane worktree directories and branches.
  - Run simple agents in each pane; confirm distinct /workspace mounts (by logging PWD, branch).
- Negative tests:
  - Without tmux: expect error 127.
  - Outside Git repo: expect error status and message.
  - Worktree creation failure (simulate): expect rollback of previously created worktrees.

Implementation Notes
- Use which::which for tmux detection (already in dependencies).
- Use aifo_coder::shell_join to build pane command safely.
- Ensure we call tmux with -c to set the starting directory per pane.
- When constructing the child args:
  - Drop both “--fork N” (two tokens) and “--fork=N” forms.
- Use ExitCode consistent with existing conventions (127 for missing binaries, 1 for generic failures).

End of Specification
