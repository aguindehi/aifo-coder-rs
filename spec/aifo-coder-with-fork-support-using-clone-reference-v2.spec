AIFO-Coder Fork Support (Clone+Reference) v2 Specification

Status
- Version: 2
- Date: 2025-09-01
- Owner: AIFO Coder maintainers
- State: Ready for implementation

Scope and Platforms
- Supported host platforms:
  - Linux (native)
  - macOS (Docker Desktop or Colima)
  - Windows:
    - WSL: treated as Linux (preferred)
    - Native Windows: orchestrated using Windows Terminal (wt.exe) if available, else PowerShell windows; Git Bash (“Git Shell”) also supported
- Container runtime: Docker CLI available in PATH on all platforms.
- Git: Git CLI available in PATH on all platforms.

Overview
This v2 specification finalizes the clone-based fork orchestration. Each “pane” operates in an isolated Git clone created with git clone --reference-if-able, optionally followed by --dissociate. A non-destructive snapshot strategy (git commit-tree) is used for including uncommitted changes without altering the user’s working tree or requiring configured Git identity/signing. Each pane uses a unique container name/hostname and per-pane agent state directories to prevent shared-state races. The orchestration is cross-platform:
- Linux/macOS/WSL: tmux panes
- Windows native: Windows Terminal panes (wt.exe) when available; otherwise PowerShell windows or Git Bash (mintty) windows

Key Improvements vs v1
- Non-destructive snapshot of dirty workspace using git write-tree + git commit-tree (no branch switch, no HEAD move, no hooks/signing required).
- Skip-lock support for child panes via AIFO_CODER_SKIP_LOCK=1 enforced by the launcher.
- Repository-scoped locking to allow concurrent runs across different repositories (including Git Shell on Windows).
- Per-pane state mounts (AIFO_CODER_FORK_STATE_DIR) respected by build_docker_cmd for .aider/.codex/.crush.
- Cross-platform orchestration (tmux on Unix-like systems; wt.exe or PowerShell or Git Bash on Windows).
- Best-effort support for submodules and Git LFS in clones.
- Robust quoting/escaping and error handling with rollback.
- Clear diagnostics and session summary.
- Fork maintenance commands (list/clean) and automatic pruning helpers for stale sessions with user notifications.
- Clone signing policy mirrors the original repository.

CLI Additions (src/main.rs)
- Extend Cli with:
  - #[arg(long)] fork: Option<usize>                   // N >= 2 enables fork mode; N < 2 => normal single-agent run
  - #[arg(long)] fork_include_dirty: bool              // include uncommitted changes via snapshot commit
  - #[arg(long)] fork_dissociate: bool                 // clone with --dissociate for clone independence
  - #[arg(long)] fork_session_name: Option<String>     // session/window name override
  - #[arg(long, value_parser = validate_layout)] fork_layout: Option<String>  // one of {tiled, even-h, even-v}
  - Fork maintenance:
    - fork list
    - fork clean [--session <sid> | --older-than <days> | --all] [--dry-run]
- Behavior:
  - If Some(n) and n >= 2: enter fork orchestrator path early; parent process does not run an agent itself and returns ExitCode from orchestrator.
  - If N < 2: ignore fork flags and run normally.
  - Maintenance subcommands operate without starting agents.

Locking Model (Repository-Scoped)
- Motivation: Allow concurrent aifo-coder runs when they operate in different Git repositories (e.g., separate projects), including under Windows Git Shell/Git Bash environments.
- Rules:
  - If inside a Git repository (git rev-parse --show-toplevel succeeds):
    - Compute repo_root = absolute canonical top-level path.
    - Derive a repository-scoped lock path:
      - Preferred: <repo_root>/.aifo-coder.lock (portable file name)
      - Additionally, use a secondary lock path in XDG_RUNTIME_DIR or /tmp based on a stable hash of repo_root to tolerate RO repos:
        - <xdg_runtime>/aifo-coder.<hash(repo_root)>.lock
      - On Windows, normalize repo_root for hashing by:
        - Converting drive letter to uppercase
        - Using case-folded path (to avoid case-only collisions)
        - Normalizing separators
    - Acquire the first available lock in the ordered set (prefer in-repo; fall back to runtime/temp hashed path).
    - Concurrent runs in different repositories are permitted because their lock keys differ.
  - If not inside a Git repository:
    - Fall back to the legacy global candidate list (HOME/XDG_RUNTIME_DIR/tmp/CWD).
  - Skip-Lock:
    - If env AIFO_CODER_SKIP_LOCK == "1", skip acquiring any lock (used by fork child panes).
