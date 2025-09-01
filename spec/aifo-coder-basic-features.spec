Specification: aifo-coder (Containerized Coding Agents)

Overview
- Goal: Provide a reproducible Docker image that bundles three terminal-based AI coding agents and a host-side launcher for transparent usage:
  - OpenAI Codex CLI (codex)
  - Charmbracelet Crush (crush)
  - Aider (aider)
- The user should interact with these tools as if installed natively, with seamless access to the host working directory and personal configuration.
- Provide a Makefile to build and rebuild images locally.
- Provide a launcher script (aifo-coder) that:
  - Ensures only one agent runs at a time via locking.
  - Mounts user configuration and state directories into the container.
  - Forwards relevant environment variables and any agent CLI arguments.
  - Maps user UID/GID so created files are owned by the host user.

Upstream References
- Codex: https://github.com/openai/codex
- Crush: https://github.com/charmbracelet/crush
- Aider: https://github.com/Aider-AI/aider

Dockerfile Requirements
- Base image
  - Use a Debian Bookworm slim node image from the internal registry:
    - FROM repository.migros.net/node:22-bookworm-slim
  - Set DEBIAN_FRONTEND=noninteractive to streamline apt operations.

- System packages
  - Install via apt:
    - python3, python3-pip, python3-venv
    - git, ca-certificates, curl, ripgrep
    - dumb-init (PID 1 signal handling)
  - Clean apt lists to keep image small.

