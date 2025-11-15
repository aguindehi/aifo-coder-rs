# Spec v2: Dual registries (Internal vs Mirror), cleanup, and migration

Executive summary
- Split registry responsibilities:
  - Internal Registry (IR): our first‑party images; used at runtime and for pushes.
  - Mirror Registry (MR): upstream base images; used only for Dockerfile base pulls.
- Remove and ignore obsolete AIFO_CODER_REGISTRY_PREFIX everywhere.
- Introduce explicit, testable Rust APIs for IR and MR (no conflation).
- Update CLI diagnostics to show both registries independently.
- Update build tooling (Makefile/scripts) and CI to use MR for base pulls, IR for tags/push.

Validation of current code base and gaps
- Single registry model:
  - Rust uses preferred_registry_prefix[_quiet]() and preferred_registry_source() coupled with
    AIFO_CODER_REGISTRY_PREFIX and disk cache “aifo-coder.regprefix”.
  - Many tests assert env=AIFO_CODER_REGISTRY_PREFIX behavior and single registry output.
- CLI diagnostics:
  - “images” prints one “registry:” line; doctor prints “docker registry:” single line.
- Build tooling:
  - Makefile tags to MR when REG fell back to RP; references AIFO_CODER_REGISTRY_PREFIX; no clear
    INTERNAL_REG concept.
  - scripts/build-images.sh tags to MR. No support for IR tagging.
- CI:
  - Kaniko jobs do not set REGISTRY_PREFIX=repository.migros.net/ for base pulls.
- Documentation:
  - README and docs/CONTRIBUTING.md refer to AIFO_CODER_REGISTRY_PREFIX and single registry.

Corrections and final decisions
- Two independent registries must be modeled and displayed:
  - IR comes only from env (AIFO_CODER_INTERNAL_REGISTRY_PREFIX), normalized, no probe.
  - MR comes from probe (curl→TCP) or build‑tool override; normalized; quiet variant available.
- Disk cache:
  - Only MR uses an on‑disk cache. Rename file to “aifo-coder.mirrorprefix”.
- Deprecation and migration:
  - Remove reads of AIFO_CODER_REGISTRY_PREFIX; ignore it everywhere.
  - Provide clear migration: IR via AIFO_CODER_INTERNAL_REGISTRY_PREFIX; push via REGISTRY in tools.
- Build-time:
  - Dockerfile continues to accept ARG REGISTRY_PREFIX (points to MR).
  - Tag outputs locally and, optionally, with IR prefix; never tag to MR.
- Runtime image resolution:
  - Our images (aifo-coder-* agents and aifo-coder-toolchain-*) are prefixed with IR when set.
  - Official upstream defaults (python:3.12-slim, golang:1.22-bookworm, rust:<ver>-bookworm)
    remain unprefixed at runtime.
- Diagnostics:
  - Verbose runs and doctor print two lines:
    - internal registry: <prefix or “(none)”> (source: internal:<source>)
    - mirror registry: <prefix or “(none)”> (source: mirror:<source>)

Environment variables (final)
- AIFO_CODER_INTERNAL_REGISTRY_PREFIX (runtime IR)
  - Normalize to a single trailing “/” when non-empty; empty/unset yields “” (no prefix).
  - Sources: “env” when non-empty, “env-empty” when set empty, “unset” when absent.
- AIFO_CODER_MIRROR_REGISTRY_PREFIX (optional build tooling MR)
  - Used by Makefile/scripts only to override MR; not used by runtime launcher.
- AIFO_CODER_REGISTRY_PREFIX
  - Obsolete. Must be ignored entirely.
- REGISTRY (Makefile/scripts only)
  - Internal registry for tagging/push; normalize trailing “/”.

Rust APIs and module responsibilities

src/registry.rs (dual registry API)
- Mirror registry (probe + disk cache):
  - preferred_mirror_registry_prefix_quiet() -> String
    - Quiet HTTPS probe via curl; fallback to TCP connect; normalize trailing “/” or empty.
    - Cache in-process (OnceCell) and on-disk “aifo-coder.mirrorprefix”.
    - Respect test overrides via registry_probe_set_override_for_tests(Some(mode)).
  - preferred_mirror_registry_source() -> String
    - Returns “curl”, “tcp”, or “unknown” (for override/unset).
- Internal registry (env-only; no probe, no disk cache):
  - preferred_internal_registry_prefix_quiet() -> String
    - Read ONLY AIFO_CODER_INTERNAL_REGISTRY_PREFIX; normalize trailing “/”; cache OnceCell.
  - preferred_internal_registry_source() -> String
    - Returns “env” / “env-empty” / “unset”.
