Title
Implement three additional CLI coding agents: OpenHands, OpenCode, Plandex.

Summary
Extend aifo-coder to support three new containerized coding agents (OpenHands, OpenCode, Plandex) alongside Aider, Crush and Codex. Work spans: CLI wiring, images output, PATH policy, documentation, and—critically—the creation and publication of six agent images (full and slim variants). Preserve existing security posture, user-visible strings, and best practices.

Goals
- New CLI subcommands: openhands, opencode, plandex
- Container images:
  - aifo-coder-openhands and aifo-coder-openhands-slim
  - aifo-coder-opencode and aifo-coder-opencode-slim
  - aifo-coder-plandex and aifo-coder-plandex-slim
- Registry selection, flavor handling, and overrides identical to existing agents
- Full compatibility with toolchain sidecars, proxy and shims; no special casing required
- Previews and images command reflect new agents; tests remain preview-only (no network pulls)

Non-goals
- Implement agent-specific UX or flags beyond current patterns
- Deliver agent source code; we only package container runtime and launcher integration
- Change toolchain/shim/proxy behavior; reuse existing architecture

References
- spec/aifo-coder-implement-openhands-opencode-plandex-v1.spec (baseline)
- CONVENTIONS.md (style, no dead code, ≤100 columns preferred)
- README.md (runtime/entrypoint expectations, security posture)

Phased Plan

Phase 0 — Architecture and Image Requirements (planning)
- Agent binaries in PATH:
  - /usr/local/bin/openhands, /usr/local/bin/opencode, /usr/local/bin/plandex
- PATH policy:
  - Default (shims-first): /opt/aifo/bin early, then system paths; use for all three initially.
  - Node-first branch remains limited to Codex/Crush unless a new agent requires it explicitly.
- Base OS: Debian Bookworm slim for predictable CA/curl/openssl behavior.
- Non-root execution:
  - Runtime uses docker --user UID:GID; image should include a “coder” user and HOME=/home/coder prepared by entrypoint.
- Entrypoint contract (aifo-entrypoint-equivalent):
  - HOME and GNUPGHOME set; XDG_RUNTIME_DIR prepared; pinentry-curses configured; gpg-agent launched (loopback pinentry).
- Security posture:
  - No privileged mode; no host Docker socket; compatible with AppArmor.
- Dependencies (full vs slim):
  - Shared minimum: curl, ca-certificates, bash/dash/sh, coreutils, gpg, pinentry-curses, git.
  - Full only: editors (emacs-nox, vim, nano, mg, nvi), ripgrep.
  - Slim: mg, nvi only (mirror README).
- Optional libs: libnss-wrapper available to support fallback identity, but runtime relies on docker --user.

Phase 1 — CLI wiring (repo code)
- src/cli.rs:
  - Add subcommands:
    - Agent::OpenHands { args: Vec<String> }
    - Agent::OpenCode  { args: Vec<String> }
    - Agent::Plandex   { args: Vec<String> }
  - Use trailing_var_arg = true to pass through agent args verbatim.
  - Docstrings: concise description lines like existing agents.
- src/main.rs:
  - resolve_agent_and_args: map OpenHands→"openhands", OpenCode→"opencode", Plandex→"plandex".
  - Ensure warnings and startup sequences include these agents where applicable.
- src/warnings.rs:
  - Extend maybe_warn_missing_toolchain_agent and maybe_warn_missing_toolchain_for_fork:
    - Include new agent names in the “coding agent” set checked for toolchain sidecar guidance.

Phase 2 — Images command output (repo code)
- src/commands/mod.rs::run_images:
  - Add effective image refs to stderr (colored) and machine-readable stdout lines for:
    - openhands <ref>
    - opencode  <ref>
    - plandex   <ref>
- src/agent_images.rs:
  - No functional changes required; default_image_for(agent) already composes <prefix>-<agent>{-slim}:<tag> with registry prefix auto-selection.

Phase 3 — Docker run wiring / PATH policy (repo code)
- src/docker.rs::build_docker_cmd:
  - Keep default PATH branch for openhands/opencode/plandex:
    - "/opt/aifo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:$PATH"
  - Codex/Crush remain on node-first branch.
  - All env mappings, mounts, entrypoint contract and sh wrapper behavior unchanged.

Phase 4 — Images: build and publish (ops deliverable, code-owned Dockerfile/Makefile)
- Naming scheme:
  - ${REGISTRY_PREFIX}${IMAGE_PREFIX}-${AGENT}${SUFFIX}:${TAG}
  - IMAGE_PREFIX defaults to aifo-coder; SUFFIX is -slim for slim variants; TAG defaults to latest.
- Base stages:
  - Full variants: derive from “base” (Node bookworm slim with editors/tools) like Codex/Crush.
  - Slim variants: derive from “base-slim” (minimal tools; mg, nvi only).
- Install agent binaries:
  - Place executables at /usr/local/bin/<agent>; method depends on agent distribution model:
    - Node CLI: npm install -g <package>; ensure NODE_EXTRA_CA_CERTS for enterprise CA if needed.
    - Prebuilt tar/zip: fetch/verify, install into /usr/local/bin.
    - Python CLI: venv-based (like Aider) if required, but prefer standalone binary to keep symmetry.
- PATH:
  - Ensure "/opt/aifo/bin" present and readable; final runtime PATH includes it (added by top layer).
- Entrypoint:
  - Reuse existing /usr/local/bin/aifo-entrypoint with GPG and XDG runtime prep; attach dumb-init.
