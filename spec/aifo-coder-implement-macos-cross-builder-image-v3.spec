Title: Implement macOS cross-builder image (osxcross) for aifo-coder
Version: v3
Status: Approved
Author: Migros AI Foundation
Date: 2025-11-10

Executive summary
- Objective: Build aifo-coder macOS launcher (aarch64-apple-darwin; optional x86_64) on Linux CI
  using osxcross with an Apple SDK injected via protected CI secrets.
- Approach: Add Dockerfile stage macos-cross-rust-builder bundling osxcross; build it with Kaniko
  after decoding SDK into ci/osx/; compile the Rust launcher in that image; ship artifacts and attach
  to releases alongside Linux binaries.
- Security/license: Do not commit the SDK to the repository. Prefer short‑lived, protected CI artifacts
  to move the SDK between jobs with checksum verification; allow masked/protected variables as a
  fallback. Restrict jobs handling the SDK to tags, schedules, or default‑branch manual runs. Cross
  image is internal; do not mirror publicly.

Plan validation and consistency checks (v2 → v3)
- Naming: Use macos-cross-rust-builder consistently in Dockerfile, CI, Makefile, and image refs.
- Kaniko secrets: Kaniko lacks RUN --mount=type=secret; v3 enforces COPY of ci/osx/<SDK>.tar.xz and
  documents decoding in CI before build; remove any secret mounts from this stage.
- Tool symlinks: osxcross tools can carry Darwin minor suffix; v3 keeps stable symlinks for
  aarch64-apple-darwin-{ar,ranlib,strip} and x86_64 equivalents to avoid SDK-version coupling.
- Cargo/linker: v3 sets CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER=oa64-clang and CC/CXX/AR/RANLIB
  envs; avoids relying on ~/.cargo/config.toml. This fixes CI linker mismatches.
- Deployment target: Pin MACOSX_DEPLOYMENT_TARGET=11.0 to avoid linking newer symbols by default.
- CI rules: Restrict cross-image build and macOS artifact jobs to tags; allow default-branch manual
  runs; add resource_group to serialize cross-image rebuilds.
- Validation: Add a file(1) smoke check asserting “Mach-O 64-bit arm64” on artifact.
- Release: Ensure publish-release needs macOS job artifacts and exposes a link named aifo-coder-macos-
  arm64; keep Linux artifact naming unchanged.

Design details
- Dockerfile: new stage macos-cross-rust-builder
  - Base: ${REGISTRY_PREFIX}rust:1-bookworm.
  - Packages: clang llvm make cmake patch xz-utils unzip curl git python3 file ca-certificates.
  - osxcross: git clone tpoechtrager/osxcross (pin optional); UNATTENDED=1 osxcross/build.sh.
  - SDK: COPY ci/osx/${OSX_SDK_FILENAME} → osxcross/tarballs/; CI decodes into ci/osx/ beforehand.
  - Tool symlinks: create stable aarch64-apple-darwin-{ar,ranlib,strip} and x86_64 equivalents.
  - Environment:
    PATH=/opt/osxcross/target/bin:$PATH
    MACOSX_DEPLOYMENT_TARGET=11.0
    CC_aarch64_apple_darwin=oa64-clang
    CXX_aarch64_apple_darwin=oa64-clang++
    AR_aarch64_apple_darwin=aarch64-apple-darwin-ar
    RANLIB_aarch64_apple_darwin=aarch64-apple-darwin-ranlib
    CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER=oa64-clang
    (Optional x86_64):
    CC_x86_64_apple_darwin=o64-clang
    CXX_x86_64_apple_darwin=o64-clang++
    AR_x86_64_apple_darwin=x86_64-apple-darwin-ar
    RANLIB_x86_64_apple_darwin=x86_64-apple-darwin-ranlib
    CARGO_TARGET_X86_64_APPLE_DARWIN_LINKER=o64-clang
  - rustup targets: aarch64-apple-darwin (required); x86_64-apple-darwin (optional).

