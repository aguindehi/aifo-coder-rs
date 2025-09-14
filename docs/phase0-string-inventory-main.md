AIFO Coder — Phase 0: String inventory and helper verification (src/main.rs)

Scope
- Enumerate all user-visible strings in src/main.rs that must remain identical after refactoring (stdout/stderr text, warnings, previews, guidance, summaries).
- Verify presence and intended usage of aifo_coder library helpers referenced by the Phase 2+ orchestrator refactor.

Notes
- Keep exact punctuation, capitalization, emojis, ANSI color sequences placement logic, and stdout/stderr routing identical.
- Do not change exit codes associated with these messages.
- “Preview” lines are typically on stderr via eprintln! in verbose and/or dry-run modes.

A) Startup banner (stdout)
Function: print_startup_banner()

1) Leading/trailing blank lines:
- println!(); before and after banner sections.

2) Static lines:
- "─────────────────────────────────────────────────────────────────────────────────────────────"
- " 🚀  Welcome to the Migros AI Foundation Coder - AIFO Coder v{version}                     🚀 "
- " 🔒 Secure by Design  |  🌍 Cross-Platform  |  🦀 Powered by Rust  |  🧠 Developed by AIFO"
- " ✨ Features:"
- "    - Linux: Docker containers with AppArmor when available; seccomp and cgroup namespaces."
- "    - macOS: Docker Desktop/Colima VM isolation; same security features inside the VM."
- "    - Windows: Docker Desktop VM; Windows Terminal/PowerShell/Git Bash fork orchestration."
- " ⚙️  Starting up coding agents..."
- " 🔧 Building a safer future for coding automation in Migros Group..."
- "    - Containerized agents; no privileged mode, no host Docker socket."
- "    - AppArmor (Linux) with custom 'aifo-coder' or 'docker-default' when available."
- "    - Seccomp and cgroup namespaces as reported by Docker."
- "    - Per-pane isolated state for forks (.aider/.codex/.crush)."
- "    - Language toolchain sidecars (rust, node/ts, python, c/cpp, go) via secure proxy."
- "    - Optional unix:// proxy on Linux; host-gateway bridging when needed."
- "    - Minimal mounts: project workspace, config files, optional GnuPG keyrings."
- " 📜 Written 2025 by Amir Guindehi <amir.guindehi@mgb.ch>, Head of Migros AI Foundation at MGB"

3) Dynamic summary lines (stdout; exact prefixes, punctuation):
- "    - Environment: Docker={docker_disp} | Virt={virtualization}"
- "    - Platform: {os}/{arch}"
- "    - Security: {AppArmor on/off string}, Seccomp={seccomp}, cgroupns={cgroupns}, rootless={yes|no}"
- "    - Version: {version}"

B) Fork mode common messages (stdout unless noted; colorized variants must remain identical)
- Summary header (stdout):
  - Colorized variant: "aifo-coder: fork session {sid} on base {base_label} ({base_ref_or_sha})" with ANSI applied as in current code when stdout is TTY.
  - Non-colorized variant ditto without ANSI.
- "created {panes} clones under {session_dir}"
- Optional snapshot lines:
  - Colorized: "included dirty working tree via snapshot {sha}"
  - Non-colorized same text.
  - Warning if requested include-dirty failed:
    - Colorized: "warning: requested --fork-include-dirty, but snapshot failed; dirty changes not included."
    - Non-colorized same text.
- Note when --fork-dissociate not set:
  - "note: clones reference the base repo’s object store; avoid pruning base objects until done."
- Per-pane info blocks (stdout):
  - "[{i}] folder={pane_dir}"
  - "    branch={branch}"
  - "    state={state_dir}"
  - "    container={container_name}"
  - With colorized equivalents as in current code.
- Verbose (stderr) layout echo:
  - "aifo-coder: tmux layout requested: {layout} -> effective: {layout_effective}"

C) Fork preflights and generic errors (stderr)
- Not in git repo:
  - "aifo-coder: error: fork mode must be run inside a Git repository."
- Too many panes prompt text (uses warn_prompt_continue_or_quit; messages come via library).
- Missing git (any OS):
  - "aifo-coder: error: git is required and was not found in PATH."
- Non-Windows: tmux missing:
  - "aifo-coder: error: tmux not found. Please install tmux to use fork mode."
