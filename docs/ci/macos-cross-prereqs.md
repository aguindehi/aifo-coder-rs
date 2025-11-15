# macOS cross-compilation in GitLab CI: prerequisites and setup
Last updated: 2025-11-15

This guide explains how to configure GitLab CI to build the aifo-coder macOS launcher on Linux
using an osxcross-based builder image. It covers prerequisites, required CI variables, the
recommended artifact-based flow between jobs, a fallback using a masked variable, and security
policy.

Overview
- We do not commit Apple SDK contents to the repository.
- We prefer a file-based exchange using GitLab artifacts:
  - A producer job downloads the Apple SDK into a stable filename: ci/osx/MacOSX.sdk.tar.xz.
  - It verifies integrity with APPLE_SDK_SHA256 and publishes the SDK as a short-lived artifact.
  - Consumer jobs fetch the artifact via needs: with artifacts: true and verify integrity again.
- If artifacts are unavailable, a fallback variable APPLE_SDK_BASE64 can be decoded just-in-time.

What gets built
- A Kaniko-built macOS cross image that embeds the osxcross SDK for linking:
  - build-macos-cross-rust-builder produces aifo-coder-macos-cross-rust-builder.
- The launcher binaries for macOS are built using that image in build-launcher-macos and
  build-launcher-macos-x86_64.

Prerequisites
- Legal/licensing
  - Ensure you have the rights to download and use the Apple SDK you reference via APPLE_SDK_URL.
- Runners
  - Linux container runners with network egress to your SDK URL.
  - Sufficient disk space (SDK ~0.5–1.5 GB unpacked; ~50–100 MB compressed).
  - Recommended: project/group-scoped runners with restricted tags (e.g., qual-mcap-gcp).
- Repository
  - No SDK files are stored in git.
  - The build expects the SDK to be present as ci/osx/MacOSX.sdk.tar.xz at build time.

Required GitLab CI variables
- APPLE_SDK_URL (protected)
  - HTTPS URL to the SDK tarball (e.g., MacOSX13.3.sdk.tar.xz hosted on an internal or trusted source).
- APPLE_SDK_SHA256 (masked + protected)
  - Exact SHA-256 digest (64 hex characters) for the tarball at APPLE_SDK_URL.
  - Used for integrity verification in both producer and consumer jobs.

Optional CI variables
- APPLE_SDK_BASE64 (masked + protected)
  - Fallback only. Base64-encoded contents of the SDK tarball, decoded on-the-fly in CI.
  - Prefer APPLE_SDK_URL + APPLE_SDK_SHA256 for reliability and speed.
- OSX_SDK_FILENAME
  - Stable local filename for the SDK inside the CI workspace.
  - Default: MacOSX.sdk.tar.xz (keeps the local path constant while the version lives in the URL/checksum).

Recommended pipeline wiring (artifact-based flow)
- Producer job: prepare-apple-sdk
  - Downloads the SDK into ci/osx/MacOSX.sdk.tar.xz (stable name), verifies checksum, and publishes artifacts.
- Consumer job: build-macos-cross-rust-builder
  - Declares needs on prepare-apple-sdk with artifacts: true to fetch files automatically.
  - Verifies checksum again, then proceeds to build the cross image.

Example: producer job (prepare-apple-sdk)
Use this job as-is; it prefers APPLE_SDK_URL + APPLE_SDK_SHA256 and falls back to APPLE_SDK_BASE64.

```yaml
prepare-apple-sdk:
  stage: build
  image: alpine:3.20
  interruptible: true
  timeout: 15m
  tags:
    - qual-mcap-gcp
  variables:
    OSX_SDK_FILENAME: "MacOSX.sdk.tar.xz"
  script:
    - |
      set -euo pipefail
      mkdir -p ci/osx
      if [ -n "${APPLE_SDK_URL:-}" ]; then
        apk add --no-cache curl >/dev/null
        src="$(basename "$APPLE_SDK_URL")"
        tmp="ci/osx/${src}"
        echo "Downloading Apple SDK from APPLE_SDK_URL into ${tmp} ..."
        curl -fL --retry 3 --connect-timeout 10 --max-time 600 "$APPLE_SDK_URL" -o "${tmp}"
        if [ -n "${APPLE_SDK_SHA256:-}" ]; then
          echo "${APPLE_SDK_SHA256}  ${tmp}" | sha256sum -c -
        else
          echo "Warning: APPLE_SDK_SHA256 not set; skipping pre-move verification." >&2
        fi
        mv -f "${tmp}" "ci/osx/${OSX_SDK_FILENAME}"
      elif [ -n "${APPLE_SDK_BASE64:-}" ]; then
        echo "Decoding APPLE_SDK_BASE64 into ci/osx/${OSX_SDK_FILENAME} (fallback) ..."
        printf '%s' "$APPLE_SDK_BASE64" | base64 -d > "ci/osx/${OSX_SDK_FILENAME}"
      else
        echo "Error: neither APPLE_SDK_URL nor APPLE_SDK_BASE64 is set. Provide a URL (preferred) or a base64 fallback." >&2
        exit 1
      fi
      ls -lh "ci/osx/${OSX_SDK_FILENAME}"
      # Verify again against stable filename if a checksum is provided
      if [ -n "${APPLE_SDK_SHA256:-}" ]; then
        echo "${APPLE_SDK_SHA256}  ci/osx/${OSX_SDK_FILENAME}" | sha256sum -c -
      else
        echo "Warning: APPLE_SDK_SHA256 not set; computing and storing checksum only." >&2
      fi
      sha256sum "ci/osx/${OSX_SDK_FILENAME}" | tee "ci/osx/${OSX_SDK_FILENAME}.sha256" >/dev/null
      if command -v xz >/dev/null 2>&1; then xz -t "ci/osx/${OSX_SDK_FILENAME}" || echo "WARNING: xz test failed" >&2; fi
  artifacts:
    expire_in: 1 week
    paths:
      - ci/osx/${OSX_SDK_FILENAME}
      - ci/osx/${OSX_SDK_FILENAME}.sha256
  rules:
    - if: $CI_COMMIT_TAG
      when: on_success
    - if: $CI_PIPELINE_SOURCE == "schedule"
      when: on_success
    - if: $CI_COMMIT_BRANCH == $CI_DEFAULT_BRANCH
      when: manual
```

