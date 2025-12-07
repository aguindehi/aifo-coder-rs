# Spec: Switch Aider builds between PyPI release and upstream git HEAD

## Objectives

1. Allow Aider images to be built from either the latest PyPI release (status quo) or a git clone of upstream HEAD (or any ref).
2. Retain full backwards compatibility: default builds must continue using the PyPI release without extra flags.
3. Provide reproducible configuration knobs similar to other git-based agents (e.g., Plandex), so CI and developers can pin specific refs.
4. Surface provenance (commit SHA vs. release version) inside resulting images.
5. Ensure minimal impact on build times and caching.

## Non-goals

- No changes to the Rust CLI, telemetry, or runtime behavior beyond how we install Aider into its virtualenv.
- No requirement to bundle multiple Aider variants simultaneously; selection happens at build time.
- Not responsible for automating periodic HEAD updates—this spec only enables opt-in builds.

## Proposed solution

### 1. Makefile knobs (analogy to existing git-driven agents)

- Introduce `AIDER_SOURCE ?= release` and `AIDER_GIT_REF ?= main` near the existing Aider variables.
- Update all Make targets that build/rebuild/publish Aider images (fat + slim) to pass:
  ```
  --build-arg AIDER_SOURCE="$(AIDER_SOURCE)"
  --build-arg AIDER_GIT_REF="$(AIDER_GIT_REF)"
  ```
- Document in `make help` comment block (same section that explains `AIDER_VERSION`/`PLANDEX_GIT_REF`).

**Consistency checks**
- Ensure rebuild/publish targets (including `build-aider`, `build-aider-slim`, `publish-aider*`, `rebuild-aider*`) propagate the new args.
- Default values guarantee identical behavior when users don’t override anything.

### 2. Dockerfile build arguments & propagation

- In `aider-builder` and `aider-builder-slim` stages:
  - Declare `ARG AIDER_SOURCE=release` and `ARG AIDER_GIT_REF=main` right after existing args.
  - Export them downward so later RUN commands can branch.
- In runtime stages (`aider`, `aider-slim`) add matching `ARG` statements even if only needed for documentation—they should pass through to keep multi-stage `--target aider` builds deterministic.
- Emit labels:
  - Always add `LABEL org.opencontainers.image.version="aider-release"` when using PyPI release (with exact version).
  - Add `LABEL org.opencontainers.image.revision="<git-sha>"` when installing from git for traceability.

### 3. Conditional installation logic

- **Release path (`AIDER_SOURCE=release`)**: keep existing `uv pip install aider-chat[...]`.
- **Git path (`AIDER_SOURCE=git`)**:
  1. Install git (already present in builder base).
  2. `git clone --depth=1 https://github.com/Aider-AI/aider.git /tmp/aider-src`.
  3. `git -C /tmp/aider-src fetch --depth=1 origin "${AIDER_GIT_REF}" && git checkout "${AIDER_GIT_REF}"`.
     - Allow tags/SHAs; when SHA is provided, depth must be removed or increased to include it.
  4. Capture resolved commit: `RESOLVED_SHA=$(git -C /tmp/aider-src rev-parse HEAD)`.
  5. Install via `uv pip install /tmp/aider-src` (plus `[playwright]` extras if requested).
  6. Record provenance inside the venv (e.g., `/opt/venv/.build-info/aider-git.txt` with ref + SHA).
  7. Clean `/tmp/aider-src` to avoid layer bloat.

**Edge cases**
- When `AIDER_GIT_REF` is a SHA or tag, set `--depth=1` only for branch HEAD; otherwise fallback to full fetch (documented in code comment).
- Fail build early with clear error if git clone or checkout fails.
- Keep `WITH_PLAYWRIGHT` logic identical for both modes.

### 4. Runtime stage alignment

- No direct logic changes; they copy `/opt/venv`.
- Ensure runtime stages share the provenance label (copy the resolved SHA via build arg or file).
  - Option: create build arg `AIDER_GIT_COMMIT` in builder stage after clone; pass it via `ARG`/`ENV` to runtime stage for labeling.

### 5. Documentation updates

- Update `README`/developer docs (if applicable) with:
  - Explanation of `AIDER_SOURCE` and `AIDER_GIT_REF`.
  - Example commands:
    - `make build-aider AIDER_SOURCE=git AIDER_GIT_REF=main`
    - `make build-aider-slim AIDER_SOURCE=git AIDER_GIT_REF=1f2e3d4`
  - Clarify default remains `release`, so CI unaffected unless overrides are set.

### 6. Testing & validation

- **Build verification**: add CI step or manual guidance to run both modes:
  - `make build-aider` (default release).
  - `make build-aider AIDER_SOURCE=git AIDER_GIT_REF=main`.
- **Runtime smoke test**: document command to check version inside container:
  ```
  docker run --rm -it <image> aider --version
  ```
  Expectation: release prints semantic version; git build should include commit/dirty info from upstream.
- Ensure `make check` still covers everything (no new Rust tests required). Mention that Docker build tests should be run after modifications.

### 7. Rollout / backward compatibility

- Land Makefile + Dockerfile changes simultaneously to avoid missing args.
- Communicate to infra that they can selectively set `AIDER_SOURCE=git` when they need HEAD images; no default change needed.
- Provide fallback instructions: if git builds fail (e.g., upstream breaks), revert to release by simply omitting overrides.

## Risks & mitigations

| Risk | Mitigation |
| ---- | ---------- |
| Git history fetch failures or rate limits | Use shallow clones, allow overriding `GIT_CLONE_FLAGS`, and retry once before failing. |
| Larger image size due to git checkout | Clean `/tmp/aider-src` and avoid adding repo data to final layers. |
| Ambiguous provenance | Enforce labels + `/opt/venv/.build-info` marker with ref + SHA. |
| CI forgetting to set overrides | Defaults remain release; document overrides in release checklist. |

## Success criteria

- Developers can build both release and git-head Aider images via Makefile overrides.
- Resulting images clearly indicate their source (version vs. commit SHA).
- No regression in default build path performance or behavior.
