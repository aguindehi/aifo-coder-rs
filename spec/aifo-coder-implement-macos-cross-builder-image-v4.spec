Title: Implement macOS cross-builder image (osxcross) for aifo-coder – artifact-based v4
Version: v4
Status: Approved
Author: Migros AI Foundation
Date: 2025-11-16

Executive summary
- Objective: Build the aifo-coder macOS launcher (aarch64-apple-darwin; optional x86_64) on Linux CI
  using osxcross with an Apple SDK provided via CI. Adopt a secure, artifact-based flow.
- Approach: Add Dockerfile stage macos-cross-rust-builder bundling osxcross; create robust clang and
  ld wrappers to enforce Mach‑O linking; build the SDK-based image in CI after a producer job places
  the SDK tarball as an artifact; then compile the Rust launcher with that image and publish artifacts.
- Security/license: Do not commit the SDK to the repository. Prefer short‑lived, protected CI artifacts
  to move the SDK between jobs with checksum verification; allow masked/protected variables as a
  fallback. Restrict jobs handling the SDK to tags, schedules, or default‑branch manual runs. Cross
  image is internal; do not mirror publicly.

Plan validation and consistency checks (v3 → v4)
- Artifact-based SDK flow: v4 replaces variable-only decoding with a producer job that downloads or
  decodes the SDK and publishes it as a short‑lived artifact; consumer job fetches via needs.
- Stable SDK filename: Use ci/osx/MacOSX.sdk.tar.xz in the CI workspace; derive the versioned filename
  for osxcross/tarballs inside the Docker build automatically (inspecting the tarball).
- Linker/driver robustness: Provide wrapper oa64-clang/o64-clang with -B/opt/osxcross/target/bin and
  SDK sysroot; install an ld wrapper that prefers cctools ld64 (from osxcross), then ld64.lld, and
  finally ld.lld -flavor darwin. Avoid GNU ld entirely.
- PATH/LD hygiene: Do not prepend /opt/osxcross/target/bin to PATH for host tasks (cargo nextest install).
  Use absolute paths for mac-only steps; unset LD during nextest install/run to prevent host-linker
  conflicts.
- Tests: Add nextest-based E2E tests e2e_macos_cross_* that verify environment, SDK, C link, and Rust
  link inside the cross image. Run them in a dedicated CI job and via a Makefile target.

Design details

Dockerfile: new stage macos-cross-rust-builder
- Base: ${REGISTRY_PREFIX}rust:1-bookworm.
- Packages (minimal + cctools prerequisites):
  - clang llvm lld make cmake patch xz-utils unzip curl git python3 file ca-certificates
  - autoconf automake libtool pkg-config bison flex zlib1g-dev libxml2-dev libssl-dev
- osxcross build:
  - git clone tpoechtrager/osxcross (optionally pin via OSXCROSS_REF); UNATTENDED=1 osxcross/build.sh.
- SDK handling:
  - COPY ci/osx/MacOSX.sdk.tar.xz → /tmp/MacOSX.sdk.tar.xz (stable name).
  - Automatically derive the versioned tarball name (MacOSX<ver>.sdk.tar.xz) by inspecting the tarball
    contents; move into osxcross/tarballs/ before build.sh.
  - Optionally accept OSXCROSS_SDK_TARBALL passed from CI when URL basename is known.
- Tool aliases:
  - Create stable aarch64-apple-darwin-{ar,ranlib,strip} and x86_64 equivalents to avoid minor suffix drift.
- Clang wrapper fallbacks:
  - If osxcross-produced clang drivers aren’t present, create oa64-clang/o64-clang wrappers that:
    - read the SDK dir from /opt/osxcross/SDK/SDK_DIR.txt (with ls fallback),
    - exec Debian clang with -target <apple triple>, --sysroot="$SDK" and -B/opt/osxcross/target/bin.
- Linker wrapper:
  - Provide /opt/osxcross/target/bin/ld script that:
    - prefers /opt/osxcross/target/bin/ld64 (cctools),
    - then ld64.lld (if available),
    - finally ld.lld -flavor darwin (fallback).
