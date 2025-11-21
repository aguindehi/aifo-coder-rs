# BuildKit and Kaniko guardrails

Date: 2025-11-17

Overview
- CI builds rely on Kaniko with --use-new-run, which supports a subset of BuildKit RUN semantics:
  - RUN --mount=type=secret
  - RUN --mount=type=cache
- Shared Dockerfiles are Kaniko-safe:
  - We avoid COPY --link and COPY --chmod (unsupported in Kaniko).
  - Where enterprise CA is injected via secrets, we perform best-effort cleanup in the same RUN.

Local builds
- Prefer docker buildx with BuildKit enabled:
  - DOCKER_BUILDKIT=1 is recommended in your environment.
  - The project Makefile defaults to buildx when available and falls back to classic docker build otherwise.
- Classic docker build without BuildKit can fail on RUN --mount lines.

Examples
- Build a single stage locally (loads into Docker):
  DOCKER_BUILDKIT=1 make build-debug STAGE=codex

- Build toolchain sidecars:
  DOCKER_BUILDKIT=1 make build-toolchain-node
  DOCKER_BUILDKIT=1 make build-toolchain-cpp
  DOCKER_BUILDKIT=1 make build-toolchain-rust

Notes
- Enterprise CA injection is best-effort and removed within the same RUN to avoid persisting CA material in layers.
- Runtime behavior is unchanged; PATH ordering and shim behavior are preserved by docker.rs and the base images.
