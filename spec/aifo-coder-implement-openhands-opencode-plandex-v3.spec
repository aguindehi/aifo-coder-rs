Title
Implement three additional CLI coding agents: OpenHands, OpenCode, Plandex.

Executive Summary
This v3 specification validates the v2 plan, fixes gaps, and provides a coherent, consistent,
comprehensive phased implementation for three new agents (OpenHands, OpenCode, Plandex) alongside
Aider, Crush, Codex. The work has two tracks:
- Code integration: CLI wiring, docker run previews and images output, PATH policy, warnings,
  and documentation.
- Images: creation and publication of six images (full and slim), with consistent security posture
  and entrypoint contracts.

Validation of v2 and gaps addressed
- Image creation: v2 correctly included image build/publish; v3 clarifies Dockerfile stage names,
  Makefile targets, and publication steps for all six images.
- CLI wiring: v2 listed modules; v3 enumerates exact additions and validation criteria.
- PATH policy: v2 chose shims-first for new agents; v3 adds explicit preview checks and acceptance.
- Documentation: v2 required updates; v3 specifies concrete sections and wording constraints.
- Tests: v2 emphasized preview-only; v3 defines assertions for images output and docker previews.
- Consistency with existing code: v3 aligns naming with current agent_images, docker.rs policies,
  images output patterns, and Makefile/Dockerfile conventions.

Goals
- New CLI subcommands: openhands, opencode, plandex
- Container images:
  - aifo-coder-openhands and aifo-coder-openhands-slim
  - aifo-coder-opencode and aifo-coder-opencode-slim
  - aifo-coder-plandex and aifo-coder-plandex-slim
- Registry selection, flavor handling, and overrides identical to existing agents
- Full compatibility with toolchain sidecars, proxy and shims (no special casing required)
- Previews and images command reflect new agents; tests remain preview-only (no pulls)

Non-goals
- Implementing agent-specific UX or custom flags beyond precedent
- Shipping agent source code; focus is container packaging and integration
- Changing toolchain/shim/proxy behavior; reuse architecture and user-visible strings

Terminology
- Full image: featureful runtime including editors (emacs-nox, vim, nano, mg, nvi) and ripgrep
- Slim image: minimized runtime retaining required dependencies (mg, nvi)
- Registry prefix: auto-detected by preferred_registry_prefix[_quiet], normalized to "<host>/"

References
- v1/v2 specifications for OpenHands/OpenCode/Plandex
- CONVENTIONS.md (≤100 columns preference, style, no dead code)
- README.md (security posture, entrypoint expectations, PATH policy)

Phased Implementation Plan

Phase 0 — Architecture and image requirements (planning)
- Agent binaries:
  - Place executables at /usr/local/bin/openhands, /usr/local/bin/opencode, /usr/local/bin/plandex.
- PATH policy:
  - Default shims-first branch: /opt/aifo/bin at front; start with this for all three agents.
  - Node-first branch remains limited to Codex/Crush unless an agent requires it (to be validated).
- OS base: Debian Bookworm slim (predictable CA/curl/openssl).
- Non-root execution:
  - Runtime uses docker --user UID:GID; images include user “coder” and HOME=/home/coder prepared.
- Entrypoint contract:
  - Set HOME and GNUPGHOME; prepare XDG_RUNTIME_DIR; configure pinentry-curses; launch gpg-agent.
- Security posture:
  - No privileged mode; no host Docker socket; compatible with AppArmor; minimal mounts.
- Dependencies:
  - Shared minimum: curl, ca-certificates, bash/dash/sh, coreutils, gpg, pinentry-curses, git, libnss-wrapper.
  - Full: editors (emacs-nox, vim, nano, mg, nvi), ripgrep.
  - Slim: mg, nvi only.

Phase 1 — CLI wiring (repo code)
- src/cli.rs:
  - Add subcommands:
    - OpenHands { args: Vec<String> }
    - OpenCode  { args: Vec<String> }
    - Plandex   { args: Vec<String> }
  - trailing_var_arg=true for pass-through of agent arguments; one-line docs mirroring existing agents.
