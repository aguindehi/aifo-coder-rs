# Spec: Split image registries into internal vs mirror and remove obsolete fallbacks

Context
- Today a single “registry prefix” is used for both:
  - Pulling official upstream base images (Node, Rust, Python, Debian, etc.).
  - Pulling and pushing our own aifo-coder images.
- We have two distinct registries with different roles:
  - Mirror registry (external upstream, cached): repository.migros.net
  - Internal registry (first-party, ours): registry.intern.migros.net
- Using the mirror for our own images fails; we must split responsibilities.
- We must not keep any fallback paths for the obsolete AIFO_CODER_REGISTRY_PREFIX.

Goals
- At build time (Docker builds): use the mirror registry only for base image pulls.
- At runtime (launcher/CLI): use the internal registry for our own images.
- In CI: push images only to the internal registry.
- Remove any dependence on AIFO_CODER_REGISTRY_PREFIX (ignore if set).

Non-goals
- Changing Dockerfile base ARG name or base selection behavior (keep ARG REGISTRY_PREFIX).
- Supporting Docker Hub for push. We either push to internal or not at all.

Terminology
- Internal Registry (IR): registry.intern.migros.net
- Mirror Registry (MR): repository.migros.net
- “Our images”: aifo-coder-* agents, toolchain sidecars, rust-builder.
- “Base images”: upstream images FROM’d in Dockerfiles (node, rust, python, golang, etc.).

Environment variables
- AIFO_CODER_INTERNAL_REGISTRY_PREFIX
  - Used by the launcher at runtime to pull our images.
  - If non-empty: must be normalized to end with a single “/”.
  - If empty or unset: no prefix is used (local tag or explicit --image is required).
- AIFO_CODER_MIRROR_REGISTRY_PREFIX (optional)
  - Developer override for build tooling (Makefile/scripts) to point base pulls elsewhere.
  - Not used by the runtime launcher; launcher does not pull base images.
- AIFO_CODER_REGISTRY_PREFIX
  - Obsolete. Must be ignored entirely (no alias/fallback).

Invariants and behavior
- Launcher/runtime
  - Image references for agents/toolchains must be prefixed with the Internal Registry, if set.
  - The launcher must not probe the MR for runtime pulls (that is a build-time concern).
  - Verbose diagnostics (doctor or --verbose runs) must print both registries and their sources.
- Build-time
  - Dockerfile base pulls use ARG REGISTRY_PREFIX which points to the MR (repository.migros.net/).
  - Local builds may optionally set AIFO_CODER_MIRROR_REGISTRY_PREFIX; Makefile/scripts map that
    to ARG REGISTRY_PREFIX, or auto-probe the MR reachability when unset (current behavior).
  - Build outputs are tagged locally and, optionally, with the Internal Registry prefix when
    configured. They must not be tagged with the Mirror Registry.
- CI
  - Kaniko builds must pass REGISTRY_PREFIX=repository.migros.net/ for base pulls.
  - All pushes go to the GitLab internal registry ($CI_REGISTRY_IMAGE), not the MR.

APIs and module-level responsibilities

Rust: src/registry.rs (new dual-registry API)
- New functions (mirror; probe + disk cache):
  - preferred_mirror_registry_prefix_quiet() -> String
    - Probe MR reachability (curl → TCP fallback). Normalize to trailing “/” or empty.
    - Cache behavior retained (existing OnceCell + on-disk cache). Disk cache filename should be
      explicit as “aifo-coder.mirrorprefix” (rename from previous generic name).
  - preferred_mirror_registry_source() -> String
    - “curl”, “tcp”, “unknown”, or test override.
- New functions (internal; env-only, no probe):
  - preferred_internal_registry_prefix_quiet() -> String
    - Read ONLY AIFO_CODER_INTERNAL_REGISTRY_PREFIX; normalize (single trailing “/”).
    - When unset or empty, return “”.
    - No disk cache (env is definitive). Cache in-process OnceCell only.
  - preferred_internal_registry_source() -> String
    - “env” or “env-empty”, else “unset”.
- Removal of obsolete variable
  - AIFO_CODER_REGISTRY_PREFIX: ignored across the entire codebase. No fallback or alias.
- Cache invalidation
  - invalidate_registry_cache() must clear the mirror on-disk cache (aifo-coder.mirrorprefix).
  - Internal has no disk cache; no action required for internal beyond in-process OnceCell
    (which is per-process anyway).

Rust: src/agent_images.rs (our image resolution)
- default_image_for(_quiet):
  - Build the unprefixed “aifo-coder-<agent>(-slim?):<tag>” first.
  - Prepend preferred_internal_registry_prefix_quiet() if non-empty.
- Do not query the mirror here.

