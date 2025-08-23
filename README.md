# aifo-coder

Containerized launcher and Docker images bundling three terminal AI coding agents:

- OpenAI Codex CLI (`codex`)
- Charmbracelet Crush (`crush`)
- Aider (`aider`)

Run these tools inside containers while keeping them feeling “native” on your machine:
- Seamless access to your working directory
- Your configs and state mounted from your host
- Your credentials forwarded via environment variables
- Correct file ownership (UID/GID mapping)
- A single host-side entrypoint: the Rust CLI `aifo-coder`
- Optional AppArmor confinement via Docker

## Why aifo-coder?

Modern coding agents are powerful, but installing and managing multiple CLIs (and their fast‑moving dependencies) can feel heavy and risky on a developer laptop. aifo‑coder bundles three best‑in‑class terminal agents (Codex, Crush and Aider) into reproducible container images and gives you a tiny Rust launcher that makes them feel native. You get a clean, consistent runtime every time without polluting the host.

Typical use cases:
- Try or evaluate multiple agents without touching your host Python/Node setups.
- Keep your dev machine lean while still enjoying rich agent tooling.
- Share a single, known‑good environment across teams or CI.
- Protect your host by containing agent execution to an isolated environment.

## How it works (at a glance)

- The Dockerfile builds a shared base and three per‑agent images via multi‑stage targets:
  - aifo-coder-codex:TAG, aifo-coder-crush:TAG, aifo-coder-aider:TAG
- The Rust `aifo-coder` launcher runs the selected agent inside the appropriate image, mounting only what’s needed:
  - Your current working directory is mounted at `/workspace`.
  - Minimal, well‑known config/state directories are mounted into the container `$HOME=/home/coder` so agents behave like locally installed tools.
  - Common credentials are forwarded via environment variables you already export on your host.
  - Your UID/GID are mapped into the container so files created in `/workspace` are owned by you.
- A lightweight lock prevents multiple agents from running concurrently against the same workspace.

## Security, isolation & privacy by design

aifo‑coder takes a “contain what matters, nothing more” approach:
- Container isolation
  - Agents run inside a container, not on your host runtimes.
  - No privileged Docker mode; no host Docker socket is mounted.
  - A sane `$HOME` inside the container (`/home/coder`) keeps agent caches/configs scoped.
  - NSS wrapper provides a passwd entry for your runtime UID so editors don’t complain about missing home accounts.
- Minimal surface area
  - Only the current project folder (`$PWD`) and essential per‑tool config/state paths are mounted:
    - `~/.codex` (Codex), `~/.local/share/crush` (Crush), `~/.aider` + common Aider config files, and `~/.gitconfig`.
    - Host `~/.gnupg` is mounted read‑only at `/home/coder/.gnupg-host` and copied into the container on start.
  - Nothing else from your home directory is exposed by default.
- Principle of least privilege
  - UID/GID mapping ensures any files written inside `/workspace` are owned by your host user—no unexpected root‑owned files.
  - No additional host devices, sockets or secrets are mounted.
- AppArmor (optional, via Docker only)
  - When supported by Docker and not disabled, the launcher adds `--security-opt apparmor=<profile>`.
  - Disable via the `--no-apparmor` flag on the Rust CLI.

---

## Requirements

- Docker installed and running
- GNU Make for the provided Makefile targets
- Optional: Rust stable toolchain (only needed if you build the CLI locally via Makefile)

If you need to access a private base image:
- Base image used: `repository.migros.net/node:22-bookworm-slim`
- If you cannot access this, replace the `FROM` line in the Dockerfile with an accessible equivalent.

---

## Quick start

- Build all agent images:
```bash
make build
```

- Build the Rust launcher locally (optional if you already have the binary):
```bash
make build-launcher
```

- Run the launcher:
```bash
./aifo-coder --help
```

- Launch an agent:
```bash
./aifo-coder codex --profile o3 --sandbox read-only --ask-for-approval on-failure
```

- Disable AppArmor for a run:
```bash
./aifo-coder --no-apparmor aider --model o3-mini --yes
```

All trailing arguments after the agent subcommand are passed through to the agent unchanged.

---

## Makefile targets

A quick reference of all Makefile targets.

