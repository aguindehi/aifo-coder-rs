Title: Refactor and extend Node toolchain with prebuilt image and precise caches

Context
- Current Node sidecar uses the official node image and only mounts npm cache.
- Several tools are commonly required for modern Node workflows: yarn, pnpm, deno.
- Mounting a broad ~/.cache from host is undesirable; we prefer precise cache mounts or one
  XDG cache directory inside the container backed by a named Docker volume.
- Project already ships dedicated toolchain images for rust and c/cpp; we align Node with that.

Goals
- Provide a prebuilt Node toolchain image with yarn, pnpm, and deno available out of the box.
- Optimize startup (no runtime bootstrap scripts) and improve determinism.
- Consolidate caches under XDG_CACHE_HOME inside the container and mount exactly that directory
  via a named volume, avoiding host coupling.
- Extend routing and shims to support yarn, pnpm, deno.
- Keep behavior configurable and backward-compatible where reasonable.

Non-Goals
- No bind-mount of host ~/.cache.
- No global package persistence beyond what is necessary for pnpm home; projects can still
  use local package managers as they prefer.

Constraints and conventions
- Line length target ≤ 100 chars (prefer ≤ 80) per CONVENTIONS.md.
- Avoid dead code; keep matches exhaustive where applicable.
- Avoid TODO markers that would trip tidy checks.
- Maintain existing Docker multi-stage patterns and shim approach.

Design summary
- New image: toolchains/node/Dockerfile building aifo-node-toolchain:<tag>.
- images.rs: default Node image points to aifo-node-toolchain (with versioned selector).
- sidecar.rs:
  - Mount a single named volume aifo-node-cache to /home/coder/.cache when caching is enabled.
  - Set env for XDG cache consumers and package managers:
    - XDG_CACHE_HOME=/home/coder/.cache
    - NPM_CONFIG_CACHE=/home/coder/.cache/npm
    - YARN_CACHE_FOLDER=/home/coder/.cache/yarn
    - PNPM_STORE_PATH=/home/coder/.cache/pnpm-store
    - PNPM_HOME=/home/coder/.local/share/pnpm
    - DENO_DIR=/home/coder/.cache/deno
    - Ensure PATH includes /home/coder/.local/share/pnpm/bin (deno installed to /usr/local/bin).
- mounts.rs:
  - Add init helper to chown the mounted node cache volume once and stamp it to avoid repeats.
  - Detect the presence of the cache mount in run args and invoke initialization prior to start.
- routing.rs:
  - Route yarn, pnpm, deno to the node sidecar; extend allowlist accordingly.
- shim.rs:
  - Add shims for yarn, pnpm, deno to ensure proxy routing works uniformly.
- Dockerfile (top-level):
  - Extend shim symlink loops to include yarn, pnpm, deno in both base and base-slim.

Phases

Phase 0: New Node toolchain image
- Add toolchains/node/Dockerfile.
- Base: ${REGISTRY_PREFIX}node:22-bookworm-slim
- Install minimal deps: ca-certificates, curl, git, unzip (deno installer requires unzip).
- Prepare user home and caches:
  - HOME=/home/coder
  - XDG_CACHE_HOME=/home/coder/.cache
  - PNPM_HOME=/home/coder/.local/share/pnpm
  - Create dirs and chmod 0777 recursively under /home/coder to support arbitrary uid:gid.
- Enable and activate corepack:
  - corepack enable
  - corepack prepare yarn@stable --activate
  - corepack prepare pnpm@latest --activate
- Install deno system-wide:
  - curl -fsSL https://deno.land/install.sh | sh -s -- -d /usr/local
  - This yields /usr/local/bin/deno; no extra PATH needed beyond defaults.
- Set env defaults for caches and PATH:
  - XDG_CACHE_HOME=/home/coder/.cache
  - NPM_CONFIG_CACHE=/home/coder/.cache/npm
  - YARN_CACHE_FOLDER=/home/coder/.cache/yarn
  - PNPM_STORE_PATH=/home/coder/.cache/pnpm-store
  - PNPM_HOME=/home/coder/.local/share/pnpm
  - DENO_DIR=/home/coder/.cache/deno
  - PATH includes $PNPM_HOME/bin
- WORKDIR /workspace; CMD ["sleep", "infinity"].

Phase 1: Image selection and normalization
- src/toolchain/images.rs:
  - Keep aliases: ts/typescript map to node.
  - Change Node defaults:
    - DEFAULT_IMAGE_BY_KIND: ("node", "aifo-node-toolchain:latest")
    - DEFAULT_IMAGE_FMT_BY_KIND: ("node", "aifo-node-toolchain:{version}")
  - Preserve existing env-based overrides for rust; Add Node overrides later if needed.