Rust: src/main.rs (diagnostics)
- print_verbose_run_info:
  - Replace the single “registry” line with two lines:
    - internal registry: <prefix or “(none)”> (source: internal:<source>)
    - mirror registry: <prefix or “(none)”> (source: mirror:<source>)
  - The internal line reflects preferred_internal_registry_prefix_quiet().
  - The mirror line reflects preferred_mirror_registry_prefix_quiet() (no MR probe logs in quiet).
- Doctor should report both registries similarly (if implemented elsewhere).

Rust: src/toolchain/images.rs
- Toolchain image defaults (aifo-coder-toolchain-*) represent “our images”:
  - Prepend preferred_internal_registry_prefix_quiet() if non-empty.
- Official upstream fallbacks (e.g., python:3.12-slim, golang:1.22-bookworm, rust:<ver>-bookworm)
  are used only if the default is an upstream image. They should remain unprefixed at runtime.
  Build-time Dockerfiles continue to pull through MR via ARG.

Makefile and scripts
- Makefile:
  - Keep MIRROR_CHECK_STRICT/MIRROR_CHECK_LAX for MR reachability and set RP accordingly.
  - REGISTRY_PREFIX build-arg must always use RP (MR) for base pulls.
  - Introduce INTERNAL_REG from REGISTRY or AIFO_CODER_INTERNAL_REGISTRY_PREFIX, normalized to end
    with a single “/” when non-empty. Example macro:

      define INTERNAL_REG_SETUP
        INTERNAL_REG="$${REGISTRY:-$${AIFO_CODER_INTERNAL_REGISTRY_PREFIX}}"; \
        if [ -n "$$INTERNAL_REG" ]; then case "$$INTERNAL_REG" in */) ;; *) INTERNAL_REG="$$INTERNAL_REG/";; esac; fi
      endef

  - For each build-* target:
    - Always pass: --build-arg REGISTRY_PREFIX="$$RP"
    - Always tag: -t $(IMAGE)
    - If INTERNAL_REG non-empty, also tag: -t "$${INTERNAL_REG}$(IMAGE)"
    - Remove any tagging to $${REG} when REG fell back to RP (that path must not exist anymore).
  - For each publish-* target:
    - Push only when REGISTRY (or INTERNAL_REG) is set; push only to INTERNAL_REG.
    - If REGISTRY is unset: never push to MR or Docker Hub; keep current OCI archive fallback.
- scripts/build-images.sh:
  - Keep RP detection and use it only for --build-arg REGISTRY_PREFIX="$RP".
  - Add INTERNAL_REG="${REGISTRY:-${AIFO_CODER_INTERNAL_REGISTRY_PREFIX}}"; normalize trailing “/”.
  - Tag images locally and, if INTERNAL_REG non-empty, also tag "$INTERNAL_REG$img".
  - Never tag to MR.

CI (.gitlab-ci.yml)
- Kaniko jobs must include KANIKO_CUSTOM_BUILD_ARGUMENTS with REGISTRY_PREFIX pointing to MR:
  - build-rust-builder: "REGISTRY_PREFIX=repository.migros.net/ WITH_WIN=0"
  - build: "REGISTRY_PREFIX=repository.migros.net/"
  - build-toolchain: "REGISTRY_PREFIX=repository.migros.net/"
- All publishing is already directed at $CI_REGISTRY_IMAGE (internal); no changes required there.

Documentation updates
- README.md and docs/CONTRIBUTING.md:
  - Add “Two registries” section:
    - Mirror: repository.migros.net (used for base pulls at build time only)
    - Internal: registry.intern.migros.net (used for our images at runtime and for pushes)
  - New env vars:
    - AIFO_CODER_INTERNAL_REGISTRY_PREFIX: internal; normalized; default empty (no prefix).
    - AIFO_CODER_MIRROR_REGISTRY_PREFIX: optional dev override for MR in build tooling.
  - AIFO_CODER_REGISTRY_PREFIX is obsolete and ignored.
  - Examples:
    - Local publish to internal:
      REGISTRY=registry.intern.migros.net/ make publish
    - Local build with MR base pulls and internal tagging:
      AIFO_CODER_INTERNAL_REGISTRY_PREFIX=registry.intern.migros.net/ make build
- docs/INSTALL.md:
  - Clarify the role of MR and IR; show how to set REGISTRY for push.
- man page (man/aifo-coder.1):
  - If it lists AIFO_CODER_REGISTRY_PREFIX, remove/replace with INTERNAL/MIRROR language.

Tests
- Mirror probe tests:
  - Switch to calling preferred_mirror_registry_prefix_quiet() and preferred_mirror_registry_source().
  - Ensure overrides (curl-ok, tcp-ok, etc.) still drive MR source to “curl”/“tcp”, not “unknown”.
