Title
Implement real agent installations for OpenHands, OpenCode, Plandex (full and slim)

Executive Summary
This v4 specification builds on v3 and replaces stub wrappers with actual installations for
OpenHands (Python via uv), OpenCode (Node via npm), and Plandex (Go-built binary via git clone
+ go build). It preserves all v3 architectural invariants (CLI wiring, PATH policy, entrypoint
contract, non-root execution), tightens best practices (enterprise CA handling, cleanup, multi-arch
friendliness, reproducibility), and details Dockerfile stage recipes and Makefile targets for
consistent full/slim images without changing established user-visible strings.

Goals
- Replace OpenHands/OpenCode/Plandex stubs with real agent installations.
- Keep shims-first PATH policy for these agents; preserve codex/crush exceptions.
- Maintain entrypoint invariants, non-root execution, and toolchain compatibility.
- Provide full and slim image pairs for each agent; consistent naming and publication flows.
- Ensure enterprise CA compatibility across npm, uv, git/go, and curl during install steps.
- Keep tests preview-only; no network pulls or container runtime in test lanes.

Non-goals
- Add agent-specific flags or alter UX beyond established patterns.
- Change CLI wiring for existing agents (codex/crush/aider).
- Introduce privileged capabilities or change mounts/entrypoint invariants.

Terminology
- Full image: runtime with editors (emacs-nox, vim, nano, mg, nvi) and ripgrep.
- Slim image: minimal runtime (mg, nvi only).
- Registry prefix: preferred_registry_prefix[_quiet], normalized to "<host>/" when present.

References
- v3 spec for OpenHands/OpenCode/Plandex integration points and wiring.
- CONVENTIONS.md (≤100 columns guidance, exhaustive matches).
- Existing Dockerfile/Makefile patterns for codex/crush/aider (CA handling, cleanup).
- uv tool install docs; npm global install guidance; Go build best practices.

Architecture and installation overview
- OpenCode (Node):
  - Install via: npm i -g opencode-ai@latest.
  - Enterprise CA during install:
    NODE_EXTRA_CA_CERTS=/run/secrets/migros_root_ca (when present),
    NODE_OPTIONS="--use-openssl-ca",
    SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt,
    SSL_CERT_DIR=/etc/ssl/certs,
    CURL_CA_BUNDLE=/etc/ssl/certs/ca-certificates.crt.
  - Post-install cleanup:
    npm prune --omit=dev; npm cache clean --force; remove npm/npx/yarn symlinks in slim images;
    wipe docs/locales and apt caches when KEEP_APT=0.
  - Binary resolution: /usr/local/bin/opencode (npm global).

- OpenHands (Python via uv):
  - Install uv: curl -LsSf https://astral.sh/uv/install.sh; place binary at /usr/local/bin/uv.
  - Tool install:
    UV_TOOL_DIR="/usr/local/bin" so uv manages a shim/symlink at /usr/local/bin/openhands.
    uv tool install --python 3.12 --from openhands-ai openhands.
  - TLS and enterprise CA during install:
    UV_NATIVE_TLS=1; REQUESTS_CA_BUNDLE, CURL_CA_BUNDLE, SSL_CERT_FILE, SSL_CERT_DIR=/etc/ssl/certs set to consolidated CA;
    ensure curl for uv installer runs with CA env set; inject corporate CA via BuildKit secret only during RUN steps; remove afterward.
  - Post-install cleanup:
    remove /root/.cache/uv and pip caches; wipe docs/locales and apt caches when KEEP_APT=0.