| Target                     | Category   | Description                                                                                   |
|---------------------------|------------|-----------------------------------------------------------------------------------------------|
| build                     | Build      | Build all per‑agent images (codex, crush, aider)                                              |
| build-codex               | Build      | Build only the Codex image (`${IMAGE_PREFIX}-codex:${TAG}`)                                   |
| build-crush               | Build      | Build only the Crush image (`${IMAGE_PREFIX}-crush:${TAG}`)                                   |
| build-aider               | Build      | Build only the Aider image (`${IMAGE_PREFIX}-aider:${TAG}`)                                   |
| rebuild                   | Rebuild    | Rebuild all images without cache                                                              |
| rebuild-codex             | Rebuild    | Rebuild only Codex, no cache                                                                  |
| rebuild-crush             | Rebuild    | Rebuild only Crush, no cache                                                                  |
| rebuild-aider             | Rebuild    | Rebuild only Aider, no cache                                                                  |
| rebuild-existing          | Rebuild    | Rebuild any existing local images with `IMAGE_PREFIX` (using cache)                           |
| rebuild-existing-nocache  | Rebuild    | Rebuild any existing local images with `IMAGE_PREFIX` (no cache)                              |
| build-launcher            | Release    | Build the Rust host launcher (release build)                                                  |
| release                   | Release    | Build multi‑platform release archives into dist/ (native rustup toolchains)                   |
| build-app                 | Release    | Build macOS .app bundle into dist/ (Darwin hosts only)                                       |
| build-dmg                 | Release    | Build macOS .dmg image from the .app (Darwin hosts only)                                     |
| clean                     | Utility    | Remove built images (ignores errors if not present)                                           |
| docker-enter              | Utility    | Enter a running container via docker exec with GPG runtime prepared                           |
| gpg-disable-signing       | GPG        | Disable GPG commit signing for the current repo                                               |
| gpg-enable-signing        | GPG        | Enable GPG commit signing for the current repo                                                |
| gpg-show-config           | GPG        | Show effective GPG/Git signing configuration                                                  |
| gpg-disable-signing-global| GPG        | Disable GPG commit signing globally                                                           |
| gpg-unset-signing         | GPG        | Unset repo signing configuration                                                              |
| git-check-signatures      | GPG        | Check signatures of recent commits                                                            |
| git-commit-no-sign        | GPG        | Make a commit without signing                                                                 |
| git-amend-no-sign         | GPG        | Amend the last commit without signing                                                         |
| git-commit-no-sign-all    | GPG        | Commit all staged changes without signing                                                     |
| scrub-coauthors           | History    | Remove a specific “Co‑authored‑by” line from all commit messages (uses git‑filter‑repo)       |
| apparmor                  | AppArmor   | Generate build/apparmor/${APPARMOR_PROFILE_NAME} from template (used by Docker)               |
| apparmor-load-colima      | AppArmor   | Load the generated profile into the Colima VM (macOS)                                         |
| apparmor-log-colima       | AppArmor   | Stream AppArmor logs (Colima VM or local Linux) into build/logs/apparmor.log                  |

Variables used by these targets:

| Variable                | Default       | Purpose                                                 |
|-------------------------|---------------|---------------------------------------------------------|
| IMAGE_PREFIX            | aifo-coder    | Image name prefix for per‑agent images                  |
| TAG                     | latest        | Tag for images                                          |
| APPARMOR_PROFILE_NAME   | aifo-coder    | Rendered AppArmor profile name                          |
| APP_NAME                | aifo-coder    | App bundle name used for macOS .app                     |
| APP_BUNDLE_ID           | ch.migros.aifo-coder | macOS bundle identifier for the .app              |
| DMG_NAME                | aifo-coder-<version> | DMG file base name (macOS)                         |
| APP_ICON                | (none)        | Path to a .icns icon to include in the .app (optional)  |

---

## Cross-compiling and Rust specifics

This repository uses native rustup toolchains for all builds. No cross-rs containers or Cross.toml are used.

Recommended approach:
- Use the Makefile’s release target to build and package binaries:
  - make release
- Optionally specify multiple targets:
  - RELEASE_TARGETS='x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu' make release
- Ensure required Rust targets are installed:
  - rustup target add <triple> for each target you build

Notes about linkers:
- For host builds, no special setup is needed.
- For non-host Linux targets on macOS, you may need a linker toolchain. One option is to install osxct toolchains (by SergioBenitez) via Homebrew; another is to use a system-provided gcc. You can also point cargo to a linker via .cargo/config.toml:
  [target.x86_64-unknown-linux-gnu]
  linker = "x86_64-unknown-linux-gnu-gcc"

Summary:
- Prefer make release with rustup-installed targets.
- Use make build-launcher for a quick host-only build.

---

## What the images contain

- Node-based global CLIs:
  - `@openai/codex`
  - `@charmland/crush`