- Internal env tests:
  - Use AIFO_CODER_INTERNAL_REGISTRY_PREFIX, not obsolete variables.
  - Cases:
    - value="registry.intern.migros.net////" → normalized "registry.intern.migros.net/", source="env"
    - value="" → "", source="env-empty"
- Remove any tests that reference AIFO_CODER_REGISTRY_PREFIX, or update them to INTERNAL.

Backward compatibility
- AIFO_CODER_REGISTRY_PREFIX is removed and ignored; this is a deliberate breaking change.
  - Document the change in CHANGES.md and announce in release notes.
  - Provide a clear migration: set AIFO_CODER_INTERNAL_REGISTRY_PREFIX for runtime, REGISTRY for push.

Risks and mitigations
- Developers setting only MR may expect runtime pulls to work; clarify MR affects base pulls only.
- Users without access to IR:
  - Runtime uses unprefixed image tags; require explicit --image or local tags if IR is not set.
- Tests flapping due to MR probe:
  - Keep MR probe cached (on-disk OnceCell cache as before, but rename file to mirror-specific).
  - Provide --invalidate-registry-cache to clear MR cache.

Phased implementation plan

Phase 0: Preparation
- Create this spec and get buy-in.
- Identify all callers of preferred_registry_prefix_* and our image resolution.

Deliverable: Approved spec committed to spec/.

Phase 1: Library API (dual registries)
- Implement in src/registry.rs:
  - Add preferred_internal_registry_prefix_quiet(), preferred_internal_registry_source().
  - Add preferred_mirror_registry_prefix_quiet(), preferred_mirror_registry_source() by
    refactoring existing probe logic. Rename disk cache file to aifo-coder.mirrorprefix.
  - Make invalidate_registry_cache() clear the MR disk cache only.
  - Ensure AIFO_CODER_REGISTRY_PREFIX is unused (remove any reads).
- Unit tests:
  - Add internal env normalization tests.
  - Update mirror probe tests to call the new mirror functions.

Acceptance:
- cargo test passes locally (including updated registry tests).
- No references remain to AIFO_CODER_REGISTRY_PREFIX in Rust code.

Phase 2: Runtime image resolution and diagnostics
- src/agent_images.rs: prepend internal prefix for our images.
- src/toolchain/images.rs: prepend internal prefix for aifo-coder-toolchain-* images.
- src/main.rs: show both registries and sources in verbose info (and doctor, if needed).

Acceptance:
- docker command preview shows internal-prefixed images when INTERNAL_REG is set.
- Verbose output shows both internal and mirror lines.

Phase 3: Build tooling updates (Makefile and scripts)
- Makefile:
  - Introduce INTERNAL_REG_SETUP macro.
  - For every build-* target: tag local + internal (when set); pass REGISTRY_PREFIX="$RP".
  - Remove any paths that tag to MR.
  - For every publish-* target: push only to INTERNAL_REG; OCI archive fallback when unset.
- scripts/build-images.sh:
  - Add INTERNAL_REG support for tagging; keep MR build-arg for base pulls.

Acceptance:
- make build produces local tags and, when configured, internal-prefixed tags.
- make publish pushes only to internal; never to mirror.

Phase 4: CI updates
- .gitlab-ci.yml:
  - Ensure KANIKO_CUSTOM_BUILD_ARGUMENTS contains REGISTRY_PREFIX=repository.migros.net/ on
    build jobs (builder, agents, toolchains).
  - Verify existing publishing continues to push to $CI_REGISTRY_IMAGE (internal).
- Validate MR reachability is not required in CI runners (explicit ARG is enough).

Acceptance:
- CI pipelines green for MR and default branches.
- Builder/agent/toolchain images available in internal registry.

Phase 5: Documentation and cleanup
- Update README.md, docs/CONTRIBUTING.md, docs/INSTALL.md, and man page to reflect new env vars,
  roles of MR vs IR, and the removal of the obsolete variable.
- Update examples and “How to publish” instructions.
- Add CHANGES.md entry announcing breaking change.

Acceptance:
- Docs committed; reviewers confirm clarity.
- No remaining mention of AIFO_CODER_REGISTRY_PREFIX in code/docs/tests.

Out-of-scope follow-ups (optional)
- Add a “doctor” check that validates the configured internal prefix is reachable (TCP check),
  clearly labeled as “best-effort” and non-fatal.
- Add a CLI subcommand to print registry configuration in JSON for tooling.

Summary
- Split MR (build-time base pulls) from IR (runtime pulls + pushes).
- Remove obsolete AIFO_CODER_REGISTRY_PREFIX entirely.
- Update code, build tooling, CI, tests, and docs accordingly.