- Cache invalidation:
  - invalidate_registry_cache() clears only MR disk cache (aifo-coder.mirrorprefix).
  - IR has no disk cache; OnceCell is per-process and needs no invalidation.
- Deprecation layer (temporary; scheduled for removal in Phase 5):
  - preferred_registry_prefix_quiet() → alias of preferred_mirror_registry_prefix_quiet().
  - preferred_registry_source() → alias of preferred_mirror_registry_source().

src/agent_images.rs (our agent images at runtime)
- default_image_for(_quiet):
  - Compose unprefixed “aifo-coder-<agent>(-slim?):<tag>”.
  - Prepend IR (preferred_internal_registry_prefix_quiet()) when non-empty.
  - Do not query MR here.

src/toolchain/images.rs (our toolchain images at runtime)
- default_toolchain_image(_for_version):
  - For aifo-coder-toolchain-* images, prepend IR when non-empty.
  - Official upstream defaults (python/golang/rust) remain unprefixed at runtime.
- is_official_rust_image / official_rust_image_for_version unchanged.

src/main.rs (diagnostics)
- print_verbose_run_info:
  - Replace single “registry” line with two lines:
    - internal registry: <prefix or “(none)”> (source: internal:<src>)
    - mirror registry: <prefix or “(none)”> (source: mirror:<src>)
  - Quiet MR probe; IR from env only.

src/doctor.rs
- Replace single “docker registry:” line with:
  - internal registry: <prefix or “(none)”>
  - mirror registry: <prefix or “(none)”>
- Keep quiet MR probe; no intermediate probe logs.

Build tooling (Makefile and scripts)
- Makefile:
  - Keep MIRROR_CHECK_STRICT/MIRROR_CHECK_LAX for MR reachability and set RP accordingly.
  - Introduce INTERNAL_REG_SETUP macro:
      define INTERNAL_REG_SETUP
        INTERNAL_REG="$${REGISTRY:-$${AIFO_CODER_INTERNAL_REGISTRY_PREFIX}}"; \
        if [ -n "$$INTERNAL_REG" ]; then case "$$INTERNAL_REG" in */) ;; *) INTERNAL_REG="$$INTERNAL_REG/";; esac; fi
      endef
  - For each build-* target:
    - Always pass: --build-arg REGISTRY_PREFIX="$$RP" (MR for base pulls).
    - Always tag: -t $(IMAGE) (local).
    - If INTERNAL_REG non-empty, also tag: -t "$${INTERNAL_REG}$(IMAGE)".
    - Remove all MR tagging paths; NEVER tag to MR.
  - For each publish-* target:
    - Push only when REGISTRY (or INTERNAL_REG) is set; push only to INTERNAL_REG.
    - If REGISTRY is unset: never push to MR or Docker Hub; keep OCI archive fallback.
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
- All publishing continues to push to $CI_REGISTRY_IMAGE (internal); no changes needed there.

Documentation updates
- README.md:
  - Add “Two registries” section clarifying MR vs IR roles and env vars.
  - Remove references to AIFO_CODER_REGISTRY_PREFIX; show IR/MR examples.
- docs/CONTRIBUTING.md:
  - Update “Conventions and registry prefix” to IR/MR language.
  - Clarify build-time REGISTRY_PREFIX (MR) and runtime IR usage.
- docs/INSTALL.md:
  - Clarify MR vs IR roles; show how to set REGISTRY for push.
- man page (man/aifo-coder.1):
  - Replace any mention of AIFO_CODER_REGISTRY_PREFIX with IR/MR descriptions.

Tests: migration plan and coverage
- Replace AIFO_CODER_REGISTRY_PREFIX with AIFO_CODER_INTERNAL_REGISTRY_PREFIX in tests that
  assert runtime image prefixing and CLI outputs:
  - tests/cli_images_registry_env.rs
  - tests/images_output_new_agents.rs
  - tests/images_output.rs
  - tests/registry_env_empty.rs
  - tests/registry_env_value.rs
- Mirror probe tests must target MR functions:
  - tests/registry_probe_determinism.rs
  - tests/registry_probe_edge_cases.rs
  - tests/registry_probe_abstraction.rs
  - Switch to preferred_mirror_registry_prefix_quiet() and preferred_mirror_registry_source().
- CLI diagnostics tests:
  - Update “images” verbose and “doctor” to assert two lines:
    - internal registry: …
    - mirror registry: …
- Cache-clear test:
  - tests/cli_cache_clear_effect.rs must use “aifo-coder.mirrorprefix” instead of “aifo-coder.regprefix”.
