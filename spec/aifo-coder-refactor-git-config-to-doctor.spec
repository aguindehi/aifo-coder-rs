# Git configuration refactor: remove in-container auto-configuration and move checks to doctor mode

Status: Implementing
Author: aifo-coder
Date: 2025-09-23

Target release: next minor
Scope: agent startup shell (docker run), doctor diagnostics

Summary
- Remove in-container Git identity and signing "auto-fix" logic executed at agent startup (Aider, Codex, Crush) inside the docker shell.
- Move validation/reporting of Git identity and signing to doctor mode on the host, with repo-level precedence over global configuration.
- Add tests to freeze behavior:
  - No more in-container writes to repo/global git config during startup.
  - Doctor reports effective identity and signing configuration with clear status and tips.

Goals
- No unexpected user configuration changes by agent startup.
- Single place (doctor) to diagnose misconfigurations with actionable tips.
- Prefer repo-level settings over global when computing the effective values.
- Keep opt-in, ephemeral overrides intact (e.g., disabling signing for Aider via env-based GIT_CONFIG_* injection).

Non-goals
- Do not remove or change host-to-container env passthrough.
- Do not change PATH or the agent command resolution behavior.
- Do not add new external dependencies.

Design

1. Remove in-container auto-configuration
   - Delete the shell segment in build_docker_cmd’s sh_cmd that:
     - Reads repo/global user.name and user.email.
     - Exports GIT_AUTHOR_* and GIT_COMMITTER_*.
     - Decides want_sign from AIFO_CODER_GIT_SIGN and writes commit.gpgsign.
     - Writes gpg.program and user.signingkey.
   - Retain environment and GnuPG setup, security, and logging.
   - Retain aider-specific ephemeral behavior that disables signing via GIT_CONFIG_* env when AIFO_CODER_GIT_SIGN is “false”.

2. Doctor: add Git identity and signing diagnostics (repo-first precedence)
   - Repository detection:
     - Use `git rev-parse --is-inside-work-tree` to detect a repo.
     - If in a repo, get repo root via `git rev-parse --show-toplevel`.
   - Helper functions:
     - git_get_repo(repo_root, key) -> Option<String>: `git -C <repo_root> config --get <key>`.
     - git_get_global(key) -> Option<String>: `git config --global --get <key>`.
     - Trim whitespace, treat empty as None.
   - Gather identity:
     - repo_name, repo_email, global_name, global_email.
     - env overrides present: GIT_AUTHOR_NAME, GIT_AUTHOR_EMAIL.
     - Effective values (for display): env > repo > global.
     - Validate identity: name not "Your Name"; email contains '@' and not "you@example.com".
   - Signing policy:
     - desired_signing from env AIFO_CODER_GIT_SIGN (0/false/no/off => disabled; else enabled).
     - commit.gpgsign and user.signingkey: repo-first then global.
     - Secret key availability: `gpg --list-secret-keys --with-colons` and check for `fpr:` lines.
   - Output formatting:
     - Follow existing doctor style (strong blue for values, green/red statuses).
     - Rows for identity (repo/global/effective) and email; env presence rows.
     - Rows for signing desired, commit.gpgsign (repo/global/effective), signing key (effective), secret keys available.
     - Tips in verbose mode:
       - If desired but not enabled effectively → instruct to set commit.gpgsign true.
       - If desired and key unset but secret key exists → instruct to set user.signingkey.
       - If desired and no secret keys → instruct to create/import a key.
       - If desired off but repo enables signing → tip to disable in repo.

3. Precedence rules
   - Identity effective: env > repo > global.
   - Signing effective: repo > global.
   - No in-container writes to repo/global git config.

Acceptance criteria
- Startup containers do not write git config files.
- No shell snippet remains in sh_cmd that runs `git -C /workspace config ...` or exports GIT_AUTHOR_* / GIT_COMMITTER_* automatically.
- Doctor shows repo/global/effective identity and signing, with repo-first precedence.
- Doctor prints actionable tips based on desired_signing and current config.
- Tests enforce absence of in-container git writes and verify doctor logic.

Phased implementation plan
- Phase 1: Implement doctor helpers and output.
- Phase 2: Remove in-container auto-configuration.
- Phase 3: Harden tests and messaging.
- Phase 4: Documentation and migration notes.

Testing strategy
- Unit tests for preview string to ensure no git mutation commands remain.
- Optional deeper doctor tests using isolated temporary repos and HOME; capture stderr and assert rows and tips.

Rollback plan
- If needed, reintroduce the old snippet behind an opt-in env flag (AIFO_CODER_LEGACY_GIT_AUTOCONFIG=1), default off.