- Plandex (Go):
  - Builder stage: golang:1.22-bookroom with CA env for git/curl (GIT_SSL_CAINFO=/etc/ssl/certs/ca-certificates.crt, SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt, CURL_CA_BUNDLE=/etc/ssl/certs/ca-certificates.crt).
    git clone https://github.com/plandex-ai/plandex.git.
    Build in plandex/app/cli:
      CGO_ENABLED=0; GOFLAGS="-trimpath -mod=readonly";
      LDFLAGS="-s -w -X plandex/version.Version=$(cat version.txt)".
    Multi-arch: honor TARGETOS/TARGETARCH (buildx) by setting GOOS/GOARCH accordingly.
    Output binary: /out/plandex.
    Cleanup builder caches: rm -rf /root/go/pkg /go/pkg/mod.
  - Runtime stage (base/base-slim):
    COPY --from=plandex-builder /out/plandex /usr/local/bin/plandex.
    Post-copy cleanup identical to other agents; keep non-root contract.

Phased Implementation Plan

Phase 0 — Validation and preparatory decisions
- Agent binaries:
  - openhands, opencode, plandex in /usr/local/bin.
- PATH policy:
  - Shims-first for openhands/opencode/plandex:
    /opt/aifo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:$PATH.
  - codex/crush keep node-first branch; aider adds venv handling.
- Non-root execution and entrypoint invariants:
  - Preserve dumb-init and aifo-entrypoint (HOME/GNUPGHOME/XDG runtime, pinentry-curses,
    gpg-agent launch).
- Enterprise CA handling (DPI-friendly):
  - Use BuildKit secret id=migros_root_ca mounted at /run/secrets/migros_root_ca during RUN steps; set TLS env vars (SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt, SSL_CERT_DIR=/etc/ssl/certs, CURL_CA_BUNDLE=/etc/ssl/certs/ca-certificates.crt, REQUESTS_CA_BUNDLE=/etc/ssl/certs/ca-certificates.crt); for Node add NODE_EXTRA_CA_CERTS=/run/secrets/migros_root_ca and NODE_OPTIONS="--use-openssl-ca"; for Git add GIT_SSL_CAINFO=/etc/ssl/certs/ca-certificates.crt; for uv set UV_NATIVE_TLS=1.
  - Optionally install the secret CA as /usr/local/share/ca-certificates/migros-root-ca.crt and run update-ca-certificates; delete the file and rerun update-ca-certificates immediately after the step to avoid persistence.
  - Apply this pattern to npm installs (OpenCode), curl+uv installs (OpenHands), and git clone/go build (Plandex).

Phase 1 — CLI wiring (no changes if v3 landed)
- src/cli.rs:
  - Agent::OpenHands/OpenCode/Plandex subcommands with trailing_var_arg=true.
- src/main.rs:
  - resolve_agent_and_args mapping to "openhands", "opencode", "plandex".
- src/warnings.rs:
  - maybe_warn_missing_toolchain_agent/for_fork include these agents.
- Validation:
  - Help output lists agents; mapping drives docker run wiring correctly.

Phase 2 — Images command output (no changes)
- src/commands/mod.rs::run_images:
  - Emits codex, crush, aider, openhands, opencode, plandex on stderr (color) and stdout (plain).
- src/agent_images.rs:
  - default_image_for(agent) composes "<prefix>-<agent>{-slim}:{tag}" with prefix/env overrides.

Phase 3 — Docker run wiring / PATH policy (no changes)
- src/docker.rs::build_docker_cmd:
  - Shims-first PATH branch for openhands/opencode/plandex; node-first for codex/crush; venv path
    inclusion for aider.
- Validation:
  - docker previews reflect agent strings in name/hostname and PATH policy.