- Windows: no wt/pwsh/bash:
  - "aifo-coder: error: none of Windows Terminal (wt.exe), PowerShell, or Git Bash were found in PATH."
- Cloning error:
  - "aifo-coder: error during cloning: {e}"
- Base determination error:
  - "aifo-coder: error determining base: {e}"

D) Windows orchestrators — selection, launch, fallback (stderr/stdout; exact text)
- Env preference enforcement:
  - If AIFO_CODER_FORK_ORCH=gitbash but neither Git Bash nor mintty found:
    - "aifo-coder: error: AIFO_CODER_FORK_ORCH=gitbash requested but Git Bash/mintty were not found in PATH."
- Windows Terminal (wt) selection with post-merge requested:
  - Warning (stderr, color-aware):
    - "aifo-coder: using PowerShell windows to enable post-fork merging (--fork-merge-strategy)."
- Windows Terminal execution failures:
  - "aifo-coder: Windows Terminal failed to start first pane (non-zero exit)."
  - "aifo-coder: Windows Terminal failed to start first pane: {e}"
  - "aifo-coder: Windows Terminal split-pane failed for one or more panes."
  - Non-waitable fallback warning pair (stderr, color-aware):
    - "aifo-coder: note: no waitable orchestrator found; automatic post-fork merging ({strategy}) is unavailable."
    - "aifo-coder: after you close all panes, run: aifo-coder fork merge --session {sid} --strategy {strategy}"
  - No orchestrators at all final error:
    - "aifo-coder: error: neither Windows Terminal (wt.exe), PowerShell, nor Git Bash/mintty found in PATH."
- Git Bash/mintty launch failures:
  - "aifo-coder: failed to launch one or more Git Bash windows."
  - "aifo-coder: failed to launch one or more mintty windows."
- PowerShell launch failures:
  - "aifo-coder: failed to launch one or more PowerShell windows."
- Recovery/cleanup informational (stdout), identical wording:
  - "Removed all created pane directories under {session_dir}."
  - "Clones remain under {session_dir} for recovery."
  - "One or more clones were created under {session_dir}."
  - "You can inspect them manually. Example:"
  - "Example recovery:"
  - And the fixed “git -C ...” guidance lines printed thereafter.

E) Fork post-merge logs (stderr; color-aware, exact text)
- Start:
  - "aifo-coder: applying post-fork merge strategy: {strategy}"
- Success:
  - "aifo-coder: merge strategy '{strategy}' completed."
- Failure:
  - "aifo-coder: merge strategy '{strategy}' failed: {error}"
- Autoclean success flow (octopus only; not in dry-run):
  - "aifo-coder: octopus merge succeeded; disposing fork session {sid} ..."
  - "aifo-coder: disposed fork session {sid}."
  - Warning on dispose failure:
    - "aifo-coder: warning: failed to dispose fork session {sid}: {e}"

F) Fork session launched/completed (stdout; exact text)
- "aifo-coder: fork session {sid} launched (Git Bash)."
- "aifo-coder: fork session {sid} launched (mintty)."
- "aifo-coder: fork session {sid} launched in Windows Terminal."
- "aifo-coder: fork session {sid} launched (PowerShell windows)."
- After tmux session exit:
  - Colorized or plain "aifo-coder: fork session {sid} completed."

G) Guidance block printed after launch (stdout)
- Comes from guidance::print_inspect_merge_guidance; the orchestrator passes:
  - Git Bash: include_remote_examples=true, extra_spacing_before_wrapper=false
  - Windows Terminal: include_remote_examples=false, extra_spacing_before_wrapper=true
  - PowerShell: include_remote_examples=false, extra_spacing_before_wrapper=true
  - Tmux: include_remote_examples=false, extra_spacing_before_wrapper=true, with colored heading based on stdout TTY
- These lines include fixed command examples beginning with "To inspect and merge changes, you can run:" and wrapper invocations.

H) Tmux path-specific messages (stderr/stdout)
- tmux new-session error:
  - "aifo-coder: tmux new-session failed to start: {e}"
  - "aifo-coder: tmux new-session failed."
- tmux split-window failure:
  - "aifo-coder: tmux split-window failed for one or more panes."
- tmux verbose previews (stderr):
  - "aifo-coder: tmux: {joined command preview}"
- Windows Terminal verbose previews (stderr):
  - "aifo-coder: windows-terminal: {joined preview}"
