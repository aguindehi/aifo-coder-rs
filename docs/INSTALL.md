
Prerequisites:
- Docker installed and running
- Git installed and available in PATH
- GNU Make (recommended)
- Optional: Rust toolchain (for building the launcher locally)

Fork mode prerequisites:
- Linux/macOS/WSL: tmux installed and available in PATH
  - macOS (Docker Desktop/Colima): install via Homebrew:
    brew install tmux
- Windows: Windows Terminal (wt.exe) recommended; fallback to PowerShell windows or Git Bash/mintty

Quick install:
```bash
make build
make build-launcher
make install
aifo-coder --help
```

Notes:
- Registries and prefixes:
  - Internal registry (IR): set AIFO_CODER_INTERNAL_REGISTRY_PREFIX to a host/path prefix with a
    trailing "/" (e.g., registry.intern.migros.net/ai-foundation/prototypes/aifo-coder-rs/). IR
    takes precedence at runtime for our aifo-coder-* images.
  - Mirror registry (MR): set AIFO_CODER_MIRROR_REGISTRY_PREFIX to a host prefix with a trailing
    "/" (e.g., repository.migros.net/). When IR is unset, MR prefixes unqualified third‑party images at
    runtime. Internal namespaces do not apply to MR.
  - Internal namespace: set AIFO_CODER_INTERNAL_REGISTRY_NAMESPACE for the path segment used with
    the internal registry (default: ai-foundation/prototypes/aifo-coder-rs).
- OTEL defaults for release binaries:
  - Release launchers may be built in CI with a baked-in default OTLP endpoint and transport, derived from
    CI variables `AIFO_OTEL_ENDPOINT` and `AIFO_OTEL_TRANSPORT` via `build.rs`.
  - At runtime, you can always override the endpoint with `OTEL_EXPORTER_OTLP_ENDPOINT`; setting
    `AIFO_CODER_OTEL=0|false|no|off` disables telemetry entirely.
  - For local builds, you can opt into a specific default by exporting `AIFO_OTEL_ENDPOINT`/`AIFO_OTEL_TRANSPORT`
    before running `cargo build`.
- Agent image overrides:
  - AIFO_CODER_AGENT_IMAGE: full image reference used verbatim (host/path:tag or @digest).
  - AIFO_CODER_AGENT_TAG: retags the default agent image (e.g., release-0.6.3).
  - CLI override: --image takes precedence over defaults.
  - Default tag: release-<version> matching the launcher version (e.g., release-0.6.3). Override via AIFO_CODER_IMAGE_TAG or AIFO_CODER_AGENT_TAG.
  - Automatic login: on permission-denied pulls, aifo-coder prompts for docker login to the
    resolved registry and retries (interactive only). Disable via AIFO_CODER_AUTO_LOGIN=0.
- To build only full (fat) images: make build-fat
- To build only slim images: make build-slim
- The wrapper script aifo-coder will try to build the Rust launcher with cargo; if cargo is missing, it can build using Docker.
- Images drop apt and procps by default to reduce surface area. Keep them by passing KEEP_APT=1:
```bash
make KEEP_APT=1 build
```
  - Internal registry (IR): set AIFO_CODER_INTERNAL_REGISTRY_PREFIX to a host/path prefix with a
    trailing "/" (e.g., registry.intern.migros.net/ai-foundation/prototypes/aifo-coder-rs/). IR
    takes precedence at runtime for our aifo-coder-* images.
  - Mirror registry (MR): set AIFO_CODER_MIRROR_REGISTRY_PREFIX to a host prefix with a trailing
    "/" (e.g., repository.migros.net/). When IR is unset, MR prefixes unqualified third‑party images at
    runtime. Internal namespaces do not apply to MR.
  - Internal namespace: set AIFO_CODER_INTERNAL_REGISTRY_NAMESPACE for the path segment used with
    the internal registry (default: ai-foundation/prototypes/aifo-coder-rs).
- OTEL defaults for release binaries:
  - Release launchers may be built in CI with a baked-in default OTLP endpoint and transport, derived from
    CI variables `AIFO_OTEL_ENDPOINT` and `AIFO_OTEL_TRANSPORT` via `build.rs`.
  - At runtime, you can always override the endpoint with `OTEL_EXPORTER_OTLP_ENDPOINT`; setting
    `AIFO_CODER_OTEL=0|false|no|off` disables telemetry entirely.
  - For local builds, you can opt into a specific default by exporting `AIFO_OTEL_ENDPOINT`/`AIFO_OTEL_TRANSPORT`
    before running `cargo build`.
- Agent image overrides:
  - AIFO_CODER_AGENT_IMAGE: full image reference used verbatim (host/path:tag or @digest).
  - AIFO_CODER_AGENT_TAG: retags the default agent image (e.g., release-0.6.3).
  - CLI override: --image takes precedence over defaults.
  - Default tag: release-<version> matching the launcher version (e.g., release-0.6.3). Override via AIFO_CODER_IMAGE_TAG or AIFO_CODER_AGENT_TAG.
  - Automatic login: on permission-denied pulls, aifo-coder prompts for docker login to the
    resolved registry and retries (interactive only). Disable via AIFO_CODER_AUTO_LOGIN=0.