- src/main.rs:
  - resolve_agent_and_args: map Agent::OpenHands→"openhands", OpenCode→"opencode", Plandex→"plandex".
  - Ensure startup warnings include these agents (calls into warnings module).
- src/warnings.rs:
  - maybe_warn_missing_toolchain_agent and maybe_warn_missing_toolchain_for_fork:
    - include "openhands", "opencode", "plandex" in the “coding agent” set for guidance.
- Validation:
  - Help output lists three new agents.
  - resolve_agent_and_args returns proper agent strings for docker run wiring.

Phase 2 — Images command output (repo code)
- src/commands/mod.rs::run_images:
  - Append colored stderr lines and machine-readable stdout lines for three agents:
    - openhands <ref>, opencode <ref>, plandex <ref>
  - Keep colorization and registry/flavor display consistent.
- src/agent_images.rs:
  - No changes; default_image_for(agent) composes "<prefix>-<agent>{-slim}:{tag}" and respects registry prefix/env overrides.
- Validation:
  - Images output includes codex, crush, aider, openhands, opencode, plandex on both stderr and stdout.

Phase 3 — Docker run wiring / PATH policy (repo code)
- src/docker.rs::build_docker_cmd:
  - Use default shims-first PATH branch for openhands/opencode/plandex:
    "/opt/aifo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:$PATH"
  - Keep codex/crush node-first branch; aider includes venv path handling.
  - No changes to env passthrough, mounts, entrypoint, or sh wrapper behavior.
- Validation:
  - docker preview shows /opt/aifo/bin at front for the three agents.
  - --name/--hostname include the agent string in container naming.

Phase 4 — Images: build and publish (Dockerfile/Makefile)
- Dockerfile stages (add per-agent stages mirroring codex/crush patterns):
  - FROM base      AS openhands       (install agent binary; ENV PATH="/opt/aifo/bin:${PATH}"; KEEP_APT cleanup)
  - FROM base      AS opencode
  - FROM base      AS plandex
  - FROM base-slim AS openhands-slim  (install agent; ENV PATH; KEEP_APT slim cleanup)
  - FROM base-slim AS opencode-slim
  - FROM base-slim AS plandex-slim
- Installation patterns:
  - Node CLI: npm install -g --omit=dev --no-audit --no-fund --no-update-notifier --no-optional <package>.
    - Ensure NODE_EXTRA_CA_CERTS and NODE_OPTIONS="--use-openssl-ca" when enterprise CA present.
  - Prebuilt tar/zip: fetch/verify, install into /usr/local/bin/<agent>.
  - Python CLI (if applicable): prefer standalone binary; if venv-based, mirror Aider (uv + /opt/venv).
- Keep consistent cleanup:
  - When KEEP_APT=0, remove apt/procps and caches; prune npm; wipe docs/locales; preserve curl in full only.
- Makefile targets (mirroring codex/crush/aider):
  - build-openhands, build-openhands-slim
  - build-opencode,  build-opencode-slim
  - build-plandex,   build-plandex-slim
  - rebuild-* and publish-* variants; registry handling identical to existing targets.
- Publish:
  - Tag both local and registry-prefixed refs when REGISTRY present; support buildx multi-arch with PLATFORMS+PUSH.
- Validation:
  - docker build/buildx succeeds for new stages locally; no run required for tests.
  - With images present, agent “--help” executes successfully and entrypoint prepares GNUPGHOME/XDG runtime.

Phase 5 — Tests (preview-only; no pulls)
- CLI parsing smoke (optional): verify new subcommands parse and resolve agent string mapping.
- run_images: assert new stdout lines exist and reflect flavor/registry behavior.
- build_docker_cmd previews:
  - PATH includes /opt/aifo/bin; --name/--hostname include agent string; env passthrough/mounts unchanged.
- No network pulls; use deterministic preview output comparisons.

