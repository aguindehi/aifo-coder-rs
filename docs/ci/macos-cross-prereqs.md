# macOS cross-builder prerequisites (Phase 0)

This document captures the CI prerequisites required to build the aifo-coder macOS
launcher on Linux via osxcross as described in:
spec/aifo-coder-implement-macos-cross-builder-image-v3.spec (v3).

Goals
- Do not commit the Apple SDK to the repository.
- Prefer short-lived, scoped CI artifacts to move the SDK between jobs; restrict retention
  and job scope.
- If artifacts are not viable, use masked and protected variables as a fallback.
- Restrict jobs that handle the SDK to tag pipelines and (optionally) default-branch manual
  runs.

Recommended: artifact-based exchange between jobs
- Overview: a producer job downloads or assembles the SDK into ci/osx/${OSX_SDK_FILENAME}
  and publishes it as an artifact with a short expire_in. Consumer jobs declare needs with
  artifacts: true so GitLab transfers the file directly.
- Pros:
  - Avoids huge masked variables (a 70–100MB base64 string is slow and error-prone).
  - Clear data lineage with checksums and retention controls.
  - Easy to scope to protected tags and default-branch manual runs.
- Cons:
  - Artifacts exist on the GitLab instance for the retention period; keep expire_in short
    and scope jobs tightly.

Minimal CI pattern
prepare-apple-sdk:
  stage: build
  image: alpine:3.20
  script:
    - set -euo pipefail
    - mkdir -p ci/osx
    - curl -fL "$APPLE_SDK_URL" -o "ci/osx/${OSX_SDK_FILENAME:-MacOSX.sdk.tar.xz}"
    - '[ -n "${APPLE_SDK_SHA256:-}" ] && echo "${APPLE_SDK_SHA256}  ci/osx/${OSX_SDK_FILENAME:-MacOSX.sdk.tar.xz}" | sha256sum -c - || echo "Warning: APPLE_SDK_SHA256 not set; skipping verification." >&2'
    - sha256sum "ci/osx/${OSX_SDK_FILENAME:-MacOSX.sdk.tar.xz}" |
      tee "ci/osx/${OSX_SDK_FILENAME:-MacOSX.sdk.tar.xz}.sha256" >/dev/null
  artifacts:
    expire_in: 1 week
    paths:
      - ci/osx/${OSX_SDK_FILENAME:-MacOSX.sdk.tar.xz}
      - ci/osx/${OSX_SDK_FILENAME:-MacOSX.sdk.tar.xz}.sha256

build-macos-cross-rust-builder:
  needs:
    - job: prepare-apple-sdk
      artifacts: true

Policy recommendations
- Scope producer/consumer jobs to:
  - Tag pipelines (preferred)
  - Default-branch manual runs (allowed)
- Lock runners and never expose raw contents in logs (print only file metadata).
- Use a short expire_in (hours or days), not weeks, unless required.

Alternative: variable-based fallback (APPLE_SDK_BASE64)

Required CI variables (GitLab)
- APPLE_SDK_URL: protected
  - HTTPS URL to the SDK tarball, e.g. MacOSX13.3.sdk.tar.xz
- APPLE_SDK_SHA256: masked + protected
  - Expected SHA-256 checksum for integrity verification.

Optional CI variables
- APPLE_SDK_BASE64: masked + protected (fallback)
  - Contents: base64 of the SDK tarball. Used only if APPLE_SDK_URL is unset.
- OSX_SDK_FILENAME (default: MacOSX.sdk.tar.xz)
  - Generic stable filename used in CI build context; version is carried by APPLE_SDK_URL and APPLE_SDK_SHA256.

How to create APPLE_SDK_BASE64 locally
1) Ensure you have a .tar.xz Apple SDK file, e.g. MacOSX13.3.sdk.tar.xz
   - Download MacOSX:
     curl -LO https://github.com/joseluisq/macosx-sdks/releases/download/13.3/MacOSX13.3.sdk.tar.xz
     echo "518e35eae6039b3f64e8025f4525c1c43786cc5cf39459d609852faf091e34be MacOSX13.3.sdk.tar.xz" | sha256sum -c
   - This should echo "MacOSX13.3.sdk.tar.xz: OK"

2) Base64 encode it without line wrapping:
   - Linux:
     base64 -w0 MacOSX13.3.sdk.tar.xz > MacOSX13.3.sdk.tar.xz.b64
   - macOS (BSD base64):
     base64 -i MacOSX13.3.sdk.tar.xz > MacOSX13.3.sdk.tar.xz.b64

3) Open your project’s CI variables settings and create a protected, masked variable:
   - Key: APPLE_SDK_BASE64
   - Value: paste the contents of MacOSX13.3.sdk.tar.xz.b64
   - Masked: enabled
   - Protected: enabled

4) (Optional) Create another variable:
   - Key: OSX_SDK_FILENAME
   - Value: MacOSX13.3.sdk.tar.xz
   - Protected: enabled

Job restrictions (policy)
- Jobs that consume APPLE_SDK_BASE64 must be restricted to:
  - Tag pipelines (preferred)
  - Default branch manual runs (allowed)
- Lock runners to your project/group and never expose the raw SDK in logs or artifacts.

Decoding helper
- Use the script ci/bin/decode-apple-sdk.sh to decode the variable into the expected path:
  - ci/osx/${OSX_SDK_FILENAME:-MacOSX.sdk.tar.xz}

Notes
- Kaniko cannot use BuildKit RUN secret mounts; therefore we rely on a COPY of ci/osx/${OSX_SDK_FILENAME}
  in the macos-cross-rust-builder Dockerfile stage (Phase 1).
- The SDK file must exist in the build context before the Docker build starts.

Security reminders
- Never commit the SDK to the repository.
- Do not print the SDK contents in CI logs (only file names/metadata like ls -lh).
- Use short-lived artifacts (expire_in hours/days) limited to protected pipelines/runners.
- Verify integrity using APPLE_SDK_SHA256 in both producer and consumer jobs.
- Keep variables masked and protected, and scope jobs as described above.
