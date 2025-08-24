
# 🚀  Welcome to the Migros AI Foundaton Coder  🚀

🔒 Secure by Design | 🌍 Cross-Platform | 🦀 Powered by Rust | 🧠 Developed by AIFO

## ✨ Features:
- Linux: Coding agents run securely inside Docker containers with AppArmor.
- macOS: Transparent VM with Docker ensures isolated and secure agent execution.

## ⚙️  Secure Coding Agents
- Environment with Secure Containerization Enabled
- Platform with Adaptive Security for Linux & macOS

## 🔧 Building a safer future for coding automation in Migros Group
- Container isolation on Linux & macOS
- Agents run inside a container, not on your host runtimes
- AppArmor Support (via Docker or Colima)
- No privileged Docker mode; no host Docker socket is mounted
- Minimal attack surface area
- Only the current project folder and essential per‑tool config/state paths are mounted
- Nothing else from your home directory is exposed by default
- Principle of least privilege
- No additional host devices, sockets or secrets are mounted

## Prerequisites and installation

Requirements:
- Docker installed and running
- GNU Make (recommended for the provided Makefile targets)
- Optional: Rust stable toolchain (only needed if you build the CLI locally)

Quick install:
```bash
make build
./aifo-coder --help
```

Optional:
```bash
make build-slim
make build-launcher
./scripts/build-images.sh
```

Notes:
- By default, images are minimized by dropping apt and procps in final stages. To keep them, build with KEEP_APT=1 (see “Image build options and package dropping” below).
- The aifo-coder wrapper will auto-build the Rust launcher with cargo when possible; if cargo is missing, it can build via Docker.

## CLI usage and arguments

Synopsis:
```bash
./aifo-coder {codex|crush|aider|doctor|images|cache-clear} [global-flags] [--] [agent-args...]
```

Global flags:
- --image <ref>                Override full image reference for all agents
- --flavor <full|slim>         Select image flavor; default is full
- --verbose                    Increase logging verbosity
- --dry-run                    Print the docker run command without executing it
- --invalidate-registry-cache  Invalidate on-disk registry probe cache and re-probe
- -h, --help                   Show help

Subcommands:
- codex [args...]              Run OpenAI Codex CLI inside container
- crush [args...]              Run Charmbracelet Crush inside container
- aider [args...]              Run Aider inside container
- doctor                       Run environment diagnostics (Docker/AppArmor/UID mapping)
- images                       Print effective image references (honoring flavor/registry)
- cache-clear                  Clear the on-disk registry probe cache (alias: cache-invalidate)

Tips:
- Registry selection is automatic (prefers repository.migros.net when reachable, otherwise Docker Hub). Override via AIFO_CODER_REGISTRY_PREFIX; set empty to force Docker Hub.
- To select slim images via environment, set AIFO_CODER_IMAGE_FLAVOR=slim.

# The aifo-coder

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
- AppArmor (via Docker)
  - When supported by Docker, the launcher adds `--security-opt apparmor=<profile>`.

### AppArmor on macOS (Colima) and Docker Desktop

- macOS (Colima):
  - Build the profile from the template: make apparmor
  - Load it into the Colima VM:
```bash
colima ssh -- sudo apparmor_parser -r -W "$PWD/build/apparmor/aifo-coder"
```
  - If the custom profile is not available, the launcher will fall back to docker-default automatically.
- Docker Desktop (macOS/Windows):
  - Docker runs inside a VM; AppArmor support and profiles are managed by the VM. The launcher defaults to docker-default on these platforms.
- Native Linux:
  - If the aifo-coder profile is loaded on the host, it will be used; otherwise docker-default is used when available, or no explicit profile.

Troubleshooting:
- Check Docker AppArmor support:
```bash
docker info --format '{{json .SecurityOptions}}'
```
- List loaded profiles (Linux):
```bash
cat /sys/kernel/security/apparmor/profiles | grep -E 'aifo-coder|docker-default' || true
```

---

## Requirements

- Docker installed and running
- GNU Make for the provided Makefile targets
- Optional: Rust stable toolchain (only needed if you build the CLI locally via Makefile)

If you need to access a private base image:
- Base image used: `repository.migros.net/node:22-bookworm-slim`
- If you cannot access this, replace the `FROM` line in the Dockerfile with an accessible equivalent.

No Rust or Make installed on your host? Use the Docker-based dev helper:
- Make the script executable once:
  chmod +x scripts/dev.sh
- Run tests via Docker:
  ./scripts/dev.sh test
- Generate a CycloneDX SBOM via Docker:
  ./scripts/dev.sh sbom

---

## Quick start

- Build all agent images:
```bash
make build
```

- If make is not installed on your host, use the Docker-only helper script:
```bash
./scripts/build-images.sh
```

