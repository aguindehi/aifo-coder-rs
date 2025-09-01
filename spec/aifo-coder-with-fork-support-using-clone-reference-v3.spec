AIFO-Coder Fork Support (Clone+Reference) v3 Specification

Status
- Version: 3
- Date: 2025-09-01
- Owner: AIFO Coder maintainers
- State: Ready for implementation

Scope and Platforms
- Supported host platforms:
  - Linux (native)
  - macOS (Docker Desktop or Colima)
  - Windows:
    - WSL: treated as Linux (preferred)
    - Native Windows: Windows Terminal (wt.exe) when available; otherwise PowerShell windows; Git Bash (“Git Shell” / mintty) also supported
- Container runtime: Docker CLI available in PATH on all platforms.
- Git: Git CLI available in PATH on all platforms (required; Git Shell available on Windows).

Overview
This v3 specification consolidates and expands the fork orchestration via clone --reference-if-able with optional --dissociate, adds repository-scoped locking (including Windows Git Shell), per-pane state mounts, a non-destructive snapshot strategy using a temporary index and commit-tree, cross-platform orchestrators (tmux/wt/PowerShell/Git Bash), comprehensive user-facing diagnostics and recovery guidance, fork maintenance commands (list/clean), and automatic pruning notices for stale sessions. Clones are created under <repo-root>/.aifo-coder/forks/<sid>/pane-1..N and persist for inspection and merging unless explicitly cleaned.

Key Improvements vs v2
- Non-destructive snapshot includes dirty working tree using a temporary index (GIT_INDEX_FILE) and git commit-tree; does not modify user index/HEAD, avoids hooks/signing and user.name/email.
- Repository-scoped locking to allow concurrent runs across different repositories on all platforms (including Windows Git Shell). Skip-lock honored for child panes.
- Cross-platform orchestration including Git Bash fallback on Windows, with quoting/escaping guidance.
- Per-pane state isolation via AIFO_CODER_FORK_STATE_DIR respected by build_docker_cmd (mounts .aider/.codex/.crush per pane).
- Comprehensive user transparency: prints exact clone paths, branches, snapshot SHA (when used), per-pane environment, and recovery tips; optional keep-on-failure behavior.
- Fork maintenance commands (list/clean) with JSON output, dry-run, safety checks, and pruning notices; optional automatic pruning via env flag.
- Signing policy mirrors the original repository; launcher does not persistently change signing in clones.

CLI Additions (src/main.rs)
- Extend Cli with:
  - #[arg(long)] fork: Option<usize>                   // N >= 2 enables fork mode; N < 2 => normal run
  - #[arg(long)] fork_include_dirty: bool              // include uncommitted changes via snapshot commit
  - #[arg(long)] fork_dissociate: bool                 // clone with --dissociate for clone independence
  - #[arg(long)] fork_session_name: Option<String>     // session/window name override; default aifo-<sid>
  - #[arg(long, value_parser = validate_layout)] fork_layout: Option<String>  // one of {tiled, even-h, even-v}
  - #[arg(long)] fork_keep_on_failure: bool            // keep created clones on orchestration failure (default: keep; see Rollback)
- Maintenance subcommands:
  - aifo-coder fork list [--json] [--all-repos]
  - aifo-coder fork clean [--session <sid> | --older-than <days> | --all] [--dry-run] [--yes]
- Behavior:
  - If Some(n) and n >= 2: enter orchestrator early; parent does not start an agent; return ExitCode from orchestrator.
  - If N < 2: ignore fork flags; run normally.
  - Maintenance subcommands operate without starting agents or acquiring the agent lock.

Locking Model (Repository-Scoped)
- Goal: Allow concurrent aifo-coder runs when they operate in different Git repositories, including on Windows Git Shell.
- If inside a Git repository (git rev-parse --show-toplevel succeeds):
  - repo_root = canonical absolute top-level path.
  - Preferred lock path: <repo_root>/.aifo-coder.lock (if writable).
  - Secondary lock path: <xdg_runtime>/aifo-coder.<hash(repo_root)>.lock where:
    - On Windows: normalize repo_root by:
      - Converting drive letter to uppercase
      - Case-folding the full path
      - Normalizing separators to backslashes
    - Hash function: stable (e.g., SHA-1 or FNV) over the normalized absolute path string; hex or base36 id.
  - Acquire the first available lock in the ordered set (prefer in-repo; fall back to runtime hashed path).
