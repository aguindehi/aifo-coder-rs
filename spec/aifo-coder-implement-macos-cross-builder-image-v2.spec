Title: Implement macOS cross-builder image (osxcross) for aifo-coder
Version: v2
Status: Approved
Author: Migros AI Foundation
Date: 2025-11-10

Executive summary
- Objective: Produce macOS launcher binaries (aarch64-apple-darwin; optional x86_64) from Linux-only
  GitLab runners using osxcross with an Apple SDK injected via CI secrets.
- Approach: Add a Dockerfile stage macos-cross-rust-builder bundling osxcross and Apple SDK; build it
  in CI via Kaniko after decoding SDK into ci/osx/; compile the Rust launcher inside this image; ship
  artifacts and attach to releases alongside Linux binaries.
- Security/license: Never commit SDK. Use protected, masked CI variables; restrict jobs to tags or
  protected contexts; avoid logging SDK contents.

Plan validation and consistency check (v1 → v2)
- Image name: Standardize on macos-cross-rust-builder across Dockerfile, CI, Makefile and artifact
  references. v1 had mixed “macos-cross-builder”; v2 corrects all to “macos-cross-rust-builder”.
- Kaniko vs BuildKit: Kaniko cannot use RUN --mount=type=secret. v2 keeps existing secret mounts in
  other stages (already in repo) and ensures macos-cross-rust-builder uses COPY of a pre-decoded SDK.
- Cargo toolchain: v2 sets aarch64 macOS linker via env var (CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER)
  and CC/CXX/AR/RANLIB via osxcross tools (oa64/o64) to avoid .cargo/config coupling. This keeps CI
  independent from developer ~/.cargo/config.toml and fixes the clippy/linker mismatch in CI.
- Tool symlinks: osxcross tools often include Darwin minor suffix. v2 creates stable symlinks so env
  values remain constant across SDK versions (aarch64-apple-darwin-{ar,ranlib,strip}).
- Deployment target: v2 pins MACOSX_DEPLOYMENT_TARGET=11.0 to avoid linking newer symbols by default.
- CI jobs: v2 adds build-macos-cross-rust-builder (Kaniko) and build-launcher-macos using the image;
  publish-release is updated to include macOS artifact. Rules restrict usage to tags and default-branch
  manual runs, aligning with SDK secrecy.
- Smoke validation: v2 adds a “file” check in the build-launcher-macos job to assert Mach-O arm64.
- Optional x86_64: v2 documents optional x86_64 support using o64-clang and publishes a second artifact
  dist/aifo-coder-macos-x86_64, gated off by default to reduce complexity.

Design details
- Dockerfile: new stage macos-cross-rust-builder
  - Base: ${REGISTRY_PREFIX}rust:1-bookworm (consistent with rust-builder lineage).
  - Packages: clang llvm make cmake patch xz-utils unzip curl git python3 file ca-certificates.
  - osxcross: git clone tpoechtrager/osxcross (pin optional); UNATTENDED=1 osxcross/build.sh.
  - SDK: Provided as ci/osx/${OSX_SDK_FILENAME}, default MacOSX13.3.sdk.tar.xz; CI decodes beforehand.
  - Tool symlinks: Create stable aarch64-apple-darwin-{ar,ranlib,strip} and x86_64 equivalents.
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
  - build-macos-cross-rust-builder: extends container-build; before_script decodes APPLE_SDK_BASE64
    into ci/osx/${OSX_SDK_FILENAME}; sets KANIKO_BUILD_OPTIONS += --target macos-cross-rust-builder;
    pushes image to $CI_REGISTRY_IMAGE/aifo-coder-macos-cross-rust-builder:$CI_COMMIT_TAG (or :ci).
  - build-launcher-macos: runs cargo build --release --target aarch64-apple-darwin; copies artifact
    to dist/aifo-coder-macos-arm64; runs file(1) check to assert Mach-O 64-bit arm64.
  - publish-release: includes both Linux launcher (aifo-coder) and macOS launcher (dist/...-macos-arm64).
    Optional x86_64 artifact can be added when enabled.

- Makefile convenience (optional)
  - build-macos-cross-rust-builder: local build of macOS stage with registry prefix logic.
  - build-launcher-macos-cross: docker run macos-cross image and build macOS artifact; reuse cargo
    caches and target mounts; print path of target/aarch64-apple-darwin/release/aifo-coder.

- Artifact naming
  - Linux: aifo-coder (unchanged).
  - macOS arm64: dist/aifo-coder-macos-arm64.
  - macOS x86_64 (optional): dist/aifo-coder-macos-x86_64.

Security and compliance controls
- APPLE_SDK_BASE64: masked + protected; restricted to tags and default-branch manual runs.
- Jobs using SDK: never output SDK contents; only ls the decoded file for debug; do not artifact it.
- Image scope: macos-cross-rust-builder remains project-internal; do not mirror to public registries.