- Build slim variants (smaller images, fewer tools):
```bash
make build-slim
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


All trailing arguments after the agent subcommand are passed through to the agent unchanged.

---

## Makefile targets

A quick reference of all Makefile targets.

Additional/updated targets and behavior:
- build-slim, build-*-slim: Build slim image variants (-slim targets for codex, crush, aider)
- rebuild-slim, rebuild-*-slim: Rebuild slim images without cache
- release: Runs rebuild and rebuild-slim before packaging to ensure fresh images
- docker-images: List local Docker images
- checksums: Generate dist/SHA256SUMS.txt for release artifacts
- sbom: Generate CycloneDX SBOM (dist/SBOM.cdx.json) when cargo-cyclonedx is installed
- loc: Count lines of code across key file types

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
| release-for-target        | Release    | Build release archives into dist/ for targets in RELEASE_TARGETS or host default              |
| release-for-mac           | Release    | Build release for the current host (calls release-for-target)                                 |
| release-for-linux         | Release    | Build Linux release (RELEASE_TARGETS=x86_64-unknown-linux-gnu)                                |
| release                   | Release    | Aggregate: build both mac (host) and Linux                                                    |
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
| KEEP_APT                | 0             | If 1, keep apt/procps in final images; 0 (default) drops them after install |

---

## Cross-compiling and Rust specifics

This repository uses native rustup toolchains for all builds. No cross-rs containers or Cross.toml are used.

Recommended approach:
- Use release-for-target to build and package binaries for the current host or selected targets:
  - make release-for-target
- Build Linux artifacts from macOS quickly:
  - make release-for-linux
- Build both macOS (host) and Linux:
  - make release
- Specify multiple targets explicitly:
  - RELEASE_TARGETS='x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu' make release-for-target
- Ensure required Rust targets are installed:
  - rustup target add <triple> for each target you build

Notes about linkers:
- For host builds, no special setup is needed.
- For non-host Linux targets on macOS, you may need a linker toolchain. One option is to install osxct toolchains (by SergioBenitez) via Homebrew; another is to use a system-provided gcc. You can also point cargo to a linker via .cargo/config.toml:
  [target.x86_64-unknown-linux-gnu]
  linker = "x86_64-unknown-linux-gnu-gcc"

Summary:
- Prefer make release-for-target with rustup-installed targets.
- Use make build-launcher for a quick host-only build.

### macOS signing and notarization (optional)

Codesign locally (ad-hoc) for the .app:
```bash
codesign --deep --force --sign - "dist/aifo-coder.app"
```

Notarize with Apple (requires Xcode CLT and credentials set up):
```bash
xcrun notarytool submit "dist/aifo-coder.dmg" --keychain-profile "AC_NOTARY" --wait
```

Staple notarization ticket:
```bash
xcrun stapler staple "dist/aifo-coder.dmg"
```

Notes:
- Create a keychain profile with: xcrun notarytool store-credentials AC_NOTARY --apple-id "<your-apple-id>" --team-id "<team-id>" --password "<app-specific-password>"
- The DMG includes an /Applications symlink for drag-and-drop install; you can further customize a background image.

---

## What the images contain

- Node-based global CLIs:
  - `@openai/codex`
  - `@charmland/crush`
- Python-based Aider installed via `uv` into `/opt/venv` (PEP 668‑safe)
- `dumb-init`, `git`, `ripgrep`, `curl`, `emacs-nox`, `vim`, `nano`, `mg`, `nvi`
- GnuPG (`gnupg`, `pinentry-curses`) and NSS wrapper (`libnss-wrapper`)
- Default working directory: `/workspace`
- Entrypoint `/usr/local/bin/aifo-entrypoint`:
  - Ensures `$HOME` and `$GNUPGHOME` exist (0700), prepares `$XDG_RUNTIME_DIR`
  - Copies keys from `/home/coder/.gnupg-host` (read‑only mount) into `GNUPGHOME`
  - Configures pinentry to `pinentry-curses` and launches `gpg-agent`

### Slim image variants

For smaller footprints, use the -slim variants of each image:

- aifo-coder-codex-slim:TAG
- aifo-coder-crush-slim:TAG
- aifo-coder-aider-slim:TAG

Differences from the full images:
- Based on the same Debian Bookworm base
- Heavy editors (emacs-nox, vim, nano) and ripgrep are omitted; lightweight editors mg and nvi are included
- Otherwise identical behavior and entrypoint

Editors installed:
- Full images: emacs-nox, vim, nano, mg, nvi
- Slim images: mg, nvi

How to use:
- Build: make build-slim
- Run via explicit image: ./aifo-coder --image aifo-coder-codex-slim:latest codex --version
- Or pass a CLI flag or set an environment variable for automatic selection:
  - ./aifo-coder --flavor slim codex --version
  - export AIFO_CODER_IMAGE_FLAVOR=slim

## Image build options and package dropping

During image builds, the final runtime stages drop apt and procps by default to minimize attack surface. You can opt out by setting KEEP_APT=1.

Default removal sequence (applied when KEEP_APT=0):
```bash
# Remove apt and clean up
apt-get remove --purge -y apt apt-get
apt-get autoremove -y
apt-get clean
rm -rf /var/lib/apt/lists/*
```
Additionally, procps is removed when present.

How to keep apt/procps:
```bash
make KEEP_APT=1 build
make KEEP_APT=1 build-slim
```

These options propagate as Docker build-args so you can also pass them directly when invoking docker build manually.

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
     - Registry auto-selection: tries `repository.migros.net/` first; if reachable, images are referenced as `repository.migros.net/IMAGE_PREFIX-<agent>:TAG`; otherwise no registry prefix is used and Docker Hub is assumed
     - Override the registry choice by setting `AIFO_CODER_REGISTRY_PREFIX` (set to empty to force Docker Hub)
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
| AIFO_CODER_CONTAINER_NAME | If set, assigns the container name                            |
| AIFO_CODER_HOSTNAME       | If set, assigns the container hostname                        |
| AIFO_CODER_APPARMOR_PROFILE | Override AppArmor profile; defaults: docker-default on Docker-in-VM (macOS/Windows), aifo-coder on native Linux |
| AIFO_CODER_REGISTRY_PREFIX | If set, prepended to image refs (e.g., `repository.migros.net/`). If unset, the launcher tests reachability of `repository.migros.net` and uses it when available; set to empty to force Docker Hub |
| AIFO_CODER_IMAGE_FLAVOR     | Optional: set to `slim` to select `-slim` image variants instead of default full images |

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
