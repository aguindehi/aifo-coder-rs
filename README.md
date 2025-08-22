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

Build images:
- build — Build all per‑agent images (codex, crush, aider)
- build-codex — Build only the Codex image (`${IMAGE_PREFIX}-codex:${TAG}`)
- build-crush — Build only the Crush image (`${IMAGE_PREFIX}-crush:${TAG}`)
- build-aider — Build only the Aider image (`${IMAGE_PREFIX}-aider:${TAG}`)

Rebuild images:
- rebuild — Rebuild all images without cache
- rebuild-codex — Rebuild only Codex, no cache
- rebuild-crush — Rebuild only Crush, no cache
- rebuild-aider — Rebuild only Aider, no cache

Rebuild existing images by prefix:
- rebuild-existing — Rebuild any existing local images with `IMAGE_PREFIX` (using cache)
- rebuild-existing-nocache — Same, but without cache

Launcher and release:
- build-launcher — Build the Rust host launcher (release build)
- release — Containerized, cross-platform builds and packaging into dist/

Utilities:
- clean — Remove built images (ignores errors if not present)
- docker-enter — Enter a running container via docker exec with GPG runtime prepared

GPG helpers:
- gpg-disable-signing, gpg-enable-signing, gpg-show-config
- gpg-disable-signing-global, gpg-unset-signing
- git-check-signatures, git-commit-no-sign, git-amend-no-sign, git-commit-no-sign-all

History rewrite helper:
- scrub-coauthors — Remove a specific “Co‑authored‑by” line from all commit messages (uses git-filter-repo)

AppArmor (security):
- apparmor — Generate build/apparmor/${APPARMOR_PROFILE_NAME} from template (used by Docker)
- apparmor-load-colima — Load the generated profile into the Colima VM (macOS)
- apparmor-log-colima — Stream AppArmor logs (Colima VM or local Linux) into build/logs/apparmor.log

Variables:
- IMAGE_PREFIX (default: aifo-coder) — Image name prefix for per‑agent images
- TAG (default: latest) — Tag for images
- APPARMOR_PROFILE_NAME (default: aifo-coder) — Rendered AppArmor profile name

---

## Cross-compiling and Rust specifics

The repository supports containerized cross-compilation via the Makefile, without installing platform linkers on your host.

Recommended approach:
- Use the Makefile’s release target for reproducible cross-builds and packaging:
  - `make release`
- Builds run in containerized toolchains (based on cross-rs images), as configured in Cross.toml.
- Only Docker is required for cross builds.

If you insist on native cross-compilation (macOS):
- Install rustup (manages Rust toolchains):
  - Homebrew: `brew install rustup` then initialize with `rustup-init`
- Add Linux GNU targets (only if you’re compiling natively, not needed for Makefile container builds):
  - `rustup target add aarch64-unknown-linux-gnu`
  - `rustup target add x86_64-unknown-linux-gnu`
- You generally do NOT need to install a host linker when using the Makefile’s containerized builds.
  - The Homebrew package `SergioBenitez/osxct/x86_64-unknown-linux-gnu` is unnecessary when using containerized builds and often unnecessary overall.
- Advanced (native only): configure a linker if you’re compiling x86_64-unknown-linux-gnu on macOS without containers:
  - .cargo/config.toml
    [target.x86_64-unknown-linux-gnu]
    linker = "x86_64-unknown-linux-gnu-gcc"

Summary:
- Prefer `make release` to build for multiple platforms in containers.
- Use `make build-launcher` for a quick local release build of the CLI for your host.

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

Forwarded to the container (if set in your host shell):
- Generic & Codex:
  - `OPENAI_API_KEY`, `OPENAI_ORG`, `OPENAI_BASE_URL`
  - `CODEX_OSS_BASE_URL`, `CODEX_OSS_PORT`, `CODEX_HOME` (also set inside the container to `/home/coder/.codex`)
- Google / Vertex / Gemini:
  - `GEMINI_API_KEY`, `VERTEXAI_PROJECT`, `VERTEXAI_LOCATION`
- Azure (per agent needs):
  - `AZURE_OPENAI_API_ENDPOINT`, `AZURE_OPENAI_API_KEY`, `AZURE_OPENAI_API_VERSION`, `AZURE_OPENAI_ENDPOINT`
  - `AZURE_API_KEY`, `AZURE_API_BASE`, `AZURE_API_VERSION`
- Git author/committer (optional overrides):
  - `GIT_AUTHOR_NAME`, `GIT_AUTHOR_EMAIL`, `GIT_COMMITTER_NAME`, `GIT_COMMITTER_EMAIL`
- GPG signing:
  - `GIT_SIGNING_KEY` (if you want to specify a particular key)
- Timezone:
  - `TZ`
- Editor preferences:
  - `EDITOR`, `VISUAL`

Always set inside the container:
- `HOME=/home/coder`
- `USER=coder`
- `CODEX_HOME=/home/coder/.codex`
- `GNUPGHOME=/home/coder/.gnupg`
- `XDG_RUNTIME_DIR=/tmp/runtime-<uid>` (computed by the launcher)

Launcher control variables (read by the Rust launcher):
- Image selection:
  - `AIFO_CODER_IMAGE` — override the full image reference used for all agents
  - `AIFO_CODER_IMAGE_PREFIX` — default: `aifo-coder`
  - `AIFO_CODER_IMAGE_TAG` — default: `latest`
- AppArmor control:
  - `AIFO_CODER_NO_APPARMOR=1` — force disable AppArmor (same effect as `--no-apparmor` flag)
- Container identity:
  - `AIFO_CODER_CONTAINER_NAME` — set the container name
  - `AIFO_CODER_HOSTNAME` — set the container hostname

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

Copyright (c) 2025, Amir Guindehi <amir.guindehi@mgb.ch>, Head of the Migros AI Foundation.
See the repository license or your organizational policy for licensing terms.

---

## Acknowledgements

- OpenAI Codex CLI: https://github.com/openai/codex
- Charmbracelet Crush: https://github.com/charmbracelet/crush
- Aider: https://github.com/Aider-AI/aider