- Python-based Aider installed via `uv` into `/opt/venv` (PEP 668‑safe)
- `dumb-init`, `git`, `ripgrep`, `curl`, `emacs-nox`, `vim`, `nano`
- GnuPG (`gnupg`, `pinentry-curses`) and NSS wrapper (`libnss-wrapper`)
- Default working directory: `/workspace`
- Entrypoint `/usr/local/bin/aifo-entrypoint`:
  - Ensures `$HOME` and `$GNUPGHOME` exist (0700), prepares `$XDG_RUNTIME_DIR`
  - Copies keys from `/home/coder/.gnupg-host` (read‑only mount) into `GNUPGHOME`
  - Configures pinentry to `pinentry-curses` and launches `gpg-agent`

---

## Runtime launching

Use the Rust launcher:

```bash
./aifo-coder {codex|crush|aider} [agent-args...]
```

Examples:

- Run Codex with a profile and safe sandbox:
```bash
./aifo-coder codex --profile o3 --sandbox read-only --ask-for-approval on-failure
```

- Run Crush with debug:
```bash
./aifo-coder crush --debug
```

- Run Aider with a specific model:
```bash
./aifo-coder aider --model o3-mini --yes
```

Override the image used by the launcher (use a specific per‑agent image):
```bash
./aifo-coder --image myrepo/aifo-coder-codex:dev codex --version
```

---

## How the launcher works

When you run `aifo-coder ...` it will:

1. Acquire a lock to ensure only one agent runs at a time (prefers `~/.aifo-coder.lock`, falls back to XDG_RUNTIME_DIR or `/tmp`).
2. Locate Docker.
3. Build a `docker run` command with:
   - `--rm` removal after exit
   - Interactive TTY (`-it`) if connected to a terminal; otherwise `-i`
   - Bind mount your current directory to `/workspace` and set `-w /workspace`
   - Map your UID:GID (`--user UID:GID`) so files written in `/workspace` are owned by you
   - Set a sane home and Codex home:
     - `HOME=/home/coder`
     - `CODEX_HOME=/home/coder/.codex`
   - Prepare GnuPG runtime:
     - `GNUPGHOME=/home/coder/.gnupg`
     - `XDG_RUNTIME_DIR=/tmp/runtime-<uid>`
     - Mount host `~/.gnupg` read‑only at `/home/coder/.gnupg-host` for key import
   - Bind mount persistent config/state:
     - `~/.local/share/crush` → `/home/coder/.local/share/crush`
     - `~/.codex` → `/home/coder/.codex`
     - `~/.aider` → `/home/coder/.aider`
     - `~/.aider.conf.yml` → `/home/coder/.aider.conf.yml`
     - `~/.aider.model.metadata.json` → `/home/coder/.aider.model.metadata.json`
     - `~/.aider.model.settings.yml` → `/home/coder/.aider.model.settings.yml`
     - `~/.gitconfig` → `/home/coder/.gitconfig`
   - Timezone passthrough (if present):
     - `/etc/localtime` and `/etc/timezone` mounted read‑only
   - AppArmor (optional): adds `--security-opt apparmor=<profile>` if supported by Docker
   - Per‑agent image selection:
     - Defaults to `AIFO_CODER_IMAGE` if set; otherwise `IMAGE_PREFIX-<agent>:TAG` (e.g., `aifo-coder-codex:latest`)
4. Execute the agent and return its exit code.

---

## Environment variables

Forwarded from host to container (only if set in your shell):

| Variable                   | Forwarded | Notes                                                                                 |
|---------------------------|-----------|---------------------------------------------------------------------------------------|
| OPENAI_API_KEY            | Yes       | Generic & Codex                                                                       |
| OPENAI_ORG                | Yes       | Generic & Codex                                                                       |
| OPENAI_BASE_URL           | Yes       | Generic & Codex                                                                       |
| CODEX_OSS_BASE_URL        | Yes       | Codex OSS                                                                             |
| CODEX_OSS_PORT            | Yes       | Codex OSS                                                                             |
| CODEX_HOME                | Yes       | Also set in container to `/home/coder/.codex`; forwarded value may influence behavior |
| GEMINI_API_KEY            | Yes       | Google / Vertex / Gemini                                                              |
| VERTEXAI_PROJECT          | Yes       | Google / Vertex / Gemini                                                              |
| VERTEXAI_LOCATION         | Yes       | Google / Vertex / Gemini                                                              |
| AZURE_OPENAI_API_ENDPOINT | Yes       | Azure                                                                                 |
| AZURE_OPENAI_API_KEY      | Yes       | Azure                                                                                 |
| AZURE_OPENAI_API_VERSION  | Yes       | Azure                                                                                 |
| AZURE_OPENAI_ENDPOINT     | Yes       | Azure                                                                                 |
| AZURE_API_KEY             | Yes       | Azure                                                                                 |
| AZURE_API_BASE            | Yes       | Azure                                                                                 |
| AZURE_API_VERSION         | Yes       | Azure                                                                                 |
| GIT_AUTHOR_NAME           | Yes       | Optional override                                                                     |
| GIT_AUTHOR_EMAIL          | Yes       | Optional override                                                                     |
| GIT_COMMITTER_NAME        | Yes       | Optional override                                                                     |
| GIT_COMMITTER_EMAIL       | Yes       | Optional override                                                                     |
| GIT_SIGNING_KEY           | Yes       | Select a specific GPG key for signing                                                 |
| TZ                        | Yes       | Timezone passthrough                                                                  |
| EDITOR                    | Yes       | Editor preference                                                                     |
| VISUAL                    | Yes       | Editor preference                                                                     |