- Environment:
  PATH=/opt/osxcross/target/bin:/usr/local/cargo/bin:/usr/local/rustup/bin:$PATH
  MACOSX_DEPLOYMENT_TARGET=11.0
  CC_aarch64_apple_darwin=oa64-clang
  CXX_aarch64_apple_darwin=oa64-clang++
  AR_aarch64_apple_darwin=aarch64-apple-darwin-ar
  RANLIB_aarch64_apple_darwin=aarch64-apple-darwin-ranlib
  CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER=/opt/osxcross/target/bin/oa64-clang
  (Optional x86_64):
  CC_x86_64_apple_darwin=o64-clang
  CXX_x86_64_apple_darwin=o64-clang++
  AR_x86_64_apple_darwin=x86_64-apple-darwin-ar
  RANLIB_x86_64_apple_darwin=x86_64-apple-darwin-ranlib
  CARGO_TARGET_X86_64_APPLE_DARWIN_LINKER=/opt/osxcross/target/bin/o64-clang
- rustup targets:
  - aarch64-apple-darwin (required), x86_64-apple-darwin (optional).
- Metadata for tests:
  - Write /opt/osxcross/SDK/SDK_NAME.txt with the tarball filename used.
  - Write /opt/osxcross/SDK/SDK_DIR.txt with the resolved SDK directory.

CI integration (.gitlab-ci.yml)

Producer: prepare-apple-sdk
- Stage: build; image: alpine:3.20; restricted to tags/schedules/default-branch manual runs.
- Variables: OSX_SDK_FILENAME=MacOSX.sdk.tar.xz.
- Script:
  - Download via APPLE_SDK_URL or decode APPLE_SDK_BASE64 into ci/osx/${OSX_SDK_FILENAME}.
  - Verify APPLE_SDK_SHA256 if provided; emit sidecar .sha256.
- Artifacts: ci/osx/${OSX_SDK_FILENAME}, ci/osx/${OSX_SDK_FILENAME}.sha256 (expire_in short).

Consumer: build-macos-cross-rust-builder (Kaniko)
- needs: prepare-apple-sdk with artifacts: true (optional true for flexibility).
- Verify checksum (APPLE_SDK_SHA256 or .sha256 artifact).
- Pass OSXCROSS_SDK_TARBALL from APPLE_SDK_URL basename when available.
- KANIKO_BUILD_OPTIONS += --target macos-cross-rust-builder; tag
  $CI_REGISTRY_IMAGE/aifo-coder-macos-cross-rust-builder:$CI_COMMIT_TAG (or :ci).
- resource_group to serialize cross-image builds; rules limited to tags and default‑branch manual runs.

Launcher builds
- build-launcher-macos (arm64):
  - Use the cross image; cargo build --release --target aarch64-apple-darwin.
  - Produce dist/aifo-coder-macos-arm64 and verify Mach‑O via file(1).
- build-launcher-macos-x86_64 (optional):
  - cargo build --release --target x86_64-apple-darwin.
  - Produce dist/aifo-coder-macos-x86_64 and verify Mach‑O via file(1).

E2E test job
- test-macos-cross-image:
  - Runs nextest e2e macOS cross tests inside the cross image.
  - Hygiene: export PATH to system cargo/rustup only and unset LD for nextest install/run to avoid
    host linker conflicts; tests use absolute wrapper paths (oa64-clang) with -B to select Darwin ld.
  - Run ignored tests via --run-ignored ignored-only and filter test(/^e2e_macos_cross_/).

Publish release
- publish-release: attach Linux and macOS artifacts; ensure links for aifo-coder, aifo-coder-macos-arm64,
  and aifo-coder-macos-x86_64.

Makefile convenience
- build-macos-cross-rust-builder: local build of the cross image; requires ci/osx/MacOSX.sdk.tar.xz.
- rebuild-macos-cross-rust-builder: no-cache rebuild with --pull to refresh base layers; purges buildx cache (best-effort).
- build-launcher-macos-cross(-arm64|‑x86_64): build macOS artifacts inside the cross image; show file(1).
- test-macos-cross-image: run the e2e tests inside the cross image with PATH/LD hygiene.

