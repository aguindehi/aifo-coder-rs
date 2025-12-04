# Validation of the macOS cross build

This document describes how to validate the macOS cross build locally and in CI
according to spec/aifo-coder-implement-macos-cross-builder-image-v3.spec (v3).

Local validation (developer machine)
1) Ensure the Apple SDK tarball is placed at:
   ci/osx/${OSX_SDK_FILENAME}  (default: MacOSX13.3.sdk.tar.xz)

2) Build the cross image:
   make build-macos-cross-rust-builder

3) Build the macOS launcher inside the cross image:
   - Both architectures:
     make build-launcher-macos-cross
   - AMD64 only:
     make build-launcher-macos-cross-amd64
   - ARM64 only:
     make build-launcher-macos-cross-arm64

4) Validate the resulting binary with file(1):
   make validate-macos-artifact
   - This checks either:
     - dist/aifo-coder-macos-arm64 (if present), or
     - target/aarch64-apple-darwin/release/aifo-coder

   The validation passes if file(1) reports “Mach-O 64-bit arm64”.

CI validation (tag pipelines)
- build-macos-cross-rust-builder completes using Kaniko.
- build-launcher-macos:
  - Produces dist/aifo-coder-macos-arm64.
  - Runs file(1) and requires “Mach-O 64-bit arm64” in the output.
- publish-release:
  - Attaches both Linux and macOS artifacts to the release and exposes links.

Troubleshooting
- If validation fails with architecture mismatch:
  - Rebuild clean: rm -rf target; re-run the build steps.
  - Ensure MACOSX_DEPLOYMENT_TARGET=11.0 is set in the cross image.
  - Verify cargo and rustup targets inside the cross image:
    rustup target list | grep aarch64-apple-darwin
- If the SDK decode fails in CI:
  - Confirm APPLE_SDK_BASE64 and OSX_SDK_FILENAME are set (masked, protected).
  - Check ci/bin/decode-apple-sdk.sh logs (metadata only; contents are never printed).
