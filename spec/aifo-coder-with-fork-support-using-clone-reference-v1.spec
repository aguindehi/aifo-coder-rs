AIFO-Coder Fork Support (Clone+Reference) v1 Specification

Status
- Version: 1
- Date: 2025-09-01
- Owner: AIFO Coder maintainers
- State: Draft (ready for implementation)

Overview
This specification replaces the worktree-based fork orchestration from the “workdir v2” plan with a clone-based approach that satisfies Aider’s requirement for a real .git directory. Instead of git worktrees, each fork pane uses an isolated Git clone created with git clone --reference-if-able to share objects with the base repo while maintaining its own .git directory. An optional --dissociate flag allows copying object data for independence when desired (especially with snapshot commits). All other v2 goals remain: per-pane isolation, unique container names/hostnames, robust rollback, and safe argument handling.

Why switch from worktrees to clone --reference
- Aider requires a full .git directory; worktrees place a .git file pointing elsewhere, which breaks Aider behavior.
- git clone --reference-if-able shares objects without hard-linking the index or worktree state, providing efficient, isolated clones.
- Optional --dissociate ensures clones retain their own objects after creation (useful when including uncommitted changes via a snapshot commit).

Motivation
- Preserve all v2 concurrency/safety goals while making each pane a real repository.
- Avoid .git/index lock contention and cross-pane interference.
- Provide a robust path for including dirty changes via a snapshot commit strategy, with clear rollback semantics.

Goals
- --fork <N> starts N >= 2 concurrent agents in tmux panes.
- Each pane:
  - Operates in its own clone directory under .aifo-coder/forks/<sid>/pane-<i>.
  - Uses a unique fork branch fork/<base-label>/<sid>-<i>.
  - Has a full .git directory (not a worktree).
  - Uses unique container name and hostname (no Docker name collisions).
  - Mounts per-pane agent state directories to avoid races in ~/.aider/.codex/.crush.
- Tmux session orchestration with tiled layout (default) or other presets.
- Robust preflight checks and rollback on failures.
- Shared content-addressed caches (npm, pip, cargo, ccache, go) remain enabled to maximize performance.

Non-Goals
- Automatic merging/rebasing of fork branches back to the base.
- Advanced tmux layouts beyond a small set of presets.
- Reusing an existing tmux window (a new session/window is created each time).

User Experience

CLI
- --fork <N>
  - Enables fork mode when N >= 2; N < 2 behaves as a normal single-agent run.
  - Applies to any subcommand (codex, crush, aider, toolchain, etc.).
  - Example: aifo-coder --fork 4 aider -- --model my-model --verbose
- Optional flags:
  - --fork-include-dirty
    - Include current uncommitted changes by basing clones on a transient snapshot commit (see policy).
  - --fork-dissociate
    - After cloning with --reference-if-able, also pass --dissociate to copy objects locally for independence.
  - --fork-session-name <name>
    - Set tmux session name explicitly; default is aifo-<sid>.
  - --fork-layout <tiled|even-h|even-v>
    - Choose pane layout; default is tiled.

User Flow

1) Preconditions
- tmux must be installed and in PATH. If missing: error and exit 127.
- Must be inside a Git repository (git rev-parse --show-toplevel). If not: error and exit 1.
- Docker recommended for agents; the parent orchestrator itself does not require Docker.

2) Discover base
- repo root: git rev-parse --show-toplevel
- base branch: git rev-parse --abbrev-ref HEAD
  - If detached (“HEAD”), record commit SHA via git rev-parse --verify HEAD and label base as “detached”.

3) Session id
- Create a short unique session id from time ⊕ pid (base36). Used for directory/branch uniqueness and diagnostics.