Artifact naming
- Linux: aifo-coder (unchanged).
- macOS arm64: dist/aifo-coder-macos-arm64.
- macOS x86_64 (optional): dist/aifo-coder-macos-x86_64.

Security and compliance controls
- Prefer APPLE_SDK_URL (protected) with APPLE_SDK_SHA256 (masked + protected) for integrity; allow
  APPLE_SDK_BASE64 as a fallback.
- Jobs using the SDK: do not log SDK contents; show only file metadata (ls -lh). Use short‑lived,
  protected artifacts for job‑to‑job transfer; restrict job scope to tags/schedules/default‑branch manual.
- Image scope: macos-cross-rust-builder is project-internal; avoid public mirroring.

Acceptance criteria
- A tag pipeline successfully:
  1) Runs prepare-apple-sdk and publishes ci/osx/MacOSX.sdk.tar.xz with .sha256 artifact.
  2) Builds macos-cross-rust-builder in Kaniko and pushes the image.
  3) Builds aifo-coder for aarch64-apple-darwin and (optionally) x86_64; verifies Mach‑O via file(1).
  4) Runs test-macos-cross-image with all e2e macOS cross tests passing.
  5) publish-release attaches Linux and macOS artifacts to the release.

Risks, constraints and mitigations
- Apple SDK licensing: keep SDK out of repo; decode from masked variable or download from protected URL; restrict jobs; avoid logging raw contents.
- Kaniko limitation: cannot use BuildKit secrets; rely on COPY of ci/osx/${OSX_SDK_FILENAME}. Use artifact flow to place SDK before build.
- osxcross drift: optionally pin OSXCROSS_REF to a known commit; document pin if instability occurs.
- Linker mismatch: avoid relying on PATH or GNU ld; enforce Darwin ld via -B in clang wrappers and ld wrapper script; set cargo linker envs explicitly.
- Host linker conflicts: during nextest install/run, keep PATH to system toolchains and unset LD; tests invoke absolute paths and set SDKROOT/OSX_SYSROOT.
- Native deps: prefer features linking Apple frameworks; avoid pkg-config Linux libs on mac targets.

Comprehensive phased implementation plan

Phase 0 — prerequisites (CI setup)
- Define APPLE_SDK_URL (protected) and APPLE_SDK_SHA256 (masked + protected). Optionally provide
  APPLE_SDK_BASE64 as a fallback. Set OSX_SDK_FILENAME (defaults to MacOSX.sdk.tar.xz).
- Use a producer job (prepare-apple-sdk) to download/verify the SDK and publish it as a short‑lived
  artifact. Restrict jobs using the SDK to tags, schedules, or default‑branch manual runs; lock
  runners to project/group and avoid logging raw contents.

