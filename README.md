# aifo-coder

Containerized launcher and images bundling three terminal AI coding agents:

- OpenAI Codex CLI (`codex`)
- Charmbracelet Crush (`crush`)
- Aider (`aider`)

Run these tools in Docker while keeping them feeling “native” on your machine:
- Seamless access to your working directory
- Your configs and state mounted from your host
- Your credentials forwarded via environment variables
- Correct file ownership (UID/GID mapping)
- A single host-side entrypoint: `./aifo-coder`
- Optional AppArmor confinement

## Why aifo-coder?

Modern coding agents are powerful, but installing and managing multiple CLIs (and their fast‑moving dependencies) can feel heavy and risky on a developer laptop. aifo‑coder bundles three best‑in‑class terminal agents (Codex, Crush and Aider) into reproducible Docker images and gives you a tiny launcher that makes them feel native. You get a clean, consistent runtime every time without polluting the host.

Typical use cases:
- Try or evaluate multiple agents without touching your host Python/Node setups.
- Keep your dev machine lean while still enjoying rich agent tooling.
- Share a single, known‑good environment across teams or CI.
- Protect your host by containing agent execution to an isolated environment.

## How it works (at a glance)

- The Dockerfile builds a shared base and three per‑agent images via multi‑stage targets:
  - aifo-coder-codex:TAG, aifo-coder-crush:TAG, aifo-coder-aider:TAG
- The `aifo-coder` launcher runs the selected agent inside the appropriate image, mounting only what’s needed:
  - Your current working directory is mounted at `/workspace`.
  - Minimal, well‑known config/state directories are mounted into the container `$HOME=/home/coder` so agents behave like locally installed tools.
  - Common credentials are forwarded via environment variables you already export on your host.
  - Your UID/GID are mapped into the container so files created in `/workspace` are owned by you.
- A lightweight lock prevents multiple agents from running concurrently against the same workspace.

## Security, isolation & privacy by design

aifo‑coder takes a “contain what matters, nothing more” approach:
- Container isolation
  - Agents run inside a container, not on your host Python/Node runtimes.
  - No privileged Docker mode; no host Docker socket is mounted.
  - A sane `$HOME` inside the container (`/home/coder`) keeps agent caches/configs scoped.
  - NSS wrapper provides a passwd entry for your runtime UID so editors like Emacs/Vim don’t complain about missing home accounts.
- Minimal surface area
  - Only the current project folder (`$PWD`) and essential per‑tool config/state paths are mounted:
    - `~/.codex` (Codex), `~/.local/share/crush` (Crush), `~/.aider` + common Aider config files, and `~/.gitconfig`.
    - Host `~/.gnupg` is mounted read‑only at `/home/coder/.gnupg-host` and copied into the container on start.
  - Nothing else from your home directory is exposed by default.
- Principle of least privilege
  - UID/GID mapping ensures any files written inside `/workspace` are owned by your host user—no unexpected root‑owned files.
  - No additional host devices, sockets or secrets are mounted.
- AppArmor (optional)
  - You can load a profile and run the container under AppArmor to further confine file/device access.
- Network considerations
  - By default, the container has normal outbound network access (agents often need it to reach model providers).
  - If you require stricter isolation (e.g., offline or deny‑by‑default), run with Docker network policies (such as `--network=none`) or behind a corporate proxy.
  - Only credentials you explicitly export in your shell are forwarded.

---

## Build the Rust launcher

The host-side launcher is now implemented in Rust. You can keep using the ./aifo-coder wrapper, which will run the compiled binary if present, or build it automatically if cargo is available.

- Build once (release):
```bash
cargo build --release
```

- Run via the wrapper:
```bash
./aifo-coder aider --version
```

If cargo is not installed, please install Rust from https://rustup.rs.

## Contents

- Dockerfile: Builds one image per agent (`codex`, `crush`, `aider`) with shared base layers
- Makefile: Build, rebuild and utilities (AppArmor, GPG helpers, “enter”)
- `aifo-coder`: Python launcher for running any of the three agents
- `examples/`: Optional example configurations (e.g., `examples/sandbox/crush/crush.json`)
- `spec/aifo-coder.spec`: Detailed project specification text
- `apparmor/aifo-coder.apparmor.tpl`: AppArmor profile template (rendered by Makefile)

---

## Requirements

- Docker installed and running
- Network access to install dependencies during build
- Access to the base image registry:
  - Base image used: `repository.migros.net/node:22-bookworm-slim`  
    If you cannot access this, replace the `FROM` line in the Dockerfile with an accessible equivalent.

---

## Build images

From the repository root:

- Build all three per‑agent images:
```bash
make build
```

- Rebuild all without cache:
```bash
make rebuild
```

- Rebuild only a specific agent:
```bash
make build-aider
```

- Rebuild all existing local tags for this prefix:
```bash
make rebuild-existing
```

- Rebuild all existing local tags without cache:
```bash
make rebuild-existing-nocache
```

You can override the image prefix/tag used for all images:
```bash
make IMAGE_PREFIX=myrepo/aifo-coder TAG=dev build
```

The resulting images will be:
- codex:  myrepo/aifo-coder-codex:dev
- crush:  myrepo/aifo-coder-crush:dev
- aider:  myrepo/aifo-coder-aider:dev

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

Use the `aifo-coder` host-side launcher:

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

All unknown arguments are forwarded to the selected agent (via `parse_known_args`).

Override the image used by the launcher (use a specific per‑agent image):
```bash
./aifo-coder --image myrepo/aifo-coder-codex:dev codex --version
```

---

## How the launcher works

When you run `./aifo-coder ...` it will:

1. Acquire a lock (`~/.aifo-coder.lock`) to ensure only one agent runs at a time (falls back to XDG_RUNTIME_DIR or /tmp if needed).
2. Build a `docker run` command with:
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
   - AppArmor (optional): `--security-opt apparmor=<profile>` if enabled and supported
   - Per‑agent image selection:
     - Defaults to `AIFO_CODER_IMAGE` if set; otherwise `IMAGE_PREFIX-<agent>:TAG` (e.g., `aifo-coder-codex:latest`)
3. Execute the agent and return its exit code.

---

## Environment variables

The launcher forwards a curated set of environment variables into the container. Populate the ones you use in your shell before running `./aifo-coder`.

Forwarded to the container (if set in your host shell):
- Generic & Codex:
  - `OPENAI_API_KEY`, `OPENAI_ORG`, `OPENAI_BASE_URL`
  - `CODEX_OSS_BASE_URL`, `CODEX_OSS_PORT`, `CODEX_HOME` (also set inside the container to `/home/coder/.codex`)
- Google / Vertex / Gemini:
  - `GEMINI_API_KEY`, `VERTEXAI_PROJECT`, `VERTEXAI_LOCATION`
- Azure (per agent needs):
  - `AZURE_OPENAI_API_ENDPOINT`, `AZURE_OPENAI_API_KEY`, `AZURE_OPENAI_API_VERSION`, `AZURE_OPENAI_ENDPOINT`
  - `AZURE_API_KEY`, `AZURE_API_BASE`, `AZURE_API_VERSION`
- Git author/committer (if you want to override):
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

Launcher control variables (read by the host launcher aifo-coder):
- Image selection:
  - `AIFO_CODER_IMAGE` — override the full image reference used for all agents
  - `AIFO_CODER_IMAGE_PREFIX` — default: `aifo-coder` (build output prefix)
  - `AIFO_CODER_IMAGE_TAG` — default: `latest`
- AppArmor control:
  - `AIFO_CODER_APPARMOR_PROFILE` — profile name to apply (e.g., `aifo-coder`)
  - `AIFO_CODER_USE_APPARMOR=1` — use default profile name `aifo-coder` if none specified
  - `AIFO_CODER_NO_APPARMOR=1` — force disable AppArmor
- Container identity:
  - `AIFO_CODER_CONTAINER_NAME` — set the container name
  - `AIFO_CODER_HOSTNAME` — set the container hostname
- Git signing:
  - `AIFO_CODER_GIT_SIGN` — set to `0|false|no|off` to disable signing for Aider runs; any other value enables
    - If enabled and a secret key exists, the launcher will auto‑configure `user.signingkey` from your keyring unless `GIT_SIGNING_KEY` is set.

Notes:
- Unknown environment variables are not forwarded by default; edit `aifo-coder` if you need more.

---

## Makefile targets

Helpful build and utility targets:

Build images:
- `build` — Build all per‑agent images (codex, crush, aider)
- `build-codex` — Build only the Codex image (`${IMAGE_PREFIX}-codex:${TAG}`)
- `build-crush` — Build only the Crush image (`${IMAGE_PREFIX}-crush:${TAG}`)
- `build-aider` — Build only the Aider image (`${IMAGE_PREFIX}-aider:${TAG}`)

Rebuild images:
- `rebuild` — Rebuild all images without cache
- `rebuild-codex` — Rebuild only Codex, no cache
- `rebuild-crush` — Rebuild only Crush, no cache
- `rebuild-aider` — Rebuild only Aider, no cache