- Git Shell / Git Bash on Windows:
  - MSYS path rewriting is irrelevant to locking because the lock path is resolved using Rust std::fs with normalized absolute paths.
  - Ensure hashing uses the normalized Windows path format, not /c/ style.

Session and Branch Naming
- Session id: short base36 id derived from time ⊕ pid (reuse existing create_session_id()).
- Session name: from --fork-session-name or default aifo-<sid>.
- Branch naming: fork/<base-label>/<sid>-<i>
  - base-label: current branch (sanitized) or “detached” when HEAD detached.
  - i is 1-based pane index.

Directories and Paths
- Root repo: git rev-parse --show-toplevel (absolute canonical path).
- Forks base dir: <repo-root>/.aifo-coder/forks/<sid>.
- Pane directories: <repo-root>/.aifo-coder/forks/<sid>/pane-<i> (i = 1..N).
- Per-pane agent state dirs (host): ~/.aifo-coder/state/<sid>/pane-<i>/{.aider,.codex,.crush}
  - These are created by the parent orchestrator; build_docker_cmd mounts from AIFO_CODER_FORK_STATE_DIR when set.

Snapshot Strategy (Include Dirty)
- Goal: include the current working tree (staged + unstaged) without altering the user’s working tree or requiring user.name/email or signing.
- Steps in base repo:
  1) base_ref:
     - If on a branch: base_ref = current branch name
     - If detached:   base_sha = git rev-parse --verify HEAD
  2) git add -A (index the current working tree; best-effort)
  3) tree = git write-tree
  4) parents:
     - If HEAD exists: parent = git rev-parse --verify HEAD; snap = echo "aifo-fork snapshot <sid>" | git commit-tree "$tree" -p "$parent"
     - If no commits yet: snap = echo "aifo-fork snapshot <sid>" | git commit-tree "$tree"
  5) Use snap (SHA) as the checkout base in clones when --fork-include-dirty is set.
- This approach:
  - Does not change HEAD or current branch.
  - Does not invoke commit-msg hooks or GPG signing.
  - Works without configured user.name/user.email.
- Note: If git add -A fails, abort snapshot and fall back to base_ref; print a warning.

Clone Setup
- For each pane i in 1..=N:
  - git clone --no-checkout --reference-if-able <repo-root> <pane-dir>
  - If --fork-dissociate: add --dissociate
  - Optionally set origin push URL to match base (non-fatal):
    - base_origin=$(git -C <repo-root> remote get-url --push origin)
    - if present: git -C <pane-dir> remote set-url origin "$base_origin"
  - Determine base for checkout:
    - If include-dirty: base = snap (SHA)
    - Else if detached: base = base_sha
    - Else: base = base_ref (branch)
  - Branch name: fork/<base-label>/<sid>-<i>
  - Checkout: git -C <pane-dir> checkout -b <branch> <base>
  - Best-effort enhancements:
    - If <pane-dir>/.gitmodules exists: git -C <pane-dir> submodule update --init --recursive
    - If git lfs is available and repo uses LFS:
      - git -C <pane-dir> lfs install
      - git -C <pane-dir> lfs fetch --all
      - git -C <pane-dir> lfs checkout
  - On any failure: record pane-dir as created; rollback will remove it.

Child Arguments and Flag Stripping
- Construct child arguments from parsed Cli (not by re-parsing argv):
  - Remove fork flags: --fork, --fork-include-dirty, --fork-dissociate, --fork-session-name, --fork-layout (both spaced and equals forms).
  - Preserve subcommand and tail args (including everything after “--”).
- The child is invoked as: aifo-coder <child-args> (in each pane).

Per-Pane Environment Variables
- Each pane i sets:
  - AIFO_CODER_SKIP_LOCK=1
  - AIFO_CODER_CONTAINER_NAME=aifo-coder-<agent>-<sid>-<i>
  - AIFO_CODER_HOSTNAME=aifo-coder-<agent>-<sid>-<i>
  - AIFO_CODER_FORK_SESSION=<sid>
  - AIFO_CODER_FORK_INDEX=<i>
  - AIFO_CODER_FORK_STATE_DIR=<host-state-base>/<sid>/pane-<i>
    - host-state-base defaults to ~/.aifo-coder/state
    - Ensure <dir>/.aider, <dir>/.codex, <dir>/.crush exist (created by parent)