- CI integration (.gitlab-ci.yml)
  - prepare-apple-sdk:
    - Downloads or decodes the Apple SDK into ci/osx/${OSX_SDK_FILENAME}, verifies APPLE_SDK_SHA256,
      and publishes it as a short‑lived artifact (expire_in). Restricted to tags, schedules, and
      default‑branch manual runs.
  - build-macos-cross-rust-builder:
    - extends container-build; declares needs on prepare-apple-sdk with artifacts: true; verifies
      checksum; sets KANIKO_BUILD_OPTIONS += --target macos-cross-rust-builder; tags
      $CI_REGISTRY_IMAGE/aifo-coder-macos-cross-rust-builder:$CI_COMMIT_TAG (or :ci). Resource group
      serializes builds; rules limited to tags and default‑branch manual runs.
  - build-launcher-macos:
    - uses image $CI_REGISTRY_IMAGE/aifo-coder-macos-cross-rust-builder:$CI_COMMIT_TAG; builds
      cargo --release --target aarch64-apple-darwin; copies to dist/aifo-coder-macos-arm64; asserts
      Mach-O arm64 via file(1).
  - publish-release: includes macOS artifact needs and a release link aifo-coder-macos-arm64.

- Makefile convenience (optional)
  - build-macos-cross-rust-builder: build Docker stage and tag locally with registry prefix logic.
  - build-launcher-macos-cross: docker run cross image and build macOS artifact; reuse cargo caches.

- Artifact naming
  - Linux: aifo-coder (unchanged).
  - macOS arm64: dist/aifo-coder-macos-arm64.
  - macOS x86_64 (optional): dist/aifo-coder-macos-x86_64.

Security and compliance controls
- Prefer APPLE_SDK_URL (protected) with APPLE_SDK_SHA256 (masked + protected) for integrity; allow
  APPLE_SDK_BASE64 as a fallback when artifacts are not viable.
- Jobs using the SDK: do not log SDK contents; show only file metadata (ls -lh). Use short‑lived,
  protected artifacts for job‑to‑job transfer; restrict job scope to tags/schedules/default‑branch manual.
- Image scope: macos-cross-rust-builder is project-internal; avoid public mirroring.

Acceptance criteria
- A tag pipeline successfully:
  1) Builds macos-cross-rust-builder and pushes it to the project registry.
  2) Builds aifo-coder for aarch64-apple-darwin using that image.
  3) Verifies Mach-O arm64 via file(1) and publishes dist/aifo-coder-macos-arm64 as artifact.
  4) publish-release attaches Linux and macOS artifacts to the release.

Risks, constraints and mitigations
- Apple SDK licensing: keep SDK out of repo; decode from masked variable; restrict jobs; do not log.
- Kaniko limitation: cannot use BuildKit secrets; rely on COPY of ci/osx/${OSX_SDK_FILENAME}.
- osxcross drift: optionally pin osxcross to a known commit; document pin if instability occurs.
- Native deps: prefer features linking Apple frameworks; avoid pkg-config Linux libs on mac targets.
- Linker mismatch: avoid ~/.cargo/config overrides in CI; set per-target envs inside the image.

Comprehensive phased implementation plan

Phase 0 — prerequisites (CI setup)
- Define APPLE_SDK_URL (protected) and APPLE_SDK_SHA256 (masked + protected). Optionally provide
  APPLE_SDK_BASE64 as a fallback. Set OSX_SDK_FILENAME (defaults to MacOSX.sdk.tar.xz).
- Use a producer job (prepare-apple-sdk) to download/verify the SDK and publish it as a short‑lived
  artifact. Restrict jobs using the SDK to tags, schedules, or default‑branch manual runs; lock
  runners to project/group and avoid logging raw contents.