- If not inside a Git repository:
  - Fall back to legacy ordered candidates: HOME/XDG_RUNTIME_DIR/tmp/CWD.
- Skip-Lock:
  - If env AIFO_CODER_SKIP_LOCK == "1", skip acquiring any lock (child panes of fork mode).
- Git Shell / Git Bash on Windows:
  - Lock paths computed via Rust std::fs on canonical absolute Windows paths; MSYS /c/ rewriting is irrelevant.

Session and Branch Naming
- Session id: short base36 id derived from time ⊕ pid (reuse create_session_id()).
- Session name: from --fork-session-name or default aifo-<sid>.
- Branch naming: fork/<base-label>/<sid>-<i>
  - base-label: current branch sanitized to a valid ref path component; if detached, use “detached”.
  - i is 1-based pane index.
- Sanitization rules:
  - Lowercase; replace spaces and invalid chars with '-'; trim to safe length; collapse repeats; strip leading/trailing '/','-','.'.

Directories and Paths
- Root repo: git rev-parse --show-toplevel (absolute canonical path).
- Forks base dir: <repo-root>/.aifo-coder/forks/<sid>.
- Pane directories: <repo-root>/.aifo-coder/forks/<sid>/pane-<i> (i = 1..N).
- Per-pane agent state dirs (host): <state-base>/<sid>/pane-<i>/{.aider,.codex,.crush}
  - state-base defaults to ~/.aifo-coder/state; override via AIFO_CODER_FORK_STATE_BASE.
  - Parent orchestrator ensures these directories exist before starting panes.

Snapshot Strategy (Include Dirty)
- Objective: include staged + unstaged changes without altering user index or working tree and without requiring user.name/email or signing.
- Steps in base repo:
  1) Determine base_ref or base_sha:
     - If on branch: base_ref = current branch name
     - If detached: base_sha = git rev-parse --verify HEAD
  2) Create a temporary index file tmp_index (e.g., under OS temp or .git):
     - Export GIT_INDEX_FILE=tmp_index for the following commands only.
  3) Index current working tree:
     - git add -A
  4) tree = git write-tree
  5) parents:
     - If HEAD exists: parent = git rev-parse --verify HEAD; snap = printf "aifo-fork snapshot <sid>\n" | git commit-tree "$tree" -p "$parent"
     - If no commits yet: snap = printf "aifo-fork snapshot <sid>\n" | git commit-tree "$tree"
  6) Remove tmp_index best-effort.
  7) Use snap (SHA) as the checkout base in clones when --fork-include-dirty is set; otherwise use base_ref/base_sha.
- Properties:
  - Does not change HEAD or current branch.
  - Does not invoke commit hooks or require signing.
  - Works without configured user.name/user.email.
- Failure handling:
  - If any step fails, abort snapshot and fall back to base_ref/base_sha; print a warning explaining that dirty changes are not included.

Clone Setup
- For each pane i in 1..=N:
  - git clone --no-checkout --reference-if-able <repo-root> <pane-dir>
  - If --fork-dissociate: include --dissociate
  - Optional: set origin push URL to match base repo (non-fatal):
    - base_origin=$(git -C <repo-root> remote get-url --push origin)
    - if present: git -C <pane-dir> remote set-url origin "$base_origin"
  - Determine checkout base:
    - If include-dirty: base = snap (SHA)
    - Else if detached: base = base_sha
    - Else: base = base_ref (branch)
  - branch = fork/<base-label>/<sid>-<i>
  - git -C <pane-dir> checkout -b <branch> <base>
  - Best-effort enhancements:
    - If <pane-dir>/.gitmodules exists: git -C <pane-dir> submodule update --init --recursive
    - If git lfs present and repo uses LFS:
      - git -C <pane-dir> lfs install
      - git -C <pane-dir> lfs fetch --all
      - git -C <pane-dir> lfs checkout
  - On failure:
    - If clone/checkout fails for pane i, record failure; remove that pane directory best-effort; continue rollback policy below.

Child Arguments and Flag Stripping
- Construct child arguments from parsed Cli (not by re-parsing argv):
  - Remove fork flags: --fork, --fork-include-dirty, --fork-dissociate, --fork-session-name, --fork-layout, --fork-keep-on-failure (spaced and equals forms).
  - Preserve subcommand and tail args (including everything after “--”).