- To build only full (fat) images: make build-fat
- To build only slim images: make build-slim
- The wrapper script aifo-coder will try to build the Rust launcher with cargo; if cargo is missing, it can build using Docker.
- Images drop apt and procps by default to reduce surface area. Keep them by passing KEEP_APT=1:
```bash
make KEEP_APT=1 build
```

Troubleshooting:
- Ensure your user can run Docker commands without sudo.
- If builds are slow, Docker may be pulling base layers; subsequent runs are faster.
- To list locally built images:
```bash
make docker-images
```

Useful Makefile targets:
- Build images:
  - make build, make build-fat, make build-slim
- Rebuild images without cache:
  - make rebuild, make rebuild-fat, make rebuild-slim
- Rebuild existing local images for your prefix:
  - make rebuild-existing, make rebuild-existing-nocache
- Build the Rust launcher:
  - make build-launcher
- Run tests:
  - make test
- macOS app and DMG (Darwin hosts only):
  - make release-app, make release-dmg, make release-dmg-sign
- Utilities:
  - make docker-images, make docker-enter, make checksums, make sbom, make loc

macOS code signing with a self‑signed certificate (no Apple Developer account)
- Create a self‑signed “Code Signing” certificate in your login keychain using Keychain Access:
  1) Open Keychain Access → Keychain: login → Menu: Keychain Access → Certificate Assistant → Create a Certificate…
  2) Name: choose a clear name (e.g., Migros AI Foundation Code Signer)
  3) Identity Type: Self Signed Root
  4) Certificate Type: Code Signing (ensures Extended Key Usage includes Code Signing)
  5) Key Size: 2048 (or 4096), Location: login keychain
  6) Ensure the certificate and its private key appear in the login keychain.
- Verify codesign can find and use it:
```bash
security find-identity -p basic -v | grep -i 'Code Sign' || true
security find-certificate -a -c "AI Foundation Code Signer" -Z 2>/dev/null | sed -n '1,12p'
```
- Build and sign on macOS:
```bash
make release-dmg-sign SIGN_IDENTITY="AI Foundation Code Signer"
```
- The Makefile will use basic signing flags for non‑Apple identities and will skip notarization automatically.

Tips:
- If prompted for key access, allow codesign to use the private key.
- If your login keychain is locked:
```bash
security unlock-keychain -p "<your-password>" login.keychain-db
```
- Clear extended attributes if you hit quarantine/signing issues:
```bash
xattr -cr dist/aifo-coder.app dist/aifo-coder.dmg
```

Toolchains
- aifo-coder can attach language toolchains (rust, node/typescript, python, c-cpp, go) as sidecar containers and inject PATH shims inside the agent so tools like cargo, npx, python, gcc, go work transparently.
- See docs/TOOLCHAINS.md for details, examples, and testing instructions.

Platform notes
- macOS/Windows: Use Docker Desktop; host.docker.internal resolves automatically to the host. TCP proxy mode works out of the box.
- Linux:
  - In TCP mode, the launcher adds --add-host=host.docker.internal:host-gateway to ensure containers can reach the host proxy.
  - Optionally enable unix socket transport with --toolchain-unix-socket, which mounts the proxy socket into the agent at /run/aifo and avoids TCP entirely.

Usage (global flags)
- Attach toolchains (repeatable):
```bash
aifo-coder --toolchain rust aider -- cargo --version
aifo-coder --toolchain node aider -- npx --version
aifo-coder --toolchain python aider -- python -m pip --version
aifo-coder --toolchain c-cpp aider -- cmake --version
```
- Per-language image override and cache control:
```bash
aifo-coder --toolchain rust --toolchain-image rust=rust:1.80-slim aider -- cargo --help
aifo-coder --toolchain node --no-toolchain-cache aider -- npm ci
```
- Linux unix-socket transport (reduces TCP surface):
```bash
aifo-coder --toolchain rust --toolchain-unix-socket aider -- cargo --version
```

C/C++ sidecar (local build and publish)
- Build the c-cpp sidecar locally:
```bash
make build-toolchain-cpp
```
- Rebuild without cache:
```bash
make rebuild-toolchain-cpp
```
- Safe multi-arch publish to a private registry (never docker.io unless REGISTRY is set):
```bash
make publish-toolchain-cpp PLATFORMS=linux/amd64,linux/arm64 PUSH=1 REGISTRY=repository.migros.net/
```

Toolchain caches
- Caches are enabled by default (named Docker volumes). Purge all toolchain caches:
```bash
aifo-coder toolchain-cache-clear
make toolchain-cache-clear
```

Tests (optional, require Docker)
- TCP proxy smoke (ignored by default):
```bash
make test-proxy-smoke
```
- Linux-only unix-socket proxy smoke (falls back to TCP on non-Linux):
```bash
make test-proxy-unix
```
- TCP streaming integration (ignored by default):
```bash
make test-proxy-tcp
```
- Dev-tool routing across sidecars (ignored by default):
```bash
make test-dev-tool-routing
```
- TypeScript local tsc resolution (ignored by default):
```bash
make test-tsc-resolution
```
- Proxy error semantics (ignored by default):
```bash
make test-proxy-errors
```
- C/C++ dry-run tests:
```bash
make test-toolchain-cpp
```