build_docker_cmd Changes (src/lib.rs)
- When AIFO_CODER_FORK_STATE_DIR is set:
  - Do not mount HOME-based ~/.aider, ~/.codex, ~/.crush.
  - Instead, mount:
    - <dir>/.aider -> /home/coder/.aider
    - <dir>/.codex -> /home/coder/.codex
    - <dir>/.crush -> /home/coder/.crush
  - Ensure these directories exist on the host (create if missing).
- Keep existing mounts for:
  - ~/.gnupg -> /home/coder/.gnupg-host:ro
  - ~/.gitconfig (read/write)
  - Timezone files /etc/localtime, /etc/timezone (if present)
- Container naming already honors AIFO_CODER_CONTAINER_NAME/HOSTNAME; no change needed.

Orchestration (Cross-Platform)
- Linux/macOS/WSL (tmux):
  - Preflight:
    - which tmux; if missing -> error and exit 127.
    - Must be inside a Git repo; else exit non-zero.
  - Session creation:
    - tmux new-session -d -s <session> -n aifo-fork -c <pane1-path> '<envs> aifo-coder <child-args>'
    - For panes 2..N: tmux split-window -t <session>:0 -c <paneX-path> '<envs> aifo-coder <child-args>'
    - tmux select-layout -t <session>:0 <layout> (default tiled)
    - tmux set-window-option -t <session>:0 synchronize-panes off
    - Attach:
      - If TMUX env present: tmux switch-client -t <session>
      - Else: tmux attach-session -t <session>
  - Quoting:
    - Compose a shell command using conservative POSIX escaping (reuse shell_join) and run under sh -lc '...'
    - Use tmux -c <dir> for each pane’s working directory

- Windows Native:
  - Preferred: Windows Terminal (wt.exe)
    - Preflight:
      - Check wt.exe presence (in PATH or WindowsApps). If present, use it.
    - Orchestration:
      - Create a new window or tab, then split into panes using wt split-pane commands.
      - Each pane command:
        - Set per-pane environment variables (setx is persistent; avoid it). Use PowerShell to set env for the child process only:
          PowerShell -NoExit -Command "$env:AIFO_CODER_SKIP_LOCK='1'; $env:AIFO_CODER_CONTAINER_NAME='...'; ...; Set-Location '<pane-dir>'; aifo-coder <child-args>"
        - In wt, specify -d <pane-dir> and commandline quoting for PowerShell execution.
      - Layout:
        - Use a sequence of split-pane -H / -V to approximate tiled or even layouts (best-effort).
    - Attachment:
      - wt opens a window and remains interactive; no further attach needed.
  - Fallback: PowerShell windows
    - For each pane i:
      - Start-Process -WindowStyle Normal -WorkingDirectory <pane-dir> powershell -ArgumentList "-NoExit", "-Command", "$env:AIFO_CODER_SKIP_LOCK='1'; ...; aifo-coder <child-args>"
    - Print the list of started window PIDs and pane paths.
  - Alternative: Git Bash (Git Shell / mintty)
    - Preflight: detect git-bash.exe (typical path: C:\Program Files\Git\git-bash.exe) or mintty.exe; use if requested or when wt.exe missing.
    - Launch N Git Bash windows:
      - Command: git-bash.exe -c "cd '<pane-dir>' && export VAR=... && aifo-coder <child-args>; exec bash"
      - Environment variables are set for the child process only; no persistent setx usage.
      - Tiling/panes are not supported; multiple windows are opened.
  - WSL detection:
    - If running under WSL (check WSL_DISTRO_NAME), prefer the tmux Linux path instead of native Windows orchestration.

Rollback and Error Handling
- If clone or checkout fails for any pane:
  - Best-effort deletion of all created pane directories under .aifo-coder/forks/<sid>.
  - If a tmux session or Windows Terminal/PowerShell/Git Bash orchestration partially created:
    - tmux: kill-session -t <session> (best-effort)
    - Windows Terminal/PowerShell/Git Bash: no global kill; leave windows; print warning with affected panes.
  - Exit non-zero.
- If session startup fails after clones are created:
  - Attempt to rollback created pane dirs, print diagnostics, exit non-zero.

Diagnostics and User Experience
- On success:
  - Print: “aifo-coder: fork session <sid> on base <base-label>; created N clones under .aifo-coder/forks/<sid>”
  - If working tree is dirty and --fork-include-dirty not set:
    - Print warning: “Uncommitted changes are not included in fork clones. Use --fork-include-dirty to include them via snapshot.”
  - If --fork-dissociate not set:
    - Print note: “Clones reference the base repo’s object store; avoid pruning base objects until done.”