- Each pane runs: aifo-coder <child-args> with per-pane env.

Per-Pane Environment Variables
- Each pane i sets:
  - AIFO_CODER_SKIP_LOCK=1
  - AIFO_CODER_CONTAINER_NAME=aifo-coder-<agent>-<sid>-<i>
  - AIFO_CODER_HOSTNAME=aifo-coder-<agent>-<sid>-<i>
  - AIFO_CODER_FORK_SESSION=<sid>
  - AIFO_CODER_FORK_INDEX=<i>
  - AIFO_CODER_FORK_STATE_DIR=<state-base>/<sid>/pane-<i>  (state-base default ~/.aifo-coder/state; override AIFO_CODER_FORK_STATE_BASE)
- Parent orchestrator creates <dir>/.aider, <dir>/.codex, <dir>/.crush before pane start.

build_docker_cmd Changes (src/lib.rs)
- When AIFO_CODER_FORK_STATE_DIR is set:
  - Do not mount HOME-based ~/.aider, ~/.codex, ~/.crush.
  - Instead mount:
    - <dir>/.aider -> /home/coder/.aider
    - <dir>/.codex -> /home/coder/.codex
    - <dir>/.crush -> /home/coder/.crush
  - Ensure these directories exist on the host (create if missing).
- Keep existing mounts:
  - ~/.gnupg -> /home/coder/.gnupg-host:ro
  - ~/.gitconfig (read/write)
  - Timezone files if present
- Container naming already honors AIFO_CODER_CONTAINER_NAME/HOSTNAME.

Orchestration (Cross-Platform)
- Linux/macOS/WSL (tmux):
  - Preflight:
    - which tmux; if missing -> exit 127 with clear error
    - which git; if missing -> exit non-zero
    - Must be inside a Git repo; else exit non-zero
  - Session:
    - tmux new-session -d -s <session> -n aifo-fork -c <pane1-path> '<envs> aifo-coder <child-args>'
    - For panes 2..N: tmux split-window -t <session>:0 -c <paneX-path> '<envs> aifo-coder <child-args>'
    - tmux select-layout -t <session>:0 <layout> (default tiled)
    - tmux set-window-option -t <session>:0 synchronize-panes off
    - Attach:
      - If TMUX present: tmux switch-client -t <session>
      - Else: tmux attach-session -t <session>
  - Quoting:
    - Build a single shell string with conservative POSIX escaping (shell_join) and run via sh -lc '...'.
    - Use tmux -c <dir> to set each pane’s working directory.

- Windows Native:
  - Preferred: Windows Terminal (wt.exe):
    - Preflight:
      - Detect wt.exe in PATH or WindowsApps; if present, use it.
      - Git is required; Git Shell is available.
    - Orchestration:
      - Create new window/tab; split panes via wt split-pane -H/-V.
      - Each pane runs PowerShell with per-process env (avoid setx):
        PowerShell -NoExit -Command "$env:AIFO_CODER_SKIP_LOCK='1'; ...; Set-Location '<pane-dir>'; aifo-coder <child-args>"
      - Also pass -d <pane-dir> to wt for working directory.
      - Layout approximated via split sequence to match tiled or even layouts.
  - Fallback: PowerShell windows
    - For each pane i:
      Start-Process -WindowStyle Normal -WorkingDirectory <pane-dir> powershell -ArgumentList "-NoExit","-Command","$env:AIFO_CODER_SKIP_LOCK='1'; ...; aifo-coder <child-args>"
    - Print started PIDs and pane paths.
  - Alternative: Git Bash (Git Shell / mintty)
    - Detect git-bash.exe or mintty.exe; use if wt.exe missing or when explicitly requested.
    - Command: git-bash.exe -c "cd '<pane-dir>' && export VAR=... && aifo-coder <child-args>; exec bash"
    - Multiple windows; no panes/layout.
  - WSL detection:
    - If running under WSL (WSL_DISTRO_NAME), prefer tmux Linux path.

Rollback, Failure Handling, and Recovery
- Principles:
  - Clones should be recoverable by the user if orchestration fails after clone creation.
  - Do not delete successfully created pane directories on orchestration failure by default.
- Policy:
  - If a particular pane fails during clone/checkout, remove that pane directory to avoid leaving a broken checkout.
  - If the orchestration (tmux/wt/etc.) fails after one or more clones were created:
    - By default, keep all successfully created pane directories for user recovery.
    - If --fork-keep-on-failure=false is explicitly set, remove all created panes (best-effort) and print a summary.