Phase 2: Shim expansion
- src/toolchain/shim.rs:
  - Extend SHIM_TOOLS with "yarn", "pnpm", "deno".
  - Rationale: allow proxy interception and uniform routing for these tools.

Phase 3: Routing update
- src/toolchain/routing.rs:
  - Add "yarn", "pnpm", "deno" to ALLOW_NODE.
  - route_tool_to_sidecar maps "yarn" | "pnpm" | "deno" to "node".
  - preferred_kinds_for_tool naturally prefers node for these (non dev-tool).

Phase 4: Sidecar run args and env
- src/toolchain/sidecar.rs:
  - build_sidecar_run_preview, kind == "node":
    - Replace aifo-npm-cache with aifo-node-cache:/home/coder/.cache when caching enabled.
    - Push env vars as listed in Design summary.
    - Keep proxy passthrough unchanged.
  - Build and log run previews as before (verbose/dry-run unchanged).

Phase 5: Volume ownership initialization
- src/toolchain/mounts.rs:
  - Add init_node_cache_volume(runtime, image, uid, gid, verbose):
    - docker run --rm -v aifo-node-cache:/home/coder/.cache <image> sh -lc
      "set -e; d=/home/coder/.cache; if [ -f \"$d/.aifo-init-done\" ]; then exit 0; fi; \
       mkdir -p \"$d\"; chown -R ${uid}:${gid} \"$d\" || true; \
       printf '%s\n' '${uid}:${gid}' > \"$d/.aifo-init-done\" || true"
  - Add an inspector similar to rust volumes logic that detects the cache mount in run args and
    calls init_node_cache_volume when present.
  - toolchain_run and toolchain_start_session:
    - Before starting a node sidecar with cache enabled, perform the init step.

Phase 6: Exec args and PATH
- src/toolchain/sidecar.rs:
  - build_sidecar_exec_preview, kind == "node":
    - Ensure PNPM_HOME and PATH that includes $PNPM_HOME/bin are pushed so commands are resolvable
      even if a pre-existing container was started previously.
  - No runtime bootstrap is needed because the image is prebuilt.

Phase 7: Purge caches
- src/toolchain/sidecar.rs:
  - toolchain_purge_caches adds "aifo-node-cache" to the list.
  - For back-compat, retain "aifo-npm-cache" in purge list to clean legacy volumes.

Phase 8: Tests
- tests/route_map.rs:
  - Add asserts that "yarn", "pnpm", "deno" route to "node".
- Optional preview tests (future work):
  - Verify run preview includes -v aifo-node-cache:/home/coder/.cache for node when caching enabled.
  - Verify env contains XDG_CACHE_HOME and PNPM_HOME in exec preview.

Backwards compatibility
- Existing behavior for rust/python/c-cpp/go unaffected.
- Node:
  - Prior npm-only cache volume remains in purge for cleanup.
  - Default image now aifo-node-toolchain; callers can still override image via existing mechanisms.

Security and trust
- Deno installation is fetched over TLS with system CAs; enterprise CA support is consistent with
  other Dockerfiles when secrets are provided during build.
- No host bind mounts for caches, reducing permission and leakage risks.

Operational notes
- Named volume aifo-node-cache provides stable caching across sessions without binding to host
  filesystem semantics.
- Ownership init is idempotent and stamped to minimize overhead in concurrent starts.

Acceptance criteria
- yarn, pnpm, deno commands work inside the node sidecar without additional setup.
- Caches materialize under /home/coder/.cache/{npm,yarn,pnpm-store,deno}.
- Routing maps yarn/pnpm/deno to node; shims exist for these tools.
- Node sidecar startup time does not include on-the-fly tool bootstrapping.

Risks and mitigations
- If corepack changes behavior, yarn/pnpm availability could drift:
  - We activate specific channels via corepack prepare and can pin versions if needed later.
- Deno installer availability:
  - If deno install fails during image build, the build fails visibly; consider pinning versions
    or mirroring in enterprise contexts if required.

Future work
- Add AIFO_NODE_TOOLCHAIN_IMAGE and AIFO_NODE_TOOLCHAIN_VERSION env override symmetry with rust.
- Optional second named volume for global package bins (NPM_CONFIG_PREFIX/PNPM_HOME) if workflows
  need persistent globals beyond current layout.
- Add preview tests covering env and mounts for node, similar to rust tests.

Rollout
- Land spec, implement Dockerfile and code changes as per phases, extend minimal tests.
- Build images with/without registry prefix argument and verify via preview tests.

Versioning
- Map default_toolchain_image_for_version("node", v) to aifo-node-toolchain:{v}.
- Keep fallback for unknown kinds consistent: "node:20-bookworm-slim" as a safety default.

End of spec