- Security hardening and cleanup:
  - DROP apt/procps, npm caches, docs/locales in final stage if KEEP_APT=0 (mirroring existing Codex/Crush slim/full cleanups).
- Publish:
  - Build and tag both full and slim images; push to configured registry (REGISTRY or AIFO_CODER_REGISTRY_PREFIX) via CI.
- Verification:
  - The agent CLI must execute “--help” without external dependencies; container entrypoint works and gpg-agent starts cleanly.

Phase 5 — Tests (repo code; preview-only)
- CLI parsing: minimal smoke tests for new subcommands if desired.
- run_images: assert new stdout lines exist and reflect registry/flavor handling.
- build_docker_cmd previews:
  - Verify --name/--hostname reflect agent string; PATH includes /opt/aifo/bin; env passthrough and mounts consistent.

Phase 6 — Documentation
- README.md:
  - List OpenHands, OpenCode, Plandex in “Containerized launcher” section.
  - Add brief usage examples; reaffirm image naming, flavor, registry overrides.
- AGENT.md (process guidance):
  - After merging, update CHANGES.md top entry with date/email, short summary; handle SCORE.md per instructions.

Phase 7 — Rollout and compatibility
- Backward-compatible; no behavior changes for existing agents.
- Overrides supported:
  - AIFO_CODER_IMAGE to force a single image ref
  - AIFO_CODER_IMAGE_PREFIX/TAG/FLAVOR and AIFO_CODER_REGISTRY_PREFIX
- Fallback:
  - Users can override with --image in CLI until published images exist.

Implementation Guidance (Dockerfile additions)
- Full images (new stages; mirroring Codex/Crush):
  - FROM base AS openhands
    - Install agent binary (npm -g or copy prebuilt)
    - ENV PATH="/opt/aifo/bin:${PATH}"
    - ARG KEEP_APT=0
    - If KEEP_APT=0: drop procps and apt, prune caches; mirror codex/crush cleanup
  - FROM base AS opencode (same pattern)
  - FROM base AS plandex (same pattern)
- Slim images (new stages; mirroring codex-slim/crush-slim):
  - FROM base-slim AS openhands-slim
    - Install agent binary
    - ENV PATH="/opt/aifo/bin:${PATH}"
    - ARG KEEP_APT=0
    - If KEEP_APT=0: drop procps and apt/curl; prune caches; mirror codex-slim/crush-slim cleanup
  - Repeat for opencode-slim and plandex-slim.

Implementation Guidance (Makefile targets)
- Add build targets:
  - build-openhands, build-opencode, build-plandex
  - build-openhands-slim, build-opencode-slim, build-plandex-slim
- Add rebuild (no-cache) targets similarly.
- Integrate into “build” and “build-slim/build-fat” aggregates if desired, or keep separate for staged rollout.
- Registry auto-detection (curl → repository.migros.net) should be reused exactly like current targets.

Best Practices (coding style and images)
- Adhere to line length guidance (~≤100 chars); keep golden strings intact.
- Prefer multi-stage builds; final runtime minimal; avoid compilers in final.
- Drop apt/procps in final layers unless KEEP_APT=1; clean npm caches aggressively.
- Provide minimal editors in slim images (mg, nvi) per README.
- Preserve entrypoint invariants and dumb-init; avoid root-only states under /workspace.

Code Integration Checklist (repo)
- src/cli.rs: add Agent variants (three new subcommands).
- src/main.rs: extend resolve_agent_and_args; ensure warning flows include new agents.
- src/warnings.rs: expand agent set checked for toolchain guidance.
- src/commands/mod.rs: add three images lines to stderr and stdout machine-readable output.
- src/docker.rs: confirm PATH branch assignment (shims-first) for new agents.
- README.md: update lists and examples for new agents.

Acceptance Criteria
- aifo-coder openhands -- --help (dry-run) prints a docker preview with correct image ref.
- aifo-coder opencode -- --help (dry-run) prints a docker preview with correct image ref.
- aifo-coder plandex -- --help (dry-run) prints a docker preview with correct image ref.
- aifo-coder images prints six agents (codex, crush, aider, openhands, opencode, plandex).
- New agents show shims-first PATH in preview.
- No regressions in existing tests; no added network pulls.
- Published full/slim images start and “--help” succeeds; entrypoint prepares GPG and runtime consistently.

Risks and Mitigations
- PATH mismatch for a new agent (e.g., Node-first required):
  - Start shims-first; if needed, explicitly assign node-first branch in docker.rs like codex/crush.
- Image availability delays:
  - CLI supports --image override; tests remain preview-only.
- Runtime differences (Node/Python):
  - Prefer standalone binary or npm global for uniformity; if Python-based, follow Aider venv model but keep minimal dependencies.

Observability
- Reuse existing stderr info logs (image, registry, PATH); no telemetry changes.

Rollout Plan
- Land code changes and tests.
- Add Dockerfile stages and Makefile targets; build and publish six images.
- Update README; announce availability.
- Iterate PATH policy or packaging per agent feedback (keep changes minimal and documented).

Appendix — Example Makefile entries (indicative)
- build-openhands:
  - Mirror build-codex: detect registry via curl, set RP, tag ${IMAGE_PREFIX}-openhands:${TAG} and registry-tag when REGISTRY present.
- build-openhands-slim:
  - Mirror build-codex-slim pattern.
- Repeat for opencode/plandex; add rebuild-* and optional publish-* variants.

End of spec.