Phase 6 — Documentation
- README.md:
  - Subcommands: add openhands/opencode/plandex entries.
  - Usage examples: dry-run “--help” for each.
  - Image naming/flavor/registry overrides: highlight shims-first PATH for the three agents.
- AGENT.md process:
  - CHANGES.md entry at top (YYYY-MM-DD, author/email, short summary ≤80 chars, followed by list).
  - Score process: rename SCORE.md→SCORE-before.md; write comprehensive scoring to SCORE.md.
- Preserve golden strings; avoid altering exact messages tested elsewhere.

Phase 7 — Rollout and compatibility
- Backward-compatible; no changes for existing agents or toolchains.
- Overrides supported:
  - AIFO_CODER_IMAGE (force a single image), AIFO_CODER_IMAGE_PREFIX/TAG/FLAVOR, AIFO_CODER_REGISTRY_PREFIX.
- Fallback:
  - Users can run with --image before images are published.
- Observability/logging:
  - Reuse existing stderr info lines; no new telemetry.

Best Practices (images)
- Multi-stage builds; keep runtime minimal.
- Drop apt/procps and clean caches by default (KEEP_APT=0).
- Provide mg,nvi in slim; full adds emacs-nox,vim,nano,ripgrep.
- Preserve entrypoint invariants and dumb-init; avoid root-owned writes in /workspace.

Code Integration Checklist (repo)
- src/cli.rs: add Agent::OpenHands/OpenCode/Plandex; trailing_var_arg=true; docs.
- src/main.rs: extend resolve_agent_and_args; ensure toolchain warnings invoked for new agents.
- src/warnings.rs: expand “coding agent” set to include new names.
- src/commands/mod.rs: append three images to stderr and stdout lists.
- src/docker.rs: confirm PATH shims-first for new agents; codex/crush node-first unchanged.
- README.md: document new agents and examples.

Acceptance Criteria
- aifo-coder openhands -- --help (dry-run) shows docker preview with correct image ref.
- aifo-coder opencode  -- --help (dry-run) shows docker preview with correct image ref.
- aifo-coder plandex   -- --help (dry-run) shows docker preview with correct image ref.
- aifo-coder images prints six agents (codex, crush, aider, openhands, opencode, plandex).
- PATH policy shims-first for new agents is visible in previews.
- No regressions in existing tests; no new pulls in tests.
- Published full/slim images start and “--help” succeeds; entrypoint prepares GNUPGHOME/XDG runtime.

Risks and Mitigations
- PATH mismatch (node-first requirement for a new agent):
  - Begin shims-first; if an agent needs node-first, update docker.rs match to include it (like codex/crush).
- Image availability delays:
  - Use --image or AIFO_CODER_IMAGE override; tests remain preview-only.
- Runtime differences (Node/Python/binary):
  - Prefer standalone binary or npm global; if venv required, mirror Aider’s minimal-PEP668 approach.

Appendix A — Dockerfile stage patterns (illustrative)
- Full:
  FROM base AS openhands
    # npm install -g <openhands package>; ENV PATH="/opt/aifo/bin:${PATH}"; ARG KEEP_APT
    # cleanup when KEEP_APT=0
  FROM base AS opencode
  FROM base AS plandex
- Slim:
  FROM base-slim AS openhands-slim
  FROM base-slim AS opencode-slim
  FROM base-slim AS plandex-slim
  # ENV PATH; cleanup removes curl in slim when KEEP_APT=0 (mirror codex-slim/crush-slim)

Appendix B — Makefile targets (indicative)
- build-openhands / build-openhands-slim
- build-opencode  / build-opencode-slim
- build-plandex   / build-plandex-slim
- rebuild-* and publish-* variants; registry prefix handling same as existing targets.

Appendix C — Documentation changes (concise)
- README “Subcommands”:
  - openhands [args...]  Run OpenHands inside container
  - opencode  [args...]  Run OpenCode inside container
  - plandex   [args...]  Run Plandex inside container
- Add short dry-run examples and mention image naming/registry/flavor overrides.

End of specification.
