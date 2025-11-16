Title: Implement macOS cross-builder image (osxcross) for aifo-coder
Version: v1
Status: Approved
Author: Migros AI Foundation
Date: 2025-11-10

Executive summary
- Objective: Produce macOS launcher binaries (aarch64-apple-darwin; optional x86_64) from Linux-only
  GitLab runners by using osxcross with an Apple SDK provided via CI secrets.
- Approach: Add a dedicated Dockerfile stage macos-cross-rust-builder that bundles osxcross; build it in
  CI via Kaniko using an SDK injected into the build context; compile the Rust launcher inside this
  image; publish artifacts and attach them to releases alongside Linux binaries.
- Security/license: The SDK is never committed or published; it is injected from a protected, masked
  CI variable and used only during image builds on protected/tag pipelines.

Validation of current repository and plan
- Dockerfile: No macOS cross stage exists yet. BuildKit secret mounts appear in other stages; Kaniko
  cannot use RUN --mount=type=secret, so the macOS stage must use COPY of a pre-decoded SDK file.
- Makefile: No convenience targets for macOS cross-building; aligns with adding developer targets.
- .gitlab-ci.yml: No jobs for cross image nor macOS launcher build; require two jobs and release
  wiring. Existing “container-build” component supports Kaniko and target selection.
- Wrapper (aifo-coder): Not used for cross builds in CI; no changes required.

Risks, constraints, and mitigations
- Apple SDK licensing: Keep SDK out of repo. Use protected, masked CI variable APPLE_SDK_BASE64 and
  restrict jobs to tags/protected contexts; never log or artifact the SDK.
- Kaniko limitation: No BuildKit secrets; decode SDK to ci/osx/<name>.tar.xz before build and COPY.
- Tool drift: Pin osxcross to a known good ref if instability is observed (optional).
- Native deps on macOS: Prefer crate feature flags that use Apple frameworks over pkg-config where
  applicable; set MACOSX_DEPLOYMENT_TARGET=11.0 to avoid newer symbol linkages.

Design details
- New Dockerfile stage macos-cross-rust-builder
  - Base: ${REGISTRY_PREFIX}rust:1-bookworm (same base lineage as rust-builder).
  - Packages: clang llvm make cmake patch xz-utils unzip curl git python3 file ca-certificates.
  - osxcross: git clone tpoechtrager/osxcross (optionally pin); UNATTENDED=1 osxcross/build.sh.
  - SDK: Provided as ci/osx/${OSX_SDK_FILENAME}, where ${OSX_SDK_FILENAME} defaults to
    MacOSX13.3.sdk.tar.xz; Kaniko job places this file into the context before build.
  - Tool symlinks: Create stable aarch64-apple-darwin-{ar,ranlib,strip} and x86_64 equivalents that
    do not include the Darwin minor suffix so CC/AR/RANLIB envs are stable across SDK versions.
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
  - build-macos-cross-rust-builder: extends container-build; before_script decodes APPLE_SDK_BASE64 into
    ci/osx/${OSX_SDK_FILENAME}; sets KANIKO_BUILD_OPTIONS += --target macos-cross-rust-builder; publishes
    image to $CI_REGISTRY_IMAGE/aifo-coder-macos-cross-rust-builder:$CI_COMMIT_TAG (or :ci).
  - build-launcher-macos: uses RB_IMAGE=$CI_REGISTRY_IMAGE/aifo-coder-macos-cross-rust-builder:<tag>;
    runs cargo build --release --target aarch64-apple-darwin; copies to dist/aifo-coder-macos-arm64;
    publishes as artifact.
  - publish-release: needs artifacts from build-launcher (Linux) and build-launcher-macos (macOS);
    attaches both to the GitLab release.

- Makefile convenience (optional)
  - build-macos-cross-builder: local build of the macos-cross-builder stage with REG prefix.
  - build-launcher-macos-cross: docker run macos-cross-builder and build the macOS artifact; reuse
    local cargo caches and target dir mounts.

- Artifact naming
  - Linux: aifo-coder (unchanged).
  - macOS arm64: dist/aifo-coder-macos-arm64.
  - macOS x86_64 (optional): dist/aifo-coder-macos-x86_64.

Security and compliance controls
- APPLE_SDK_BASE64: masked + protected; only available to protected branches or tags.
- Jobs using the SDK: restricted by rules to tags and optionally default-branch manual runs.
- No SDK file is stored in repo history or uploaded as artifact.
- macos-cross-builder image remains project-internal; do not mirror to public registries.

Acceptance criteria
- A tag pipeline successfully:
  1) Builds macos-cross-builder image and pushes it to the project registry.
  2) Builds aifo-coder for aarch64-apple-darwin using that image.
  3) Publishes dist/aifo-coder-macos-arm64 as CI artifact.
  4) publish-release attaches both Linux and macOS artifacts to the release.
- The produced macOS binary is a valid Mach-O for arm64 (file reports “Mach-O 64-bit arm64”).