Example: consumer job (build-macos-cross-rust-builder)
This job consumes the artifact and verifies integrity again. It can also fall back to APPLE_SDK_BASE64.

```yaml
build-macos-cross-rust-builder:
  stage: build
  needs:
    - job: prepare-apple-sdk
      artifacts: true
      optional: true
  variables:
    TARGET_NAME: "macos-cross-rust-builder"
    IMAGE_PATH_EXTRA: "macos-cross-rust-builder"
    OSX_SDK_FILENAME: "MacOSX.sdk.tar.xz"
  before_script:
    - mkdir -p ci/osx
    - |
      if [ -f "ci/osx/${OSX_SDK_FILENAME}" ]; then
        echo "Using SDK artifact: ci/osx/${OSX_SDK_FILENAME}"
      elif found="$(ls -1 ci/osx/MacOSX*.sdk.tar.* 2>/dev/null | head -n1)"; then
        echo "Found legacy SDK artifact: ${found}"
        export OSX_SDK_FILENAME="$(basename "${found}")"
      elif [ -n "${APPLE_SDK_BASE64:-}" ]; then
        echo "Decoding APPLE_SDK_BASE64 into ci/osx/${OSX_SDK_FILENAME} ..."
        /bin/sh ci/bin/decode-apple-sdk.sh
      else
        echo "Error: ci/osx/${OSX_SDK_FILENAME} not found and APPLE_SDK_BASE64 not set." >&2
        echo "Hint: ensure the 'prepare-apple-sdk' job runs in the same pipeline and is listed in needs (artifacts: true)." >&2
        exit 1
      fi
      if [ -n "${APPLE_SDK_SHA256:-}" ]; then
        echo "${APPLE_SDK_SHA256}  ci/osx/${OSX_SDK_FILENAME}" | sha256sum -c -
      elif [ -f "ci/osx/${OSX_SDK_FILENAME}.sha256" ]; then
        sha256sum -c "ci/osx/${OSX_SDK_FILENAME}.sha256"
      else
        echo "Warning: no APPLE_SDK_SHA256 provided and no .sha256 artifact found; skipping verification." >&2
      fi
    - ls -lh "ci/osx/${OSX_SDK_FILENAME}"
```

Fallback: variable-based decoding helper
- The script ci/bin/decode-apple-sdk.sh decodes APPLE_SDK_BASE64 into ci/osx/${OSX_SDK_FILENAME}
  and verifies APPLE_SDK_SHA256 when provided. Use this only when artifacts are not practical.

```sh
# Example usage in a job's before_script:
export OSX_SDK_FILENAME="MacOSX.sdk.tar.xz"
export APPLE_SDK_BASE64="…masked…"
export APPLE_SDK_SHA256="…64 hex…"
sh ci/bin/decode-apple-sdk.sh
```

Local development
- If you want to build the cross image locally, place the SDK at:
  - ci/osx/MacOSX.sdk.tar.xz
- Then run:
  - make build-macos-cross-rust-builder

Security and policy
- Keep APPLE_SDK_SHA256 masked + protected; keep APPLE_SDK_URL protected.
- Restrict jobs that handle the SDK to:
  - Tag pipelines (preferred),
  - Default-branch manual runs, and schedules for periodic refresh.
- Use short artifact retention (expire_in) and project/group-scoped runners.
- Never print SDK contents; only print metadata like ls -lh.
- Verify checksum in both producer and consumer jobs.

Troubleshooting
- Download fails (HTTP or TLS)
  - Ensure the runner can reach APPLE_SDK_URL and the URL is correct (no redirects requiring auth).
- Checksum mismatch
  - Recompute the SHA-256 for the exact tarball served at APPLE_SDK_URL and update APPLE_SDK_SHA256.
- Artifact missing in consumer
  - Confirm build-macos-cross-rust-builder declares needs on prepare-apple-sdk with artifacts: true.
  - Ensure both jobs run in the same pipeline and the producer succeeded.
- Fallback variable too large/slow
  - Prefer the artifact-based flow. If you must use base64, ensure CI quotas and job timeouts are sufficient.

Notes
- Kaniko cannot read BuildKit RUN secrets; the SDK must be present in the build context and copied during
  the Docker build of the cross image.
- The stable filename MacOSX.sdk.tar.xz prevents filename/version drift; version is carried by URL and checksum.

Testing the macOS cross image
- To validate the cross image locally:
  - make build-macos-cross-rust-builder
  - make test-macos-cross-image
- What these tests cover:
  - Environment and toolchain presence in the image (including oa64-clang/o64-clang, stable tool aliases).
  - SDK installation sanity (MacOSX<ver>.sdk directory and SDK_NAME.txt).
  - C smoke link against CoreFoundation producing a Mach-O binary.
  - Rust hello-world build for aarch64-apple-darwin producing a Mach-O arm64 binary.

CI
- A dedicated job `test-macos-cross-image` can run these tests inside the cross image:
  - It uses the same image tags as the build (`:$CI_COMMIT_TAG` on tags; `:ci` on default-branch manual runs and schedules).
  - It runs only the macOS cross tests using nextest expression `test(/^e2e_macos_cross_/)`.