- Metadata:
  - Write <repo-root>/.aifo-coder/forks/<sid>/.meta.json with:
    { "created_at": epoch_secs, "base_label": "...", "base_ref_or_sha": "...", "snapshot_sha": "..." (optional), "panes": N, "pane_dirs": ["..."], "branches": ["..."], "layout": "..." }
  - On partial failures, record "panes_created": M and list existing pane_dirs/branches.
- Diagnostics on failure:
  - Print a summary with exact paths to surviving pane directories and branches.
  - Provide guidance for manual inspection and merging (see Post-Session Merging Guidance).
  - For tmux session failures, attempt tmux kill-session -t <session> (best-effort) and leave clones in place.

User Guidance, Transparency, and Messages
- On success:
  - Print:
    - "aifo-coder: fork session <sid> on base <base-label> (<base-ref/SHA>)"
    - "created N clones under <repo-root>/.aifo-coder/forks/<sid>"
    - Per-pane lines: "[i] <pane-dir> branch=<branch> container=<name> state=<state-dir>"
    - If snapshot used: "included dirty working tree via snapshot <snap-sha>"
    - If not using --fork-dissociate: "note: clones reference the base repo’s object store; avoid pruning base objects until done."
    - If working tree was dirty but --fork-include-dirty not set: warn changes are not included; suggest re-run with flag.
  - On attach/exit:
    - Linux/macOS/WSL: after tmux terminates, print merging guidance (see Post-Session Merging Guidance).
    - Windows (wt/PowerShell/Git Bash): print equivalent guidance after windows launch or on detection of orchestration completion/failure.
- On failure:
  - Clear message explaining stage of failure (snapshot/cloning/orchestration).
  - Paths to clones retained for recovery and the .meta.json location.
  - Example commands for inspection and merge (see below).
- Large-N warning:
  - If N > 8, warn about disk/memory and I/O impact.

Post-Session Merging Guidance (Suggested to User)
- Inspect changes:
  - git -C "<pane-1-dir>" status
  - git -C "<pane-1-dir>" log --oneline --decorate --graph -n 20
  - git -C "<root-repo>" log --oneline --decorate --graph -n 20