- Git Bash/mintty verbose previews (stderr):
  - "aifo-coder: git-bash: {joined preview}"
  - "aifo-coder: mintty: {joined preview}"
- PowerShell verbose (stderr):
  - "aifo-coder: powershell start-script: {script}"
  - "aifo-coder: powershell detected at: {path}"
  - wait-script preview when merging:
  - "aifo-coder: powershell wait-script: {cmd}"

I) Fork metadata update prints (stdout)
- After failure cleanup:
  - "Removed all created pane directories under {session_dir}."
  - "Clones remain under {session_dir} for recovery."
- The rest of metadata writes are silent (file I/O only).

J) Non-fork command paths (stdout/stderr)
Doctor:
- "aifo-coder doctor" (stderr)
- "doctor: completed diagnostics." (stderr)
Images:
- "aifo-coder images" (stderr)
- Lines showing "  flavor:   {colored or plain}", "  registry: {colored or plain}"
- "  codex: {img}", "  crush: {img}", "  aider: {img}"

Cache clear:
- "aifo-coder: cleared on-disk registry cache." (stderr)

Toolchain cache clear:
- "aifo-coder: purged toolchain cache volumes." (stderr)
- "aifo-coder: failed to purge toolchain caches: {e}" (stderr)

Toolchain subcommand (stderr):
- Verbose info:
  - "aifo-coder: toolchain kind: {kind}"
  - "aifo-coder: toolchain image override: {img}"
  - "aifo-coder: toolchain caches disabled for this run"
- Dry-run previews (when verbose): “would …” lines:
  - "aifo-coder: would attach toolchains: {kinds:?}"
  - "aifo-coder: would use image overrides: {overrides:?}"
  - "aifo-coder: would disable toolchain caches"
  - "aifo-coder: would use unix:/// socket transport for proxy and mount /run/aifo"
  - "aifo-coder: would bootstrap: {bootstraps:?}"
  - "aifo-coder: would prepare and mount /opt/aifo/bin shims; set AIFO_TOOLEEXEC_URL/TOKEN; join aifo-net-<id>"
- Runtime (non-dry-run):
  - "aifo-coder: using embedded PATH shims from agent image (/opt/aifo/bin)"
  - "aifo-coder: failed to start toolchain sidecars: {e}"
  - "aifo-coder: typescript bootstrap failed: {e}"
  - "aifo-coder: failed to start toolexec proxy: {e}"

K) Agent run path: docker build/exec logs (stderr)
- Verbose:
  - "aifo-coder: effective apparmor profile: {profile or (disabled)}"
  - "aifo-coder: registry: {reg_display} (source: {reg_src})"
  - "aifo-coder: image: {image}"
  - "aifo-coder: agent: {agent}"
  - "aifo-coder: docker: {preview}"
- Dry-run:
  - "aifo-coder: dry-run requested; not executing Docker."
- Abort on tmp workspace prompt decline (stderr then ExitCode 1):
  - "aborted."
- Lock acquisition error uses Display of io::Error (keep unchanged).
- On docker command NotFound path, exit 127 (message is underlying error via Display).

L) Shared maintenance notices (stdout/stderr)
- fork_autoclean_if_enabled(); fork_print_stale_notice(): messages defined in library; main.rs just invokes. Preserve call order and streams:
  - Do not print stale notice for Doctor; printed otherwise early in non-fork runs.

M) Environment variables read/set timing (must remain identical)
- AIFO_CODER_IMAGE_FLAVOR set from CLI flavor before images/doctor/toolchain/agent runs.
- AIFO_SESSION_NETWORK, AIFO_TOOLEEXEC_* set only when toolchain session started.
- AIFO_TOOLCHAIN_VERBOSE set when verbose toolchain proxy active.
- AIFO_CODER_SKIP_LOCK checked just before docker status() spawn.
- In fork panes env per pane:
  - AIFO_CODER_SUPPRESS_TOOLCHAIN_WARNING=1 is included in pane env in all orchestrators (Windows+tmux).
- Windows-only verbose: PowerShell path detection output order unchanged.

N) Exit codes (do not change)
- 127 for NotFound cases (docker not in PATH; Windows orchestrator “none found” preflight).
- 1 for generic errors, user aborts (warn prompt declines), and fork orchestration failures.

O) Streams and color
- Banner (stdout only), no additional color beyond current formatting.
- Previews, warnings, errors, and merge progress on stderr (eprintln!) as in code.
- Color decisions via aifo_coder::color_enabled_* are kept exactly where currently used; do not change.

