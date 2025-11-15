# macOS cross-builder prerequisites (Phase 0)

This document captures the CI prerequisites required to build the aifo-coder macOS
launcher on Linux via osxcross as described in:
spec/aifo-coder-implement-macos-cross-builder-image-v3.spec (v3).

Goal
- The Apple SDK must NOT be committed to the repository or uploaded as an artifact.
- The SDK must be injected into CI securely via masked, protected variables.
- CI jobs using the SDK must be restricted to tags (and optionally default-branch manual runs).

Required CI variables (GitLab)
- APPLE_SDK_BASE64: masked + protected
  - Contents: base64 of the SDK tarball, e.g. MacOSX13.3.sdk.tar.xz
  - This variable is decoded by CI into ci/osx/${OSX_SDK_FILENAME} just-in-time.

Optional CI variables
- OSX_SDK_FILENAME (default: MacOSX13.3.sdk.tar.xz)
  - The filename used when placing the decoded SDK into the build context.

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
  - ci/osx/${OSX_SDK_FILENAME:-MacOSX13.3.sdk.tar.xz}

Notes
- Kaniko cannot use BuildKit RUN secret mounts; therefore we rely on a COPY of ci/osx/${OSX_SDK_FILENAME}
  in the macos-cross-rust-builder Dockerfile stage (Phase 1).
- The SDK file must exist in the build context before the Docker build starts.

Security reminders
- Never commit the SDK to the repository.
- Do not print the SDK contents in CI logs (only print file names/metadata like ls -lh).
- Do not upload the SDK as an artifact.
- Keep variables masked and protected, and scope jobs as described above.