- Option A: Merge branch from clone into base repo:
  - git -C "<root-repo>" remote add "fork-<sid>-1" "<pane-1-dir>"    (or use file:// URL)
  - git -C "<root-repo>" fetch "fork-<sid>-1" "fork/<base>/<sid>-1"
  - git -C "<root-repo>" checkout "<base-branch>"    (if not detached)
  - git -C "<root-repo>" merge --no-ff "fork/<base>/<sid>-1"
- Option B: Cherry-pick a subset:
  - git -C "<root-repo>" cherry-pick <sha1> [<sha2> ...]  (after fetch)
- Option C: Rebase onto base:
  - git -C "<root-repo>" checkout -b "tmp/fork-<sid>-1" "fork/<base>/<sid>-1"
  - git -C "<root-repo>" rebase "<base-branch>"
- Option D: Patch-based transfer:
  - git -C "<pane-1-dir>" format-patch -o "<out-dir>" "<base-ref>"
  - git -C "<root-repo>" am "<out-dir>/*.patch"
- Option E: Push clone branch to a remote and open PR/MR (if configured).
- Cleanup after merge:
  - rm -rf "<repo-root>/.aifo-coder/forks/<sid>" or use: aifo-coder fork clean --session <sid>

Security
- Snapshot via temporary index + commit-tree avoids hooks/signing and does not alter user index/working tree/HEAD.
- Signing policy:
  - Clones inherit signing and other repo-level Git settings from the original (via git clone).
  - Launcher does not modify clone config; Aider per-process env (AIFO_CODER_GIT_SIGN=0) can disable signing temporarily without persisting.
- Per-pane state directories isolate concurrent writes to agent state.
- Containers do not use privileged mode or host sockets.

Performance
- clone --reference-if-able minimizes object duplication; optional --dissociate for independence at some cost.
- Shared caches (npm/pip/cargo/ccache/go) remain enabled and shared.
- Large repos and high N: warn users.

Environment Variables
- Child panes:
  - AIFO_CODER_SKIP_LOCK=1
  - AIFO_CODER_FORK_SESSION, AIFO_CODER_FORK_INDEX, AIFO_CODER_FORK_STATE_DIR
  - AIFO_CODER_CONTAINER_NAME / AIFO_CODER_HOSTNAME
- State base override:
  - AIFO_CODER_FORK_STATE_BASE to override default ~/.aifo-coder/state
- Pruning thresholds:
  - AIFO_CODER_FORK_STALE_DAYS (default 30)
  - AIFO_CODER_FORK_AUTOCLEAN=1 to auto-clean older-than threshold on startup in normal (non-fork) runs.

Fork Maintenance Commands
- fork list:
  - Lists sessions under <repo-root>/.aifo-coder/forks with:
    - sid, creation time (from dir mtime or .meta.json), pane count, base label, age; stale highlighting per threshold (default 14 days).
  - Flags:
    - --json for machine-readable output
    - --all-repos to scan upward or a configurable workspace root
- fork clean:
  - Deletes sessions and clone directories.
  - Modes:
    - --session <sid>: remove exactly that session
    - --older-than <days>: remove sessions older than N days
    - --all: remove all sessions
    - --dry-run: report plan without deleting
    - --yes: proceed without interactive confirmation
  - Safety:
    - On Unix, best-effort refusal if any pane dir appears to be the current working directory of a running process (via lsof/fuser); on Windows, skip this check.
  - Summary printed with counts and paths.

Automatic Pruning/Notifications
- On normal (non-fork) runs, the launcher scans <repo-root>/.aifo-coder/forks for old sessions:
  - If sessions older than AIFO_CODER_FORK_STALE_DAYS (default 30) exist, print:
    - "Found <n> old fork sessions (oldest <Xd>). Consider: aifo-coder fork clean --older-than <D>"
- Optional automatic pruning:
  - If AIFO_CODER_FORK_AUTOCLEAN=1 is set, apply --older-than threshold automatically at startup; print summary of deletions.

Testing Strategy
- Unit tests:
  - Argument stripping: ensure --fork flags are removed (spaced and equals forms); tail args after “--” unchanged.
  - Branch name generation and detached HEAD handling; sanitize/length limits.
  - build_docker_cmd honors AIFO_CODER_FORK_STATE_DIR mounts vs HOME equivalents.
  - Skip-lock behavior: when AIFO_CODER_SKIP_LOCK=1, skip acquiring locks.
  - Repository-scoped locking:
    - Different fake repo roots map to different lock keys.
    - Same repo root maps to the same key on case-insensitive Windows after normalization.
  - fork list JSON shape and metadata parsing.
- E2E (guarded by feature/env):
  - Linux/macOS/WSL (tmux): create N=2 clones, verify branches, verify tmux session attaches, panes use correct directories.
  - Windows:
    - wt.exe present: verify window launch command construction succeeds (best-effort).
    - Fallbacks: PowerShell and Git Bash commands built correctly; check quoting with spaces in paths.
- Negative:
  - Missing tmux on Linux/macOS: exit 127 with clear error.
  - Outside Git repo: exit non-zero with clear error.
  - Clone failure mid-way: prior failed pane dirs removed; others preserved for recovery; exit non-zero.
  - Session start failure: clones preserved (unless keep-on-failure=false); exit non-zero.

Acceptance Criteria
- For N >= 2:
  - Creates N clones under .aifo-coder/forks/<sid>/pane-1..N.
  - In each clone, creates branch fork/<base|detached>/<sid>-<i> based on:
    - base branch for clean states
    - snapshot commit when --fork-include-dirty is provided
    - detached HEAD SHA when applicable
  - Starts a new session:
    - Linux/macOS/WSL: tmux session with N panes, each running aifo-coder in its own clone directory.
    - Windows: wt.exe panes when available; else multiple PowerShell or Git Bash windows running in the correct directories.
  - Sets AIFO_CODER_SKIP_LOCK=1 for child panes; normal (non-fork) runs acquire a repository-scoped lock.
  - Concurrent runs in different repositories do not block each other; runs in the same repository do.
  - Each pane uses unique container name and hostname; no Docker name collisions.
  - Each pane mounts its own agent state directory; no concurrent writes to the same host ~/.aider/.codex/.crush.
  - On orchestration failure, surviving clones remain for recovery (unless keep-on-failure=false).
  - On setup failure, exit code is non-zero with clear diagnostics and recovery guidance.
  - Clones persist after successful runs for inspection and merging.
- Maintenance:
  - fork list enumerates sessions with correct metadata and stale highlighting.
  - fork clean removes targeted sessions and directories; --dry-run produces an accurate plan with summaries.

Implementation Plan (Phased)
- Phase 0: Locking groundwork
  - Implement repository-scoped locking with normalized repo root hashing and fallback paths.
  - Honor AIFO_CODER_SKIP_LOCK=1 in the launcher.
  - Tests: repo vs non-repo; Windows case-folding behavior; skip-lock.

- Phase 1: Per-pane state mounts
  - build_docker_cmd honors AIFO_CODER_FORK_STATE_DIR and mounts per-pane .aider/.codex/.crush.
  - Tests: mount substitution verified in preview; env semantics unchanged.

- Phase 2: Snapshot and cloning primitives
  - Implement snapshot via temporary index and commit-tree; return base ref/SHA or snapshot SHA.
  - Implement clone + checkout fork branches with optional --dissociate.
  - Tests: branch naming, detached HEAD, snapshot fallback on errors.

- Phase 3: Unix-like orchestrator (tmux)
  - CLI flags; fork_run() orchestration on Linux/macOS/WSL.
  - Tmux session creation with layout; robust quoting; write .meta.json; user transparency messages; recovery policy; rollback of failed panes.
  - Tests: gated E2E with N=2.

- Phase 4: Windows orchestrator
  - wt.exe orchestration; PowerShell Start-Process fallback; Git Bash support.
  - Quoting rules, working directory handling, per-process env scoping, WSL detection.
  - Tests: command construction validation; smoke if possible.

- Phase 5: Submodules/LFS best-effort
  - Detect/init submodules; detect/use Git LFS if present (non-fatal on failure).
  - Tests: mock repos where feasible.

- Phase 6: Maintenance and pruning
  - Implement fork list and fork clean; JSON output; dry-run; safety prompts.
  - Startup stale-session notice; optional auto-clean controlled by env.
  - Tests: listing/cleaning logic; threshold handling; messaging.

- Phase 7: Polish and docs
  - Diagnostics, large-N warnings, error messages, merging guidance in help output.
  - Update README/spec references.

Implementation Plan (Code Pointers)
- src/main.rs:
  - Extend Cli with fork flags: --fork, --fork-include-dirty, --fork-dissociate, --fork-session-name, --fork-layout, --fork-keep-on-failure.
  - Add fork subcommands: fork list, fork clean (args as above).
  - Early fork_run(n, &Cli) -> ExitCode:
    1) Preflight: which("git"); which("tmux") on Unix-like; on Windows detect wt.exe/git-bash.exe or fallback to PowerShell.
    2) Identify base: branch or detached SHA.
    3) Session id and name; write .meta.json skeleton.
    4) Snapshot commit (when requested) using temporary index and commit-tree.
    5) Create N clones with reference/dissociate and checkout fork branches; update .meta.json.
    6) Prepare per-pane env and child args (strip fork flags).
    7) Orchestrate (tmux/wt/PowerShell/Git Bash); on orchestration failure, preserve clones by default; print recovery guidance; otherwise attach.
    8) Print summary and return ExitCode::from(0) on success.
  - Maintenance:
    - fork_list(repo_root, opts) -> ExitCode
    - fork_clean(repo_root, opts) -> ExitCode
  - Before acquire_lock(): if env AIFO_CODER_SKIP_LOCK == "1" -> skip lock acquisition.

- src/lib.rs:
  - build_docker_cmd(): if env AIFO_CODER_FORK_STATE_DIR set, mount per-pane .aider/.codex/.crush instead of HOME equivalents.
  - Locking helpers:
    - repo_root() -> Option<PathBuf>
    - normalized_repo_key_for_hash(&Path) -> String (Windows normalization rules)
    - candidate_lock_paths() updated to prefer repo-scoped paths.

Risks and Mitigations
- Windows quoting (wt.exe/PowerShell/Git Bash): Provide well-tested quoting builders; test paths with spaces; prefer single quotes in PowerShell script content, doubling to escape.
- LFS/submodules network fetch latency: mark as best-effort/non-fatal and continue orchestration.
- Large repos/high N resource usage: warn; allow --fork-dissociate to avoid shared object GC surprises.
- Repository lock placement: prefer in-repo when writable; else runtime hashed lock; skip-lock for panes prevents deadlocks.

End of v3 specification.