- Large-N warning:
  - If N > 8, print a warning about disk/memory and IO impact.

Security
- Snapshot via commit-tree avoids hooks/signing and does not alter user state.
- Signing policy:
  - Clones inherit signing and other repo-level Git settings from the original (via git clone).
  - The launcher does not alter commit.gpgsign in clones. AIDER runtime env (AIFO_CODER_GIT_SIGN) can disable signing per-process without persisting to repo config.
- Per-pane state directories isolate concurrent writes for .aider/.codex/.crush.
- Containers keep AppArmor/host-gateway behavior as today; no privileged or host socket mounts.

Performance
- clone --reference-if-able minimizes object duplication.
- Optional --dissociate for independence at some cost.
- Shared global caches (npm/pip/cargo/ccache/go) remain shared and accelerate cold starts.

Environment Variables
- Child panes:
  - AIFO_CODER_SKIP_LOCK=1
  - AIFO_CODER_FORK_SESSION, AIFO_CODER_FORK_INDEX, AIFO_CODER_FORK_STATE_DIR
  - AIFO_CODER_CONTAINER_NAME / AIFO_CODER_HOSTNAME
- For Windows orchestration:
  - PowerShell: set $env:VAR for the launched process
  - Git Bash: export VAR=... within bash -c "..."; no persistent setx

Fork Maintenance Commands
- fork list:
  - Lists sessions under <repo-root>/.aifo-coder/forks with:
    - sid, creation time (from dir mtime or recorded metadata file), number of panes, base label (if recorded), age
  - Supports flags:
    - --json: machine-readable output
    - --all-repos: scan upward and optionally a configurable workspace root; default is current repo only
  - Highlights stale sessions (older than a threshold, e.g., 14 days)

- fork clean:
  - Deletes sessions and their clone directories.
  - Modes:
    - --session <sid>: remove exactly that session
    - --older-than <days>: remove sessions older than N days
    - --all: remove all sessions
    - --dry-run: report what would be removed without deleting
  - Safety:
    - Refuses to clean a session if any pane directory is the current working directory of a running shell (best-effort on Unix via lsof/fuser; on Windows, skip this check).
    - Prints a summary and requests confirmation unless --yes is provided.

Automatic Pruning/Notifications
- On normal (non-fork) runs, the launcher scans <repo-root>/.aifo-coder/forks for old sessions:
  - If sessions older than 30 days exist, print a one-line notice with a hint:
    - “Found 3 old fork sessions (oldest 47d). Consider: aifo-coder fork clean --older-than 30”
  - The threshold and behavior can be tuned via env (AIFO_CODER_FORK_STALE_DAYS).
  - No automatic deletion without explicit user action unless AIFO_CODER_FORK_AUTOCLEAN=1 is set (then apply --older-than threshold automatically at startup, printing a summary).

Testing Strategy
- Unit tests:
  - Argument stripping: ensure --fork flags are removed (both spaced and equals forms); tail args after “--” unchanged.
  - Branch name generation and detached HEAD handling; ensure sanitization for branch path components.
  - build_docker_cmd honors AIFO_CODER_FORK_STATE_DIR: mounts per-pane state instead of HOME equivalents.
  - Skip-lock behavior when AIFO_CODER_SKIP_LOCK=1 is set (acquire_lock is skipped).
  - Repository-scoped locking: different fake repo roots map to different lock keys; same repo root maps to the same key on case-insensitive Windows.
  - fork list parsing of metadata, JSON output shape.
- E2E (guarded by feature or env):
  - Linux/macOS/WSL (tmux required): create N=2 clones, verify branches, verify tmux session, and that each pane runs in its own directory.
  - Windows:
    - wt.exe present: verify window launch commands succeed (best-effort).
    - Fallbacks: PowerShell Start-Process and Git Bash git-bash.exe paths; ensure correct working directories and env in the command construction.
- Negative:
  - Missing tmux on Linux/macOS: exit 127 with clear error.
  - Outside Git repo: exit non-zero with clear error.
  - Clone failure mid-way: prior clones removed; exit non-zero.
  - Session start failure: clones removed; exit non-zero.

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
  - Shared caches (npm, pip, cargo, ccache, go) remain enabled and shared.
  - On setup failure, prior clones are rolled back and the exit code is non-zero.
  - Clones persist after successful runs for inspection and merging.