Always set inside the container:

| Variable         | Value                       | Notes                          |
|------------------|-----------------------------|--------------------------------|
| HOME             | /home/coder                 | Canonical container home       |
| USER             | coder                       | Runtime user                   |
| CODEX_HOME       | /home/coder/.codex          | Ensures consistent Codex home  |
| GNUPGHOME        | /home/coder/.gnupg          | GPG runtime location           |
| XDG_RUNTIME_DIR  | /tmp/runtime-<uid>          | Computed by the launcher       |

Launcher control variables (read by the Rust launcher):

| Variable                  | Default/Behavior                                              |
|---------------------------|---------------------------------------------------------------|
| AIFO_CODER_IMAGE          | If set, overrides the full image reference for all agents     |
| AIFO_CODER_IMAGE_PREFIX   | Default: `aifo-coder`                                         |
| AIFO_CODER_IMAGE_TAG      | Default: `latest`                                             |
| AIFO_CODER_NO_APPARMOR    | force disable AppArmor (same effect as `--no-apparmor`)       |
| AIFO_CODER_CONTAINER_NAME | If set, assigns the container name                            |
| AIFO_CODER_HOSTNAME       | If set, assigns the container hostname                        |

---

## Configuration & persistence

The launcher mounts common config/state from your host to make the tools behave as if installed locally:

- Crush state (cache, logs, etc.):
  - Host: `~/.local/share/crush` → Container: `/home/coder/.local/share/crush`
- Codex:
  - Host: `~/.codex` → Container: `/home/coder/.codex`
  - Inside container `CODEX_HOME` is set to `/home/coder/.codex`
- Aider:
  - State & Cache: `~/.aider` → `/home/coder/.aider`
  - Root-level config files:
    - `~/.aider.conf.yml` → `/home/coder/.aider.conf.yml`
    - `~/.aider.model.metadata.json` → `/home/coder/.aider.model.metadata.json`
    - `~/.aider.model.settings.yml` → `/home/coder/.aider.model.settings.yml`
- Git:
  - `~/.gitconfig` → `/home/coder/.gitconfig`
- Timezone:
  - `/etc/localtime` and `/etc/timezone` mounted read‑only when present

Crush example config:
```bash
./aifo-coder crush --config /workspace/examples/sandbox/crush/crush.json
```

---

## Rust implementation notes

- CLI parsing is powered by Clap; subcommands are `codex`, `crush`, and `aider`. Trailing arguments are passed through to the agent unchanged.
- TTY detection uses `atty` to select `-it` vs `-i` for interactive runs.
- The launcher uses Docker; ensure it is installed and available in PATH.
- The default image selection can be overridden via `AIFO_CODER_IMAGE`, or computed from `AIFO_CODER_IMAGE_PREFIX` and `AIFO_CODER_IMAGE_TAG`.
- A lock file is used to avoid concurrent runs against the same workspace; candidate locations include `$HOME`, `$XDG_RUNTIME_DIR`, and `/tmp`.
- Arguments are shell-escaped conservatively before passing to the container.

---

## License and copyright

Licensed under the Apache License, Version 2.0.

You may not use this project except in compliance with the License.
You may obtain a copy of the License at:

  http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, this software is
distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND,
either express or implied. See the License for the specific language governing
permissions and limitations under the License.

Copyright (c) 2025, Amir Guindehi <amir.guindehi@mgb.ch>, Head of the Migros AI Foundation.

---

## Acknowledgements

- OpenAI Codex CLI: https://github.com/openai/codex
- Charmbracelet Crush: https://github.com/charmbracelet/crush
- Aider: https://github.com/Aider-AI/aider