- New internal env tests:
  - Cases:
    - value="registry.intern.migros.net////" → normalized "registry.intern.migros.net/", source="env"
    - value="" → "", source="env-empty"
    - unset → "", source="unset"

Backward compatibility
- Breaking change: AIFO_CODER_REGISTRY_PREFIX removed and ignored.
- Clear migration path:
  - Runtime: AIFO_CODER_INTERNAL_REGISTRY_PREFIX; build/push: REGISTRY via Makefile/scripts.
- Announce in CHANGES.md and release notes; include examples and FAQ.

Risks and mitigations
- Confusion between MR and IR roles:
  - CLI diagnostics and docs clarify MR affects base pulls only; IR affects runtime/push.
- Users without IR:
  - Runtime uses unprefixed image tags; require explicit --image or local tags when IR not set.
- MR probe flapping:
  - Keep MR probe cached (OnceCell + on-disk aifo-coder.mirrorprefix) and provide cache invalidation.

Phased implementation plan

Phase 0: Preparation and inventory
- Commit this v2 spec to spec/.
- Inventory code/tests/docs that reference preferred_registry_* and AIFO_CODER_REGISTRY_PREFIX.
- Identify any hard-coded paths to “aifo-coder.regprefix”.

Acceptance:
- Spec committed; inventory documented (internal task notes).

Phase 1: Library API (dual registries)
- Implement:
  - preferred_internal_registry_prefix_quiet(), preferred_internal_registry_source().
  - preferred_mirror_registry_prefix_quiet(), preferred_mirror_registry_source().
- MR probe refactor:
  - Rename disk cache to “aifo-coder.mirrorprefix”.
  - Split OnceCells for IR and MR.
- invalidate_registry_cache() clears only MR disk cache.
- Deprecation layer:
  - Temporarily alias preferred_registry_* to MR functions; mark for removal in Phase 5.
- Remove all reads of AIFO_CODER_REGISTRY_PREFIX in library code.

Acceptance:
- cargo test passes for updated registry unit tests.
- No library reads of obsolete env var remain.

Phase 2: Runtime image resolution and CLI diagnostics
- src/agent_images.rs and src/toolchain/images.rs:
  - Prepend IR for our images; upstream images stay unprefixed.
- src/main.rs:
  - Update verbose output to two lines with sources.
- src/doctor.rs:
  - Print internal and mirror registries (quiet MR probe).

Acceptance:
- Verbose output shows both internal and mirror lines with sources.
- doctor prints two registry lines; no probe noise.
- Agents/toolchains resolve IR when set; upstream images remain unprefixed.

Phase 3: Build tooling updates
- Makefile:
  - Add INTERNAL_REG_SETUP macro; use RP for base pulls; tag local + IR; remove MR tagging.
  - Publish targets push to IR only; OCI fallback when REGISTRY unset.
- scripts/build-images.sh:
  - Add INTERNAL_REG support (normalize trailing “/”); tag local + IR; never tag MR.

Acceptance:
- make build creates local tags and IR tags when configured; no MR tags.
- make publish pushes only to internal; OCI fallback works.

Phase 4: CI updates
- .gitlab-ci.yml:
  - Ensure build jobs pass REGISTRY_PREFIX=repository.migros.net/ via KANIKO_CUSTOM_BUILD_ARGUMENTS.
  - Validate publishing continues to $CI_REGISTRY_IMAGE (internal).

Acceptance:
- CI pipelines green; builder/agent/toolchain images accessible from internal registry.

Phase 5: Documentation and tests; remove deprecated API
- Update README.md, docs/CONTRIBUTING.md, docs/INSTALL.md, and man page to reflect IR/MR.
- Update or remove tests that reference AIFO_CODER_REGISTRY_PREFIX and single registry lines.
- Delete preferred_registry_* deprecated aliases from src/registry.rs.
- Update cache-clear test to use aifo-coder.mirrorprefix.
- Add CHANGES.md entry about breaking change and migration examples.

Acceptance:
- Docs updated; tests pass; no remaining references to AIFO_CODER_REGISTRY_PREFIX.

Appendix: normalization & display
- Normalize prefixes to end with one “/” when non-empty.
- Display rules:
  - Verbose runs:
    - internal registry: <prefix or “(none)”> (source: internal:<src>)
    - mirror registry: <prefix or “(none)”> (source: mirror:<src>)
  - Doctor:
    - internal registry: <prefix or “(none)”>
    - mirror registry: <prefix or “(none)”>

Summary
- Clearly separate IR (runtime/push) from MR (build-time base pulls).
- Update library APIs, diagnostics, tooling, CI and docs accordingly.
- Remove obsolete env variable and single-registry paths; provide migration.