- Maintenance:
  - fork list enumerates sessions with correct metadata and stale highlighting.
  - fork clean removes targeted sessions and directories; --dry-run produces an accurate plan.

Implementation Plan (Phased)
- Phase 0: Locking groundwork
  - Implement repository-scoped locking with normalized repo root hashing and fallback paths.
  - Respect AIFO_CODER_SKIP_LOCK=1 in the launcher.
  - Tests: repo vs non-repo, Windows case-folding behavior, skip-lock.

- Phase 1: Per-pane state mounts
  - build_docker_cmd honors AIFO_CODER_FORK_STATE_DIR and mounts per-pane .aider/.codex/.crush.
  - Tests: mount substitution verified in preview; env semantics unchanged.

- Phase 2: Unix-like orchestrator (tmux)
  - CLI flags for fork mode; fork_run() orchestration on Linux/macOS/WSL.
  - Snapshot via commit-tree; clone with reference/dissociate; branch checkout.
  - Tmux session creation with layout; robust quoting; rollback on failure.
  - Tests: gated E2E with N=2.

- Phase 3: Windows orchestrator
  - wt.exe orchestration; PowerShell fallback; Git Bash support (git-bash.exe).
  - Quoting rules for PowerShell and Git Bash; working directory handling; environment scoping.
  - Tests: best-effort validation of command construction.

- Phase 4: Submodules/LFS best-effort
  - Detect and initialize submodules; detect/use Git LFS if present (non-fatal on failure).
  - Tests: mock repos where feasible.

- Phase 5: Fork maintenance (list/clean)
  - Implement fork list and fork clean subcommands.
  - JSON output option; dry-run for clean; safety prompts.
  - Tests: create fake session dirs; ensure listing/cleaning logic.

- Phase 6: Pruning helpers and notifications
  - Startup scan for stale sessions with notice; optional auto-clean via env flag.
  - Tests: threshold calculations; message formatting.

- Phase 7: Polish and docs
  - Diagnostics, large-N warnings, error messages.
  - Update README/spec references.

Implementation Plan (Code Pointers)
- src/main.rs:
  - Extend Cli with new fork flags and maintenance subcommands (+ validate_layout()).
  - Early fork_run(n, &Cli) -> ExitCode:
    1) Preflight: which("tmux") on Unix-like hosts; on Windows, detect wt.exe/git-bash.exe or fallback to PowerShell.
    2) Identify base: branch or detached SHA.
    3) Session id and name.
    4) Snapshot commit (when requested) using write-tree/commit-tree.
    5) Create N clones with reference/dissociate and checkout fork branches.
    6) Build per-pane env and child args (drop fork flags).
    7) Orchestrate session (tmux/wt/PowerShell/Git Bash); rollback on failure.
    8) Print summary and return ExitCode::from(0).
  - Maintenance:
    - fork_list(repo_root, opts) -> ExitCode
    - fork_clean(repo_root, opts) -> ExitCode
    - Optional metadata file per session: <repo-root>/.aifo-coder/forks/<sid>/.meta.json { created_at, base_label, panes }
  - Before acquire_lock(): if env AIFO_CODER_SKIP_LOCK == "1" -> skip lock acquisition.

- src/lib.rs:
  - build_docker_cmd(): if env AIFO_CODER_FORK_STATE_DIR set, switch .aider/.codex/.crush mounts to that directory’s subfolders and ensure they exist.
  - Locking helpers:
    - repo_root() -> Option<PathBuf>
    - normalized_repo_key_for_hash(&Path) -> String
    - candidate_lock_paths() updated to include repo-scoped paths when in repo; maintain compatibility with tests.

Open Questions / Future Enhancements
- Layout refinement for Windows Terminal (more precise tiling).
- Optional automatic pruning defaults and policies (org-wide configuration).
- Snapshot branch lifecycle helpers if we introduce named snapshots in future variants (currently using commit-tree SHA only).
- Enhanced safety in fork clean on Windows (detect running processes locking pane dirs).

Risks and Mitigations
- Windows quoting: Use PowerShell -Command with careful quoting; Git Bash uses bash -lc with POSIX quoting. Test with spaces in paths.
- LFS/submodules network fetch can take time: mark as best-effort and non-fatal.
- Large repos and high N: warn users; allow --fork-dissociate to avoid shared object GC surprises.
- Repository lock placement: prefer in-repo file when writable; otherwise use runtime hashed lock file.

End of v2 specification.