- Python package management
  - Use uv (https://astral.sh/uv) to avoid PEP 668 system-site-package restrictions.
  - Steps:
    - Install uv via the official one-liner and move uv to /usr/local/bin/uv.
    - Create a virtualenv at /opt/venv with uv venv /opt/venv.
    - Install aider-chat into the venv using uv pip with the venv’s Python.
    - Add /opt/venv/bin to PATH.

- Node CLI installations
  - Install Codex and Crush globally with npm:
    - npm install -g @openai/codex @charmland/crush
  - These should be on the PATH (provided by the base image/node global bin).

- Working directory and entrypoint
  - WORKDIR /workspace
  - Use ENTRYPOINT ["dumb-init", "--"] and default CMD ["bash"].

- Example configuration
  - Copy example configurations into /opt/examples and set EXAMPLES_DIR=/opt/examples for discovery (optional helper).

Runtime Expectations (container)
- Executables codex, crush, aider must be available on PATH.
- The default HOME within container should be configurable by the launcher (e.g., /home/coder).
- Respect user-provided configurations via bind mounts created by the launcher (see “Launcher Requirements – Mounts”).
- No need to run as privileged; operate as the mapped host UID:GID provided by the launcher.

Makefile Requirements
- Variables
  - IMAGE ?= aifo-coder:latest
  - REPO := $(shell echo $(IMAGE) | cut -d: -f1)

- Targets
  - build: docker build -t $(IMAGE) .
  - rebuild: docker build --no-cache -t $(IMAGE) .
  - rebuild-existing:
    - Rebuild all locally existing image tags whose repository prefix matches $(REPO), using cache.
  - rebuild-existing-nocache:
    - Same as above but with --no-cache.
  - clean:
    - No destructive actions by default; print a hint how to remove images (docker rmi).

- Design notes
  - Keep Makefile self-contained, no external scripts assumed.
  - Respect $(IMAGE) override from environment or command line (e.g. make IMAGE=... build).

Launcher Requirements (aifo-coder)
- General
  - Language: Python 3.
  - Script name: aifo-coder (executable).
  - Runs a container from the image and invokes one of the bundled agents.
  - Supported subcommands (mutually exclusive):
    - codex, crush, aider
  - Pass-through all unknown arguments to the agent (e.g., --profile, --sandbox, etc.) using argparse.parse_known_args.
  - Optional flag: --image to override the Docker image (default from env AIFO_CODER_IMAGE or “aifo-coder:latest”).
  - Propagate agent exit code to the host.

- Concurrency & locking
  - Ensure only one agent runs at a time via an advisory, non-blocking lock file under user’s home, e.g. ~/.aifo-coder.lock, using fcntl.flock(LOCK_EX | LOCK_NB).
  - On lock contention, print a clear message and exit with non-zero status.

- Docker run invocation
  - Use docker run --rm.
  - Allocate TTY when stdin/stdout is a TTY (-it), else use -i for non-interactive piping.
  - Mount current working directory to /workspace (-v "$PWD:/workspace") and set -w /workspace.
  - Map host UID:GID with --user UID:GID (when available) so files created in the container are owned by the host user.
  - Set HOME=/home/coder in the container environment so tools write under /home/coder instead of “/”.
  - Set CODEX_HOME=/home/coder/.codex specifically for Codex.

- Mounts (persistent host state/config)
  - Crush persistent state:
    - Host: ~/.local/share/crush
    - Container: /home/coder/.local/share/crush
    - Ensure directory exists on host; create if needed.
  - Codex state:
    - Host: ~/.codex
    - Container: /home/coder/.codex
    - Ensure directory exists on host; create if needed.
  - Aider state & config directory:
    - Host: ~/.aider
    - Container: /home/coder/.aider
    - Ensure directory exists on host; create if needed.
  - Aider top-level config files (mounted into container $HOME to ensure auto-discovery):
    - Host -> Container:
      - ~/.aider.conf.yml                -> /home/coder/.aider.conf.yml
      - ~/.aider.model.metadata.json     -> /home/coder/.aider.model.metadata.json
      - ~/.aider.model.settings.yml      -> /home/coder/.aider.model.settings.yml
    - If missing on host, create empty files to allow bind mounting.
  - Git configuration:
    - Host: ~/.gitconfig
    - Container: /home/coder/.gitconfig
    - If missing on host, create an empty file to allow bind mounting.

- Environment variables forwarded into container
  - Generic & Codex:
    - OPENAI_API_KEY, OPENAI_ORG, OPENAI_BASE_URL
    - CODEX_OSS_BASE_URL, CODEX_OSS_PORT, CODEX_HOME
  - Google / Vertex / Gemini:
    - GEMINI_API_KEY, VERTEXAI_PROJECT, VERTEXAI_LOCATION
  - Azure (per agent needs):
    - Codex:
      - AZURE_API_KEY
    - Crush:
      - AZURE_OPENAI_API_KEY
      - AZURE_OPENAI_API_ENDPOINT
      - AZURE_OPENAI_API_VERSION
    - Aider:
      - AZURE_API_KEY
      - AZURE_API_BASE
      - AZURE_API_VERSION
  - Always set inside container:
    - HOME=/home/coder
    - CODEX_HOME=/home/coder/.codex

- TTY handling
  - If stdin/stdout is a TTY, pass -it to docker run, else pass -i (useful for piping and non-interactive executions).

- Exit behavior
  - Return the agent’s exit code to the host shell.
  - On missing Docker binary, exit with 127 and print a clear message.

- Platform assumptions
  - Primary target: Linux/macOS hosts with Docker installed.
  - Windows support is possible via WSL2; the script uses fcntl so native Windows Python is not assumed.

Examples
- Build image:
  - make build
- Rebuild without cache:
  - make rebuild
- Rebuild all existing tags for the repository:
  - make rebuild-existing
  - make rebuild-existing-nocache
- Run agents:
  - ./aifo-coder codex
  - ./aifo-coder codex --profile o3 --sandbox read-only --ask-for-approval on-failure
  - ./aifo-coder crush --debug
  - ./aifo-coder aider --model o3-mini --yes

Security & Safety
- Process isolation via Docker; no privileged mode required.
- No mounting of the host Docker socket into the container.
- User ID mapping ensures correct ownership of files written in /workspace.
- Locking prevents multiple concurrent agents to avoid conflicting edits in the same working directory.

Directory Structure
- Root:
  - Dockerfile
  - Makefile
  - aifo-coder (Python launcher, executable)
  - examples/
    - crush/
      - crush.json (example configuration)
    - aider/
      - .aider.conf.yml (optional example)
- specification/
  - aifo-coder.spec (this file)

Acceptance Criteria
- Building the Docker image via make build succeeds on a standard Linux/macOS Docker installation.
- After building, running:
  - ./aifo-coder codex and ./aifo-coder crush and ./aifo-coder aider
  - All three launch inside the container; agent processes have access to /workspace.
- Agent arguments are forwarded transparently via parse_known_args.
- Host directories/files ~/.local/share/crush, ~/.codex, ~/.aider, ~/.gitconfig are mounted to corresponding locations and used by the agents.
- HOME inside the container is /home/coder and Codex uses CODEX_HOME=/home/coder/.codex.
- Only one instance of any agent can run at a time due to locking (second run exits with a clear message).
- The Makefile provides the specified targets and honors IMAGE override.

Notes / Future Enhancements (Optional)
- Add optional mounts for additional known config locations (e.g., ~/.config/aider).
- Provide a configurable mapping to share SSH keys if required by agents (disabled by default for safety).
- Provide Docker BuildKit cache mounts for faster rebuilds.
- Add CI pipeline definitions to lint, build and optionally smoke-test the image.