Failure modes and rollback
- If osxcross build fails: Skip cross jobs (rules), ship Linux artifacts only; investigate SDK ref,
  osxcross pin or package list.
- If macOS build fails (crate native deps): Gate features for target_os = "macos" as needed.
- Rollback: Remove build-macos-cross-builder/build-launcher-macos jobs and Dockerfile stage refs.

Comprehensive phased implementation plan

Phase 0 — prerequisites (CI setup)
- Create protected, masked CI variable APPLE_SDK_BASE64 containing the base64 of MacOSX13.3.sdk.tar.xz
  (or preferred SDK). Optionally create OSX_SDK_FILENAME (“MacOSX13.3.sdk.tar.xz”).
- Restrict the macOS cross-image build job to tags and default-branch manual runs.

Phase 1 — Dockerfile: macos-cross-rust-builder stage (no BuildKit secrets)
- Add a stage after rust-builder:
  FROM ${REGISTRY_PREFIX}rust:1-bookworm AS macos-cross-rust-builder
  ENV DEBIAN_FRONTEND=noninteractive
  RUN apt-get update && apt-get install -y --no-install-recommends \
        clang llvm make cmake patch xz-utils unzip curl git python3 file ca-certificates \
      && rm -rf /var/lib/apt/lists/*
  WORKDIR /opt
  ARG OSX_SDK_FILENAME=MacOSX13.3.sdk.tar.xz
  # Expect CI to place the SDK under ci/osx/ before build
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

Phase 2 — CI: build macos cross image (Kaniko) and macOS launcher
- Add job build-macos-cross-builder (extends container-build):
  variables:
    TARGET_NAME: "macos-cross-rust-builder"
    IMAGE_PATH_EXTRA: "macos-cross-rust-builder"
    OSX_SDK_FILENAME: "MacOSX13.3.sdk.tar.xz"
  before_script:
    - mkdir -p ci/osx
    - test -n "$APPLE_SDK_BASE64"
    - echo "$APPLE_SDK_BASE64" | base64 -d > "ci/osx/${OSX_SDK_FILENAME}"
    - KANIKO_BUILD_OPTIONS="${KANIKO_BUILD_OPTIONS} --target ${TARGET_NAME}"
  rules:
    - if: $CI_COMMIT_TAG
    - if: $CI_COMMIT_BRANCH == $CI_DEFAULT_BRANCH
      when: manual
- Add job build-launcher-macos:
  needs: [ build-macos-cross-builder ]
  image: $CI_REGISTRY_IMAGE/aifo-coder-macos-cross-rust-builder:$CI_COMMIT_TAG
  script:
    - rustc --version; cargo --version
    - rustup target add aarch64-apple-darwin || true
    - cargo build --release --target aarch64-apple-darwin
    - mkdir -p dist
    - cp target/aarch64-apple-darwin/release/aifo-coder dist/aifo-coder-macos-arm64
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
- Add:
  MACOS_CROSS_IMAGE ?= $(IMAGE_PREFIX)-macos-cross-rust-builder:$(TAG)
  OSX_SDK_FILENAME ?= MacOSX13.3.sdk.tar.xz
- Targets:
  build-macos-cross-rust-builder:
    - Uses $(DOCKER_BUILD) --target macos-cross-rust-builder and tags $(MACOS_CROSS_IMAGE).
  build-launcher-macos-cross:
    - docker run --rm -v "$(PWD):/workspace" -v "$(HOME)/.cargo/registry:/root/.cargo/registry"
      -v "$(HOME)/.cargo/git:/root/.cargo/git" -v "$(PWD)/target:/workspace/target"
      $(MACOS_CROSS_IMAGE) sh -lc 'rustup target add aarch64-apple-darwin || true;
                                   cargo build --release --target aarch64-apple-darwin'
    - Validate existence: target/aarch64-apple-darwin/release/aifo-coder

Phase 4 — Validation
- Local (optional):
  - Place SDK archive under ci/osx/; run make build-macos-cross-builder; then
    make build-launcher-macos-cross; verify binary exists and file(1) shows Mach-O 64-bit arm64.
- CI:
  - Tag a commit; verify:
    - build-macos-cross-builder completes.
    - build-launcher-macos produces dist/aifo-coder-macos-arm64.
    - publish-release attaches both artifacts.

Phase 5 — Enhancements (optional)
- Add x86_64 macOS build in same job; publish dist/aifo-coder-macos-x86_64.
- Pin osxcross to a specific commit for reproducibility; document the pin in this spec.
- Add a “file” smoke check in CI to assert Mach-O output and arch.

Appendix: CI variables
- APPLE_SDK_BASE64 (required, masked, protected): base64-encoded SDK .tar.xz.
- OSX_SDK_FILENAME (optional; default MacOSX13.3.sdk.tar.xz): filename used in COPY.

Appendix: Reproducibility and caching
- The cross image build is cached by Kaniko with cache TTL; associate a resource_group to serialize
  rebuilds; reuse cargo registries by mounting .cargo in runtime jobs (handled by default in our
  runner images or via Makefile convenience).

End of specification.