Rebuild existing images by prefix:
- `rebuild-existing` — Rebuild any existing local images with `IMAGE_PREFIX` (using cache)
- `rebuild-existing-nocache` — Same, but without cache

Utilities:
- `clean` — Remove built images (ignores errors if not present)
- `docker-enter` — Enter a running container via `docker exec` with GPG runtime prepared

GPG helpers:
- `gpg-disable-signing` — Disable GPG signing (commit/tag) in this repo
- `gpg-enable-signing` — Re‑enable GPG signing in this repo
- `gpg-show-config` — Show current git GPG signing‑related configuration
- `gpg-disable-signing-global` — Disable GPG signing globally (`~/.gitconfig`)
- `gpg-unset-signing` — Unset local signing config for this repo
- `git-check-signatures` — Show commit signature status (`git log %h %G? %s`)
- `git-commit-no-sign` — Commit staged changes without signing
- `git-amend-no-sign` — Amend last commit without signing
- `git-commit-no-sign-all` — Stage all and commit without signing

History rewrite helper:
- `scrub-coauthors` — Remove a specific “Co‑authored‑by” line from all commit messages (uses `git-filter-repo`)

AppArmor (security):
- `apparmor` — Generate `build/apparmor/${APPARMOR_PROFILE_NAME}` from template `apparmor/aifo-coder.apparmor.tpl`
- `apparmor-load-colima` — Load the generated profile into the Colima VM (macOS)
- `apparmor-log-colima` — Stream AppArmor logs (Colima VM or local Linux) into `build/logs/apparmor.log`

Variable hints:
- `IMAGE_PREFIX` (default: `aifo-coder`) — Image name prefix for per‑agent images
- `TAG` (default: `latest`) — Tag for images
- `APPARMOR_PROFILE_NAME` (default: `aifo-coder`) — Rendered AppArmor profile name

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
- See `examples/sandbox/crush/crush.json`. Copy it into your repo and run:
```bash
./aifo-coder crush --config /workspace/examples/sandbox/crush/crush.json
```

---

## AppArmor (optional)

- Render the profile from the template:
```bash
make apparmor
```
- Load into a Linux host:
```bash
sudo apparmor_parser -r -W build/apparmor/aifo-coder
```
- Load into Colima (macOS):
```bash
make apparmor-load-colima
```
- View AppArmor logs (Colima VM or local Linux):
```bash
make apparmor-log-colima
```

Enable AppArmor when launching:
- Set `AIFO_CODER_APPARMOR_PROFILE=aifo-coder`, or
- Set `AIFO_CODER_USE_APPARMOR=1` to use the default profile name.
- Disable explicitly with `AIFO_CODER_NO_APPARMOR=1`.

---

## Troubleshooting

- “User coder has no home directory” in editors:
  - The container and launcher ensure `$HOME=/home/coder` exists, and NSS wrapper synthesizes a passwd entry for your UID. This prevents editor warnings and ensures a valid home directory.

- GPG signing errors (e.g., “No pinentry”, “Inappropriate ioctl for device”):
  - `pinentry-curses` is installed and selected by default; `GPG_TTY` is set when a TTY is present.
  - `XDG_RUNTIME_DIR` is prepared and `gpg-agent` is launched automatically.
  - Host `~/.gnupg` is mounted read‑only at `/home/coder/.gnupg-host` and keys are copied into `GNUPGHOME` on start.
  - You can disable signing for Aider runs with `AIFO_CODER_GIT_SIGN=0`.

- Permission denied creating caches:
  - Ensure the mounted host directories (`~/.codex`, `~/.aider`, `~/.local/share/crush`) exist and are writable by your user.

- Multiple agent instances:
  - If you see a lock message, another agent is running. Exit it first or remove a stale lock file at `~/.aifo-coder.lock` if you're certain no agent is active.

- AppArmor denies:
  - Use `make apparmor-log-colima` to view denies, adjust the template `apparmor/aifo-coder.apparmor.tpl`, re‑render via `make apparmor`, then reload the profile.

---

## Customization

- Image names:
  - Override via `make IMAGE_PREFIX=... TAG=... build` and `./aifo-coder --image ...`
- Additional mounts:
  - Extend `aifo-coder` if you need more persistent config paths
- Alternate base image:
  - Replace `FROM repository.migros.net/node:22-bookworm-slim` with an accessible equivalent if needed

---

## License

See repository license or your organizational policy.

---

## Acknowledgements

- OpenAI Codex CLI: https://github.com/openai/codex
- Charmbracelet Crush: https://github.com/charmbracelet/crush
- Aider: https://github.com/Aider-AI/aider