Acceptance criteria
- A tag pipeline successfully:
  1) Builds macos-cross-rust-builder image and pushes it to the project registry.
  2) Builds aifo-coder for aarch64-apple-darwin using that image.
  3) Verifies Mach-O arm64 via file(1) and publishes dist/aifo-coder-macos-arm64 as CI artifact.
  4) publish-release attaches both Linux and macOS artifacts to the release.

Risks, constraints and mitigations
- Apple SDK licensing: keep SDK out of repo; secret variable restricted; do not log or artifact SDK.
- Kaniko limitation: no BuildKit secrets; decode SDK to ci/osx/<name>.tar.xz and COPY in Dockerfile.
- Tool drift in osxcross: optionally pin osxcross to a known commit; document pin if instability occurs.
- Native deps on macOS: prefer crate features that link Apple frameworks rather than pkg-config libs.
- Linker mismatch: avoid ~/.cargo/config overrides in CI; set per-target env vars in the image.

Comprehensive phased implementation plan

Phase 0 — prerequisites (CI setup)
- Create protected, masked CI variable APPLE_SDK_BASE64 containing base64 of MacOSX13.3.sdk.tar.xz
  (or preferred SDK). Create optional variable OSX_SDK_FILENAME, default “MacOSX13.3.sdk.tar.xz”.
- Restrict jobs that use SDK to tags and default-branch manual runs; lock runners to project/group.

Phase 1 — Dockerfile: macos-cross-rust-builder stage (no secrets)
- Append after rust-builder:
  FROM ${REGISTRY_PREFIX}rust:1-bookworm AS macos-cross-rust-builder
  ENV DEBIAN_FRONTEND=noninteractive
  RUN apt-get update && apt-get install -y --no-install-recommends \
        clang llvm make cmake patch xz-utils unzip curl git python3 file ca-certificates \
      && rm -rf /var/lib/apt/lists/*
  WORKDIR /opt
  ARG OSX_SDK_FILENAME=MacOSX13.3.sdk.tar.xz
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
- Add job build-macos-cross-rust-builder (extends container-build):
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
  needs: [ build-macos-cross-rust-builder ]
  image: $CI_REGISTRY_IMAGE/aifo-coder-macos-cross-rust-builder:$CI_COMMIT_TAG
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
- Add variables:
  MACOS_CROSS_IMAGE ?= $(IMAGE_PREFIX)-macos-cross-rust-builder:$(TAG)
  OSX_SDK_FILENAME ?= MacOSX13.3.sdk.tar.xz
- Targets:
  build-macos-cross-rust-builder:
    - $(DOCKER_BUILD) --target macos-cross-rust-builder and tag $(MACOS_CROSS_IMAGE).
  build-launcher-macos-cross:
    - docker run --rm -v "$(PWD):/workspace" -v "$(HOME)/.cargo/registry:/root/.cargo/registry" \
      -v "$(HOME)/.cargo/git:/root/.cargo/git" -v "$(PWD)/target:/workspace/target" \
      $(MACOS_CROSS_IMAGE) sh -lc 'rustup target add aarch64-apple-darwin || true; \
                                   cargo build --release --target aarch64-apple-darwin'
    - Validate: target/aarch64-apple-darwin/release/aifo-coder exists; run file(1) locally if present.

Phase 4 — Validation
- Local (optional):
  - Place SDK under ci/osx/; make build-macos-cross-rust-builder; then
    make build-launcher-macos-cross; verify Mach-O arm64 via file(1).
- CI:
  - Tag a commit and verify:
    - build-macos-cross-rust-builder completes.
    - build-launcher-macos produces dist/aifo-coder-macos-arm64 and passes file(1) check.
    - publish-release attaches both artifacts.

Phase 5 — Enhancements (optional)
- Add x86_64 macOS build and publish dist/aifo-coder-macos-x86_64.
- Pin osxcross to a specific commit for reproducibility; document pin and rationale.
- Add a small smoke test running otool -hv if available (best-effort), else rely on file(1).

Appendix: CI variables
- APPLE_SDK_BASE64 (required, masked, protected): base64-encoded SDK .tar.xz.
- OSX_SDK_FILENAME (optional; default MacOSX13.3.sdk.tar.xz): filename used in COPY.
- RB_IMAGE (tests/lint base): unchanged; v2 adds macOS jobs using cross image.

Appendix: Reproducibility and caching
- Kaniko cache enabled via KANIKO_CACHE_ARGS; add resource_group to serialize rebuilds and improve
  cache locality; reuses cargo registries by mounting in runtime build job.

Rollback
- If osxcross build fails: skip macOS jobs (rules), ship Linux artifacts only; investigate SDK ref,
  osxcross pin or package list.
- If macOS build fails (native deps): gate features for target_os = "macos"; retry with clean cache.
- Rollback: remove build-macos-cross-rust-builder/build-launcher-macos jobs and Dockerfile stage refs.

End of specification.