Phase 4 — Dockerfile images: real agent installation
- Full images (base):
  - OpenCode:
    RUN step with CA envs:
      npm install -g --omit=dev --no-audit --no-fund --no-update-notifier --no-optional \
      opencode-ai@latest.
    ENV PATH="/opt/aifo/bin:${PATH}" after install.
    Cleanup when KEEP_APT=0:
      drop apt/procps; npm prune; npm cache clean; remove npm/npx/yarn symlinks; wipe docs/locales.

  - OpenHands:
    Install uv to /usr/local/bin/uv; set UV_NATIVE_TLS and CA envs.
    UV_TOOL_DIR="/usr/local/bin" uv tool install --python 3.12 --from openhands-ai openhands.
    ENV PATH="/opt/aifo/bin:${PATH}" after install.
    Cleanup when KEEP_APT=0:
      drop apt/procps; remove uv/pip caches; wipe docs/locales and apt caches.

  - Plandex:
    Builder: FROM golang:1.22-bookworm AS plandex-builder with CA envs; git clone; GOOS/GOARCH from
    TARGETOS/TARGETARCH; CGO_ENABLED=0; LDFLAGS inject version from version.txt; output /out/plandex.
    Runtime: FROM base; COPY the binary; ENV PATH; cleanup when KEEP_APT=0 as above.

- Slim images (base-slim):
  - Repeat installations with slim cleanup:
    - OpenCode slim: same npm install; remove curl when KEEP_APT=0; prune npm and caches.
    - OpenHands slim: uv install; remove curl when KEEP_APT=0; cleanup caches.
    - Plandex slim: copy binary from builder; remove curl when KEEP_APT=0; cleanup caches.

- Entrypoint and non-root:
  - Preserve dumb-init and aifo-entrypoint; do not introduce root-only writes to /workspace.

- Versioning and reproducibility:
  - Allow ARG overrides:
    - OpenCode: ARG OPCODE_VERSION=latest; install opencode-ai@"${OPCODE_VERSION}".
    - OpenHands: ARG OPENHANDS_CONSTRAINT (optional constraint passed to uv tool install).
    - Plandex: ARG PLX_GIT_REF (default main); checkout tag/commit when provided.
  - Defaults remain latest/main; document overrides in Makefile targets.

Phase 5 — Makefile targets and publish flows
- Build targets:
  - build-openhands, build-openhands-slim
  - build-opencode,  build-opencode-slim
  - build-plandex,   build-plandex-slim
- Rebuild variants: rebuild-* for no-cache builds.
- Publish targets:
  - publish-openhands{,-slim}, publish-opencode{,-slim}, publish-plandex{,-slim}.
  - Respect REGISTRY/REGISTRY_PREFIX/TAG/FLAVOR; tag both local and registry-prefixed refs.
- Multi-arch:
  - With PLATFORMS and PUSH set, use buildx; Go builder honors TARGETOS/TARGETARCH.

Phase 6 — Tests (preview-only)
- CLI parsing smoke (optional) unchanged.
- run_images output:
  - Assert six agents on stdout; reflect flavor/registry behavior.
- Docker preview assertions:
  - PATH includes /opt/aifo/bin for openhands/opencode/plandex.
  - Name/hostname include agent strings; env passthrough and mounts unchanged.
- No network pulls; deterministic previews and images output only.

Phase 7 — Documentation
- README:
  - Subcommands include openhands/opencode/plandex; dry-run examples; image naming/flavor/registry
    overrides; shims-first PATH policy for the three agents.
  - Note OpenHands via uv-managed Python; OpenCode via npm; Plandex is a Go binary built from source.
- AGENT.md process remains unchanged; CHANGES.md and SCORE.md updates upon landing.

Best Practices (images and builds)
- Multi-stage:
  - Dedicated Go builder for Plandex; uv install in runtime stages; no compilers in final images.
- CA handling:
  - Inject enterprise CA via secret only during RUN steps; set TLS env vars; remove CA afterward.
- Cleanup and footprint:
  - KEEP_APT=0 drops apt/procps; prune npm; clear npm/uv/pip caches; remove heavy docs/locales.
  - Prefer CGO_ENABLED=0 and -s -w LDFLAGS for smaller Go binaries.
- PATH policy:
  - Maintain /opt/aifo/bin first for openhands/opencode/plandex; node-first for codex/crush; aider
    includes venv path.
- Security posture:
  - No privileged mode; no host Docker socket; AppArmor-compatible; minimal mounts; non-root runs.