4) Clone setup (replaces worktrees)
- Create directories: <repo-root>/.aifo-coder/forks/<sid>/pane-1..N
- Use absolute, canonical path of <repo-root> to avoid alternate storage surprises.
- For pane i (1..N):
  - Pane dir: <repo-root>/.aifo-coder/forks/<sid>/pane-<i>
  - Clone:
    - git clone --no-checkout --reference-if-able <repo-root> <pane-dir>
    - If --fork-dissociate is set: add --dissociate
  - Optional: set origin in the clone to base’s push URL (if available):
    - base_origin=$(git -C <repo-root> remote get-url --push origin)
    - if present: git -C <pane-dir> remote set-url origin "$base_origin"
  - Branch: fork/<base-label>/<sid>-<i>
  - Checkout base and create fork branch:
    - Detached base: git -C <pane-dir> checkout -b <branch> <sha>
    - Normal base:   git -C <pane-dir> checkout -b <branch> <base-branch>
- If any clone/checkout fails, perform best-effort rollback:
  - rm -rf <created-pane-dirs>
  - Exit non-zero.

5) Build child args (minus --fork)
- Construct child arguments from parsed CLI; remove --fork (both --fork N and --fork=N) at the top-level only.
- Preserve everything after the subcommand’s “--”.

6) Uncommitted Changes Policy (clone-friendly)
- Default: proceed even if the main workspace is dirty; clones start from base HEAD (or detached SHA). Print a clear warning.
- With --fork-include-dirty: use a “snapshot commit” strategy:
  - In base repo:
    - Record current branch/SHA.
    - Create and checkout a temporary branch: aifo-snapshot/<sid> at <base-ref>.
    - git add -A && git commit -m "aifo-fork snapshot <sid>"
  - Clone panes from the base repo as usual; for checkout, use aifo-snapshot/<sid> instead of <base-branch>/<sha>.
  - If --fork-dissociate is set: clones become independent; after cloning, switch base back to the original branch; keep the snapshot branch by default (future cleanup).
  - If --fork-dissociate is NOT set: clones depend on base objects; warn the user not to prune base objects until done.
- Rationale: stash/apply across clones is fragile; snapshot commit is deterministic and robust with clones.

7) Per-pane environment isolation
- For each pane i:
  - AIFO_CODER_SKIP_LOCK=1
  - AIFO_CODER_CONTAINER_NAME=aifo-coder-<agent>-<sid>-<i>
  - AIFO_CODER_HOSTNAME=aifo-coder-<agent>-<sid>-<i>
  - AIFO_CODER_FORK_SESSION=<sid>
  - AIFO_CODER_FORK_INDEX=<i>
  - AIFO_CODER_FORK_STATE_DIR=<host-state-base>/<sid>/pane-<i>
    - host-state-base defaults to ~/.aifo-coder/state
    - Implementation must mount per-pane state directories:
      - <state>/.aider -> /home/coder/.aider
      - <state>/.codex -> /home/coder/.codex
      - <state>/.crush -> /home/coder/.crush
    - Rationale: avoid concurrent writes to shared host ~/.aider/.codex/.crush.
- Shared content-addressed caches remain shared and global:
  - npm, pip, cargo, ccache, go named volumes are safe to share and improve performance.

8) tmux orchestration
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
- If tmux session startup fails, attempt to rollback created clones (best effort), then exit non-zero.

9) Agent concurrency
- Each pane runs in its clone directory; agent mounts that as /workspace.
- Each pane’s agent uses unique container name/hostname via env vars (no collisions).
- Each pane’s agent uses its own proxy session id and network (created by each child run).
- Shared named caches (npm/pip/cargo/ccache/go) remain shared.

10) Cleanup
- Clones remain after the session for review and merging.
- A future helper will provide listing and cleanup (fork list/clean).
- On setup errors (clone failure or tmux orchestration), rollback is attempted.

Concurrency and Safety

Workspace isolation
- Each agent operates in its own clone directory -> no .git/index lock contention, no overlapping temp files, no editor/backups collision.

Agent state isolation
- Per-pane state directories on the host are mounted for .aider/.codex/.crush:
  - Host: ~/.aifo-coder/state/<sid>/pane-<i>/{.aider,.codex,.crush}
  - Container: /home/coder/{.aider,.codex,.crush}
