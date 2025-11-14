# Phase 0 Inventory: Dual Registries Refactor (Spec v2)

Date: 2025-11-14

Executive summary
- Document current single-registry references in code, tests, and docs.
- Identify hard-coded disk cache path "aifo-coder.regprefix".
- Prepare for Phase 1–5 changes per spec.

Key single-registry APIs and env var references
- Functions (to be split into IR/MR in Phase 1):
  - src/registry.rs
    - preferred_registry_prefix()
    - preferred_registry_prefix_quiet()
    - preferred_registry_source()
    - invalidate_registry_cache() (clears on-disk "aifo-coder.regprefix")
- Env var (obsolete in Spec v2; will be ignored in Phase 1+):
  - AIFO_CODER_REGISTRY_PREFIX
    - Read in src/registry.rs (env override precedence).
    - Used in tests and build tooling (Makefile) today.

Code paths using preferred_registry_* and AIFO_CODER_REGISTRY_PREFIX
- src/agent_images.rs
  - Calls aifo_coder::preferred_registry_prefix() and _quiet().
- src/commands/mod.rs
  - run_images(): uses preferred_registry_prefix_quiet() for display.
  - run_cache_clear(): calls invalidate_registry_cache().
- src/main.rs
  - print_verbose_run_info(): preferred_registry_prefix_quiet() and preferred_registry_source().
- src/support.rs
  - At end: preferred_registry_prefix_quiet() for verbose registry line.
- src/doctor.rs
  - Prints "docker registry: ..." via preferred_registry_prefix_quiet().
- src/lib.rs
  - pub use registry::* (re-exports the single-registry API publicly).

On-disk cache file (to be renamed in Phase 1)
- Hard-coded path: "aifo-coder.regprefix"
  - src/registry.rs → registry_cache_path(), write_registry_cache_disk(), invalidate_registry_cache().
  - tests/cli_cache_clear_effect.rs creates/removes aifo-coder.regprefix under XDG_RUNTIME_DIR.

Tests that assume single-registry model and/or AIFO_CODER_REGISTRY_PREFIX
- tests/cli_images_registry_env.rs
  - Sets/clears AIFO_CODER_REGISTRY_PREFIX; asserts single "registry:" line.
- tests/images_output_new_agents.rs
  - Sets AIFO_CODER_REGISTRY_PREFIX to "" or a value to force prefix; asserts single line behavior.
- tests/registry_env_empty.rs
  - Asserts preferred_registry_prefix_quiet() produces empty prefix and source="env-empty".
- tests/registry_env_value.rs
  - Asserts normalized trailing slash "example.com/" and source="env".
- tests/images_output.rs
  - Uses preferred_registry_prefix_quiet() to compose image refs for agents.
- tests/registry_probe_determinism.rs
  - Asserts preferred_registry_prefix_quiet()/preferred_registry_source() with probe overrides.
- tests/registry_probe_edge_cases.rs
  - Clears PATH, exercises quiet probe; expects empty or internal registry.
- tests/registry_probe_abstraction.rs
  - Uses registry_probe_set_override_for_tests(); checks quiet/source semantics.

Build tooling and scripts (to be updated in Phase 3)
- Makefile
  - REG_SETUP_COMMON and REG_SETUP_WITH_FALLBACK read AIFO_CODER_REGISTRY_PREFIX today.
  - Many build/rebuild/publish targets tag both local and "$REG" prefixed images.
  - Mirror reachability macros present but used for base and sometimes tagging.
- scripts/build-images.sh
  - Probes reachability; tags to RP + local (no AIFO_CODER_REGISTRY_PREFIX reads here).

Documentation with single-registry language (to be updated in Phase 5)
- README.md
  - Mentions AIFO_CODER_REGISTRY_PREFIX and single "registry" display/output.
- docs/CONTRIBUTING.md
  - Describes AIFO_CODER_REGISTRY_PREFIX and Docker ARG REGISTRY_PREFIX.
- docs/INSTALL.md
  - Build/publish examples; single-registry mindset present.

Summary of migration targets for Spec v2
- Library (Phase 1):
  - Introduce explicit IR/MR APIs; remove reads of AIFO_CODER_REGISTRY_PREFIX.
  - Rename disk cache file to "aifo-coder.mirrorprefix" (MR only).
- Runtime/CLI (Phase 2):
  - Agents/toolchains prepend IR for our images; upstream remain unprefixed.
  - Diagnostics show two lines: internal registry and mirror registry.
- Build tooling (Phase 3):
  - Use MR (REGISTRY_PREFIX) for base pulls only.
  - Tag/push only to IR (REGISTRY or AIFO_CODER_INTERNAL_REGISTRY_PREFIX); never tag/push to MR.
- CI (Phase 4):
  - Ensure KANIKO_CUSTOM_BUILD_ARGUMENTS sets REGISTRY_PREFIX=repository.migros.net/.
- Docs/Tests and removal of deprecated API (Phase 5):
  - Replace AIFO_CODER_REGISTRY_PREFIX usage; update tests and docs.
  - Delete preferred_registry_* deprecated aliases.

Appendix: Files with hard-coded path "aifo-coder.regprefix"
- src/registry.rs
- tests/cli_cache_clear_effect.rs

Notes
- This inventory is preparatory; no behavior changes introduced in Phase 0.
- Next phases will implement the dual-registry model per spec.