- Entrypoint invariants:
  - HOME/GNUPGHOME/XDG runtime; pinentry-curses configured; gpg-agent launched; prefer TTY for
  pinentry when interactive.

Code Integration Checklist
- src/cli.rs: Agent::OpenHands/OpenCode/Plandex with trailing_var_arg=true.
- src/main.rs: resolve_agent_and_args mapping present.
- src/warnings.rs: include agents in coding agent set for guidance prompts.
- src/commands/mod.rs: images output includes all six agents.
- src/docker.rs: shims-first PATH for three agents; codex/crush node-first; aider venv added.
- Dockerfile:
  - Replace stub stages with real installations:
    - openhands/openhands-slim via uv tool install; /usr/local/bin symlink.
    - opencode/opencode-slim via npm install -g opencode-ai.
    - plandex/plandex-slim via Go builder and runtime COPY.
  - Maintain cleanup and entrypoint invariants.
- Makefile: add build/rebuild/publish targets for three agents (full/slim).
- README: document agents, examples, overrides.

Acceptance Criteria
- aifo-coder openhands -- --help (dry-run) shows preview with correct image ref.
- aifo-coder opencode  -- --help (dry-run) shows preview with correct image ref.
- aifo-coder plandex   -- --help (dry-run) shows preview with correct image ref.
- aifo-coder images prints six agents (codex, crush, aider, openhands, opencode, plandex).
- PATH policy shims-first visible in previews for openhands/opencode/plandex.
- Build validations:
  - docker build/buildx succeed for full/slim stages locally.
  - With images present, --help executes successfully in containers; entrypoint prepares GNUPGHOME/
    XDG runtime; OpenHands via uv, OpenCode via npm, Plandex binary runs.
- No regressions in existing tests; no network pulls in tests.

Risks and Mitigations
- PATH mismatch or runtime needs:
  - Start shims-first; if an agent requires node-first, adjust docker.rs like codex/crush.
- Image availability delays:
  - Override via --image or environment; tests stay preview-only.
- uv assumptions:
  - Ensure UV_TOOL_DIR=/usr/local/bin; UV_NATIVE_TLS=1; CA envs set; cleanup caches.
- Plandex multi-arch and reproducibility:
  - Use dedicated Go builder; GOOS/GOARCH from buildx; inject version via ldflags; CGO off.
- Global npm reliability:
  - Set CA envs; prune and clean caches; in slim images remove npm/npx for smaller footprint.
- Corporate proxy behavior:
  - Prefer consolidated CA bundle; configure NODE_OPTIONS/SSL_CERT_FILE; remove CA after step.

Appendix A — Installation recipes (indicative)
- OpenCode:
  npm install -g --omit=dev --no-audit --no-fund --no-update-notifier --no-optional opencode-ai@latest
- OpenHands:
  curl -LsSf https://astral.sh/uv/install.sh; install uv to /usr/local/bin/uv
  UV_TOOL_DIR=/usr/local/bin uv tool install --python 3.12 --from openhands-ai openhands
- Plandex:
  Builder: golang:1.22-bookworm; git clone; GOOS/GOARCH from buildx; CGO_ENABLED=0
  go build -ldflags "-s -w -X plandex/version.Version=$(cat version.txt)" -o /out/plandex
  Runtime: COPY to /usr/local/bin/plandex

Appendix B — Makefile targets (indicative)
- build-openhands / build-openhands-slim
- build-opencode  / build-opencode-slim
- build-plandex   / build-plandex-slim
- rebuild-* (no-cache) and publish-* targets mirroring existing agent flows.

Appendix C — Documentation changes (concise)
- README “Subcommands”:
  - openhands [args...]  Run OpenHands inside container
  - opencode  [args...]  Run OpenCode inside container
  - plandex   [args...]  Run Plandex inside container
- Dry-run examples; image naming/flavor/registry overrides; shims-first policy notes.
- OpenHands via uv-managed Python; OpenCode via npm; Plandex is a Go binary built from source.