- Prevents concurrent writes to shared host ~/.aider/.codex/.crush.

Container isolation
- AIFO_CODER_CONTAINER_NAME and AIFO_CODER_HOSTNAME are set uniquely per pane to avoid name collisions.
- Each agent run creates its own network/session id; no port/name collisions for proxies or sidecars.

Caches
- Named Docker volumes (cargo, npm, pip, ccache, go) remain shared and are safe (content-addressed).

Global lock
- Parent process operates normally (no agent run).
- Child panes set AIFO_CODER_SKIP_LOCK=1 so single-run lock is bypassed intentionally.
- Normal (non-fork) runs continue to enforce one-at-a-time execution.

Security
- Pane commands constructed via shell_join with conservative POSIX escaping.
- Fork branches and clones avoid shared mutable state across panes.
- No elevated privileges beyond standard Docker usage.

Performance
- clone --reference-if-able shares objects efficiently (similar space/time benefits to worktrees).
- Shared caches accelerate cold starts across panes.
- Each pane uses separate containers; resource usage scales with N.

Implementation Plan (High-Level)

CLI Additions (src/main.rs)
- Extend Cli:
  - #[arg(long)] fork: Option<usize>
  - #[arg(long)] fork_include_dirty: bool
  - #[arg(long)] fork_dissociate: bool
  - #[arg(long)] fork_session_name: Option<String>
  - #[arg(long)] fork_layout: Option<String> with validation among {tiled, even-h, even-v}
- Behavior:
  - If Some(n) and n >= 2 -> enter fork orchestrator path early and return ExitCode from there (parent does not run an agent).

Skip-Lock Mechanism (src/main.rs)
- Before acquire_lock(): if env AIFO_CODER_SKIP_LOCK == "1", skip acquiring the lock.

Fork Orchestrator (src/main.rs)
- fork_run(n, &Cli) -> ExitCode:
  1) Preflight:
     - which::which("tmux"); error 127 if missing.
     - git rev-parse --show-toplevel; error if not inside repo.
  2) Identify base:
     - base branch with git rev-parse --abbrev-ref HEAD.
     - if “HEAD”, read commit sha with git rev-parse --verify HEAD; set base label “detached”.
  3) Session id creation (time ⊕ pid, base36). Compute session name from CLI or default aifo-<sid>.
  4) Create forks base dir: <root>/.aifo-coder/forks/<sid>.
  5) Optional dirty include:
     - If --fork-include-dirty: create snapshot branch aifo-snapshot/<sid> at base ref; git add -A && git commit -m "aifo-fork snapshot <sid>".
     - Use this snapshot ref for pane branch creation instead of base ref.
  6) For i in 1..=n:
     - Compute pane dir and ensure parent directories exist.
     - git clone --no-checkout --reference-if-able [--dissociate if flag set] <repo-root> <pane-dir>.
     - Optional: set clone origin to base’s push URL if present.
     - Determine fork branch name: fork/<base|detached>/<sid>-<i>; checkout -b <branch> <base-ref-or-snapshot-ref>.
     - On any failure -> delete created pane dirs and exit 1.
  7) Build child args:
     - Rebuild from parsed Cli (not argv scanning), dropping --fork and its value (both --fork N and --fork=N).
     - Preserve agent subcommand and tail args after “--”.
  8) Build per-pane env:
     - Always set AIFO_CODER_SKIP_LOCK=1.
     - AIFO_CODER_CONTAINER_NAME / AIFO_CODER_HOSTNAME unique per pane.
     - AIFO_CODER_FORK_SESSION=<sid>, AIFO_CODER_FORK_INDEX=<i>.
     - AIFO_CODER_FORK_STATE_DIR=~/.aifo-coder/state/<sid>/pane-<i>.
  9) tmux orchestration:
     - tmux new-session -d -s <session> -n aifo-fork -c <pane1-path> '<envs> aifo-coder <args>'
     - For panes 2..N: tmux split-window -t <session>:0 -c <paneX-path> '<envs> aifo-coder <args>'
     - tmux select-layout tiled (or per flag)
     - tmux set-window-option -t <session>:0 synchronize-panes off
     - Attach or switch to the new session.
     - If tmux fails, attempt rollback of clones and exit non-zero.
  10) Print summary and return ExitCode::from(0).