Phase 1 — Dockerfile: macos-cross-rust-builder stage (no secrets)
- Append after rust-builder:
  FROM ${REGISTRY_PREFIX}rust:1-bookworm AS macos-cross-rust-builder
  ENV DEBIAN_FRONTEND=noninteractive
  RUN apt-get update && apt-get install -y --no-install-recommends \
        clang llvm make cmake patch xz-utils unzip curl git python3 file ca-certificates \
      && rm -rf /var/lib/apt/lists/*
  WORKDIR /opt
  ARG OSX_SDK_FILENAME=MacOSX.sdk.tar.xz
  # Expect CI to place SDK under ci/osx/ before build
  COPY ci/osx/${OSX_SDK_FILENAME} /tmp/${OSX_SDK_FILENAME}
  RUN git clone --depth=1 https://github.com/tpoechtrager/osxcross.git osxcross && \
      mv /tmp/${OSX_SDK_FILENAME} osxcross/tarballs/ && \
      UNATTENDED=1 osxcross/build.sh
  # Stable tool aliases (avoid darwinXX suffix dependency)
  RUN set -e; cd /opt/osxcross/target/bin; \
      for t in ar ranlib strip; do \
        ln -sf "$(ls aarch64-apple-darwin*-$$t | head -n1)" aarch64-apple-darwin-$$t || true; \
        ln -sf "$(ls x86_64-apple-darwin*-$$t | head -n1)"  x86_64-apple-darwin-$$t  || true; \
      done
  ENV PATH="/opt/osxcross/target/bin:${PATH}" \
      MACOSX_DEPLOYMENT_TARGET=11.0 \
      CC_aarch64_apple_darwin=oa64-clang \
      CXX_aarch64_apple_darwin=oa64-clang++ \
      AR_aarch64_apple_darwin=aarch64-apple-darwin-ar \
      RANLIB_aarch64_apple_darwin=aarch64-apple-darwin-ranlib \
      CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER=oa64-clang
  RUN /usr/local/cargo/bin/rustup target add aarch64-apple-darwin || true
  # Optional x86_64:
  # ENV CC_x86_64_apple_darwin=o64-clang \
  #     CXX_x86_64_apple_darwin=o64-clang++ \
  #     AR_x86_64_apple_darwin=x86_64-apple-darwin-ar \
  #     RANLIB_x86_64_apple_darwin=x86_64-apple-darwin-ranlib \
  #     CARGO_TARGET_X86_64_APPLE_DARWIN_LINKER=o64-clang
  # RUN /usr/local/cargo/bin/rustup target add x86_64-apple-darwin || true

Phase 2 — CI: build cross image (Kaniko) and macOS launcher
- Add job build-macos-cross-rust-builder:
  extends: container-build
  stage: build
  interruptible: true
  timeout: 60m
  resource_group: macos-cross-rust-builder
  tags: [ qual-mcap-gcp ]
  variables:
    TARGET_NAME: "macos-cross-rust-builder"
    IMAGE_PATH_EXTRA: "macos-cross-rust-builder"
    OSX_SDK_FILENAME: "MacOSX13.3.sdk.tar.xz"
  before_script:
    - mkdir -p ci/osx
    - test -n "$APPLE_SDK_BASE64"
    - echo "$APPLE_SDK_BASE64" | base64 -d > "ci/osx/${OSX_SDK_FILENAME}"
    - ls -lh "ci/osx/${OSX_SDK_FILENAME}"
    - KANIKO_BUILD_OPTIONS="${KANIKO_BUILD_OPTIONS} --target ${TARGET_NAME}"
  rules:
    - if: $CI_COMMIT_TAG
    - if: $CI_COMMIT_BRANCH == $CI_DEFAULT_BRANCH
      when: manual
- Add job build-launcher-macos:
  stage: build
  timeout: 45m
  interruptible: true
  needs: [ build-macos-cross-rust-builder ]
  image: $CI_REGISTRY_IMAGE/aifo-coder-macos-cross-rust-builder:$CI_COMMIT_TAG
  tags: [ qual-mcap-gcp ]
  script:
    - rustc --version; cargo --version
    - rustup target add aarch64-apple-darwin || true
    - cargo build --release --target aarch64-apple-darwin
    - mkdir -p dist
    - cp target/aarch64-apple-darwin/release/aifo-coder dist/aifo-coder-macos-arm64
    - file dist/aifo-coder-macos-arm64 | grep -qi 'Mach-O 64-bit arm64' || \
      { echo "not a Mach-O arm64"; exit 2; }
  artifacts:
    paths: [ dist/aifo-coder-macos-arm64 ]
    expire_in: 1 week
  rules:
    - if: $CI_COMMIT_TAG
- Update publish-release:
  needs:
    - { job: build-launcher, artifacts: true }
    - { job: build-launcher-macos, artifacts: true }
  release.assets.links add aifo-coder-macos-arm64 link.

Phase 3 — Makefile (developer convenience; optional for CI)
- Variables:
  MACOS_CROSS_IMAGE ?= $(IMAGE_PREFIX)-macos-cross-rust-builder:$(TAG)
  OSX_SDK_FILENAME ?= MacOSX13.3.sdk.tar.xz
- Targets:
  build-macos-cross-rust-builder:
    - Use $(DOCKER_BUILD) --target macos-cross-rust-builder and tag $(MACOS_CROSS_IMAGE).
  build-launcher-macos-cross:
    - docker run --rm \
      -v "$(PWD):/workspace" \
      -v "$(HOME)/.cargo/registry:/root/.cargo/registry" \
      -v "$(HOME)/.cargo/git:/root/.cargo/git" \
      -v "$(PWD)/target:/workspace/target" \
      $(MACOS_CROSS_IMAGE) sh -lc \
      'rustup target add aarch64-apple-darwin || true; \
       cargo build --release --target aarch64-apple-darwin'
    - Validate: target/aarch64-apple-darwin/release/aifo-coder exists.

Phase 4 — Validation
- Local (optional):
  - Place SDK under ci/osx/; make build-macos-cross-rust-builder; then
    make build-launcher-macos-cross; verify with file(1) for Mach-O arm64.
- CI:
  - Tag a commit and verify:
    - build-macos-cross-rust-builder completes.
    - build-launcher-macos produces dist/aifo-coder-macos-arm64 and passes file(1) check.
    - publish-release attaches both artifacts.

Phase 5 — Enhancements (optional)
- Add x86_64 macOS build and publish dist/aifo-coder-macos-x86_64.
- Pin osxcross to a specific commit for reproducibility; document pin and rationale.
- Add a small smoke test with otool -hv if available (best-effort); else rely on file(1).

Appendix: CI variables
- APPLE_SDK_URL (protected): HTTPS URL to the SDK .tar.xz.
- APPLE_SDK_SHA256 (masked + protected): expected SHA‑256 digest for integrity verification.
- APPLE_SDK_BASE64 (masked + protected, optional): base64 of the SDK .tar.xz (fallback when URL/artifacts
  are not available).
- OSX_SDK_FILENAME (optional; default MacOSX.sdk.tar.xz): stable filename used in CI workspace and COPY.
- RB_IMAGE (tests/lint base): unchanged; macOS jobs use the cross image only for building launcher.

Appendix: Reproducibility and caching
- Enable Kaniko cache via KANIKO_CACHE_ARGS. Use resource_group to serialize cross-image builds and
  improve cache locality. Reuse cargo registries by mounting in runtime build job.

Rollback
- If osxcross build fails: skip macOS jobs via rules; ship Linux artifacts only; investigate SDK ref,
  osxcross pin or package list.
- If macOS build fails (native deps): gate features for target_os="macos"; retry with clean cache.
- Rollback: remove build-macos-cross-rust-builder/build-launcher-macos jobs and Dockerfile stage refs.

End of specification.