Helper verification (library APIs present and expected behavior)
Checked in src/lib.rs (public re-exports and helpers):
- Orchestrator inner builders:
  - fork_ps_inner_string(agent, sid, i, pane_dir, state_dir, child_args) -> String
    - Returns Set-Location; $env: assignments for pane/session vars; then "aifo-coder ..." command.
    - Does NOT include AIFO_CODER_SUPPRESS_TOOLCHAIN_WARNING=1; bin-side must inject via string augmentation for PS.
  - fork_bash_inner_string(agent, sid, i, pane_dir, state_dir, child_args) -> String
    - Returns "cd … && export …; aifo-coder …; exec bash"
    - Does NOT include SUPPRESS var; bin-side must inject; also must trim "; exec bash" when post-merge requested.
- Windows Terminal helpers:
  - wt_orient_for_layout(layout, i) -> "-H" | "-V"
  - wt_build_new_tab_args(psbin, pane_dir, inner) -> Vec<String>
  - wt_build_split_args(orient, psbin, pane_dir, inner) -> Vec<String>
    - Both include "wt" as argv[0]. Keep for preview strings; drop argv[0] when passing to Command::new(wt_path).
- Waiting helpers:
  - ps_wait_process_cmd(ids: &[&str]) -> "Wait-Process -Id …"
- Merge and clean:
  - fork_merge_branches_by_session(root, sid, strategy, verbose, dry_run) -> Result<(), String>
  - fork_clean(root, &ForkCleanOpts) -> Result<i32, String>
  - fork_autoclean_if_enabled(); fork_print_stale_notice()
- Misc required:
  - repo_root(), fork_base_info(), fork_create_snapshot(), fork_clone_and_checkout_panes()
  - json_escape(), shell_join(), shell_escape()
  - preferred_registry_prefix[_quiet](), preferred_registry_source()
  - container_runtime_path(), docker_supports_apparmor(), desired_apparmor_profile[_quiet]()

Implications for Phase 1+
- All strings listed above must be preserved verbatim in any module extraction (e.g., moved into src/fork/*, src/banner.rs, etc.).
- For Windows inner strings, bin-side builders will augment library outputs by:
  - PowerShell: insert "$env:AIFO_CODER_SUPPRESS_TOOLCHAIN_WARNING='1';" immediately after the first “; ” following Set-Location.
  - Git Bash: prefix "export AIFO_CODER_SUPPRESS_TOOLCHAIN_WARNING=1; " after existing export lines.
  - When a post-merge strategy is requested, trim the trailing "; exec bash" from the Git Bash inner string.
- For Windows Terminal execution, use wt_* helpers to build preview strings; when executing with Command::new(wt_path), drop the first "wt" element from the args.

This document should be kept alongside the refactor PR to validate that user-visible behavior remains 1:1.

Phase 0 validation status (documentation-only; no code changes)
- Public façade helpers verified present under aifo_coder:: and gated as documented:
  - Windows-only: fork_ps_inner_string, fork_bash_inner_string, wt_orient_for_layout, wt_build_new_tab_args, wt_build_split_args, ps_wait_process_cmd are behind cfg(windows).
  - Cross-platform: MergingStrategy, path_pair, ensure_file_exists, create_session_id, color helpers (color_enabled_* and paint), and fork maintenance helpers (fork_* functions referenced here) are accessible via the crate façade.
- Behavioral invariants revalidated against current code:
  - Windows inner-string builders exclude AIFO_CODER_SUPPRESS_TOOLCHAIN_WARNING; Git Bash variant ends with “; exec bash”.
  - wt_* argument previews include "wt" as argv[0]; execution paths drop argv[0] before Command::new(wt_path).
  - warn_prompt_continue_or_quit emits exactly two trailing newlines and preserves platform-specific input handling on Windows/Unix.
  - Exit code mapping and stderr/stdout routing for previews, warnings, and errors are unchanged.
- Environment timing/ordering checks aligned with inventory Section M:
  - Reads/writes for AIFO_CODER_SKIP_LOCK, AIFO_CODER_FORK_SESSION, AIFO_TOOLEEXEC_*, and color mode occur at the same points in program flow as documented.
- Scope note:
  - Phase 0 is a guardrail-only step. No implementation changes were made; later phases will refactor modules without altering any strings, exit codes, or routing captured here.