Agent Launcher Changes (src/lib.rs build_docker_cmd)
- When AIFO_CODER_FORK_STATE_DIR is set:
  - Replace host mounts for ~/.aider, ~/.codex, ~/.crush with:
    - <dir>/.aider  -> /home/coder/.aider
    - <dir>/.codex  -> /home/coder/.codex
    - <dir>/.crush  -> /home/coder/.crush
  - Ensure directories exist (create on host).
- Container naming:
  - Honor AIFO_CODER_CONTAINER_NAME and AIFO_CODER_HOSTNAME if set (already supported).

Diagnostics
- On starting fork mode, print a summary line:
  - “aifo-coder: fork session <sid> on base <base-label>; created N clones under .aifo-coder/forks/<sid>”
  - If dirty and not including dirty: print warning.

Environment Variables
- AIFO_CODER_SKIP_LOCK=1 (child panes)
- AIFO_CODER_FORK_SESSION / AIFO_CODER_FORK_INDEX (for diagnostics/telemetry)
- AIFO_CODER_FORK_STATE_DIR (host path for per-pane agent state)
- AIFO_CODER_CONTAINER_NAME / AIFO_CODER_HOSTNAME (unique per pane)
- TMUX presence determines attach vs switch

Testing Strategy

Unit
- Argument stripping: ensure both --fork N and --fork=N are removed; ensure tokens after “--” are unchanged.
- Branch name generation and detached HEAD handling.
- Construction of per-pane env vars and derived container names.
- build_docker_cmd honors AIFO_CODER_FORK_STATE_DIR mounts.
- Skip-lock behavior with AIFO_CODER_SKIP_LOCK=1.

E2E (manual or CI with tmux)
- Verify creation of N clones and branches under .aifo-coder/forks/<sid>.
- Verify tmux session with N panes, each running in its own clone directory.
- Verify unique container name and hostname per pane.
- Verify per-pane agent state directories on host are created and mounted.
- With --fork-include-dirty + --fork-dissociate: verify snapshot content visible in clones.

Negative
- Missing tmux: exit 127.
- Outside Git repo: exit non-zero with clear error.
- Clone creation failure mid-way: prior clones removed; exit non-zero.
- tmux session start failure: clones removed; exit non-zero.

Acceptance Criteria
- For N >= 2:
  - Creates N clones under .aifo-coder/forks/<sid>/pane-1..N.
  - In each clone, creates branch fork/<base|detached>/<sid>-<i> from the correct base (or snapshot when enabled).
  - Starts a new tmux session with N panes; each runs aifo-coder in its own clone directory.
  - Sets AIFO_CODER_SKIP_LOCK=1 for child panes; normal runs still acquire a global lock.
  - Each pane uses unique container name and hostname; no Docker name collisions occur.
  - Each pane mounts its own agent state directory; no concurrent writes to the same host ~/.aider/.codex/.crush.
  - Shared caches (npm, pip, cargo, ccache, go) remain enabled and shared.
  - On setup failure, prior clones are rolled back and the exit code is non-zero.
  - Clones persist after successful runs for inspection and merging.

Open Questions / Future Enhancements
- fork list / fork clean commands to enumerate and remove sessions and their clones.
- Layout presets and custom arrangements (--fork-layout).
- Optional commit signing management within clones (consistent policies with agent config).
- Snapshot branch lifecycle helpers (e.g., delete or archive) and dissociation defaults.
- Enabling optional reuse of an existing tmux window/session.