Phase 1 — Dockerfile: macos-cross-rust-builder stage (no secrets)
  FROM ${REGISTRY_PREFIX}rust:1-bookworm AS macos-cross-rust-builder
  ENV DEBIAN_FRONTEND=noninteractive
  RUN apt-get update && apt-get install -y --no-install-recommends \
        clang llvm lld make cmake patch xz-utils unzip curl git python3 file ca-certificates \
        autoconf automake libtool pkg-config bison flex zlib1g-dev libxml2-dev libssl-dev \
      && rm -rf /var/lib/apt/lists/*
  WORKDIR /opt
  ARG OSX_SDK_FILENAME=MacOSX.sdk.tar.xz
  ARG OSXCROSS_REF
  ARG OSXCROSS_SDK_TARBALL
  COPY ci/osx/MacOSX.sdk.tar.xz /tmp/MacOSX.sdk.tar.xz
  RUN git clone --depth=1 https://github.com/tpoechtrager/osxcross.git osxcross && \
      if [ -n "${OSXCROSS_REF}" ]; then cd osxcross && git fetch --depth=1 origin "${OSXCROSS_REF}" && git checkout FETCH_HEAD && cd ..; fi && \
      SDK_TMP="/tmp/MacOSX.sdk.tar.xz" && \
      SDK_NAME="${OSXCROSS_SDK_TARBALL}" && \
      if [ -z "$SDK_NAME" ]; then TOP="$( (tar -tf "$SDK_TMP" 2>/dev/null || xz -dc "$SDK_TMP" 2>/dev/null | tar -tf - 2>/dev/null) | head -n1 || true)"; \
        VER="$(printf '%s\n' "$TOP" | sed -n -E 's#^(\./)?MacOSX([0-9][0-9.]*)\.sdk(/.*)?$#\2#p' | tr -d ' \t\r\n')"; \
        if [ -n "$VER" ]; then SDK_NAME="MacOSX${VER}.sdk.tar.xz"; fi; \
      fi && \
      if [ -z "$SDK_NAME" ]; then echo "warning: could not derive SDK version; using original name"; SDK_NAME="${OSX_SDK_FILENAME}"; fi && \
      mkdir -p osxcross/tarballs && \
      mv "$SDK_TMP" "osxcross/tarballs/${SDK_NAME}" && \
      UNATTENDED=1 osxcross/build.sh && \
      mkdir -p /opt/osxcross/SDK && \
      printf '%s\n' "${SDK_NAME}" > /opt/osxcross/SDK/SDK_NAME.txt || true && \
      SDK_DIR="$(ls -d /opt/osxcross/target/SDK/MacOSX*.sdk 2>/dev/null | head -n1)" && \
      [ -n "$SDK_DIR" ] && printf '%s\n' "$SDK_DIR" > /opt/osxcross/SDK/SDK_DIR.txt || true
  RUN set -e; cd /opt/osxcross/target/bin; \
      for t in ar ranlib strip; do \
        ln -sf "$(ls aarch64-apple-darwin*-$t 2>/dev/null | head -n1)" aarch64-apple-darwin-$t || true; \
        ln -sf "$(ls x86_64-apple-darwin*-$t 2>/dev/null | head -n1)" x86_64-apple-darwin-$t || true; \
      done && \
      if [ ! -x oa64-clang ]; then CAND="$(ls -1 aarch64-apple-darwin*-clang 2>/dev/null | head -n1 || true)"; \
        if [ -n "$CAND" ] && [ -x "$CAND" ]; then ln -sf "$CAND" oa64-clang; \
        else printf '%s\n' '#!/bin/sh' \
          'SDK="$(cat /opt/osxcross/SDK/SDK_DIR.txt 2>/dev/null || ls -d /opt/osxcross/target/SDK/MacOSX*.sdk 2>/dev/null | head -n1)"' \
          'exec clang -target aarch64-apple-darwin --sysroot="$SDK" -B/opt/osxcross/target/bin "$@"' > oa64-clang && chmod 0755 oa64-clang; fi; \
      fi && \
      if [ ! -x o64-clang ]; then CAND="$(ls -1 x86_64-apple-darwin*-clang 2>/dev/null | head -n1 || true)"; \
        if [ -n "$CAND" ] && [ -x "$CAND" ]; then ln -sf "$CAND" o64-clang; \
        else printf '%s\n' '#!/bin/sh' \
          'SDK="$(cat /opt/osxcross/SDK/SDK_DIR.txt 2>/dev/null || ls -d /opt/osxcross/target/SDK/MacOSX*.sdk 2>/dev/null | head -n1)"' \
          'exec clang -target x86_64-apple-darwin --sysroot="$SDK" -B/opt/osxcross/target/bin "$@"' > o64-clang && chmod 0755 o64-clang; fi; \
      fi && \
      printf '%s\n' '#!/bin/sh' 'set -e' \
        'if [ -x "/opt/osxcross/target/bin/ld64" ]; then exec /opt/osxcross/target/bin/ld64 "$@"; fi' \
        'if command -v ld64.lld >/dev/null 2>&1; then exec "$(command -v ld64.lld)" "$@"; fi' \
        'if command -v ld.lld   >/dev/null 2>&1; then exec "$(command -v ld.lld)" -flavor darwin "$@"; fi' \
        'echo "error: Mach-O ld not found (need cctools ld64 or ld64.lld)" >&2; exit 127' \
        > ld && chmod 0755 ld
  ENV RUSTUP_HOME="/usr/local/rustup" \
      CARGO_HOME="/usr/local/cargo" \
      PATH="/opt/osxcross/target/bin:/usr/local/cargo/bin:/usr/local/rustup/bin:${PATH}" \
      MACOSX_DEPLOYMENT_TARGET=11.0 \
      CC_aarch64_apple_darwin=oa64-clang \
      CXX_aarch64_apple_darwin=oa64-clang++ \
      AR_aarch64_apple_darwin=aarch64-apple-darwin-ar \
      RANLIB_aarch64_apple_darwin=aarch64-apple-darwin-ranlib \
      CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER=/opt/osxcross/target/bin/oa64-clang
  ENV CC_x86_64_apple_darwin=o64-clang \
      CXX_x86_64_apple_darwin=o64-clang++ \
      AR_x86_64_apple_darwin=x86_64-apple-darwin-ar \
      RANLIB_x86_64_apple_darwin=x86_64-apple-darwin-ranlib \
      CARGO_TARGET_X86_64_APPLE_DARWIN_LINKER=/opt/osxcross/target/bin/o64-clang
  RUN /usr/local/cargo/bin/rustup target add aarch64-apple-darwin x86_64-apple-darwin || true
  RUN /usr/local/cargo/bin/cargo install cargo-nextest --locked || true && \
      rm -rf /usr/local/cargo/registry /usr/local/cargo/git

Phase 2 — CI: build macOS cross image (Kaniko), tests, and macOS launcher
- prepare-apple-sdk: producer job; see above.
- build-macos-cross-rust-builder: consumer job; needs artifacts from prepare-apple-sdk; verifies checksum.
- test-macos-cross-image: run nextest e2e tests inside the cross image with PATH/LD hygiene.
- build-launcher-macos (+ x86_64): build macOS artifacts and verify Mach‑O via file(1).
- publish-release: attach Linux and macOS artifacts.

Phase 3 — Makefile (developer convenience)
- Targets:
  - build-macos-cross-rust-builder: build Docker stage locally.
  - rebuild-macos-cross-rust-builder: force rebuild (no cache, pull fresh bases).
  - build-launcher-macos-cross[-arm64|‑x86_64]: build macOS artifacts using the cross image.
  - test-macos-cross-image: run nextest e2e macOS tests inside the cross image.

Phase 4 — Validation
- Local (optional):
  - Place SDK under ci/osx/; make build-macos-cross-rust-builder; then make test-macos-cross-image;
    verify tests pass; optionally build launchers via Makefile and validate with file(1).
- CI:
  - Tag a commit and verify:
    - prepare-apple-sdk produces SDK artifacts and passes checksum.
    - build-macos-cross-rust-builder completes.
    - test-macos-cross-image passes all e2e macOS tests.
    - build-launcher-macos produces dist/aifo-coder-macos-arm64 and passes file(1) check.
    - publish-release attaches Linux and macOS artifacts.

Appendix: CI variables
- APPLE_SDK_URL (protected): HTTPS URL to the SDK .tar.xz.
- APPLE_SDK_SHA256 (masked + protected): expected SHA‑256 digest for integrity verification.
- APPLE_SDK_BASE64 (masked + protected, optional): base64 of the SDK .tar.xz (fallback when URL/artifacts
  are not available).
- OSX_SDK_FILENAME (optional; default MacOSX.sdk.tar.xz): stable filename used in CI workspace and COPY.
- RB_IMAGE (tests/lint base): unchanged; macOS jobs use the cross image for mac-only tasks.

Appendix: Reproducibility and caching
- Enable Kaniko cache via KANIKO_CACHE_ARGS. Use resource_group to serialize cross-image builds and
  improve cache locality. Reuse cargo registries by mounting in runtime build job.
- Local Makefile rebuild target supports --no-cache and --pull with buildx/docker.

Rollback
- If osxcross build fails: skip macOS jobs via rules; investigate SDK tarball, osxcross pin, or package list.
- If macOS build fails (native deps): gate features for target_os="macos"; retry with clean cache.
- If e2e tests fail due to host linker conflicts: ensure PATH/LD hygiene in test jobs; rely on absolute
  wrapper paths and -B/opt/osxcross/target/bin.

End of specification (v4).
