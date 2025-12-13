AIFO Coder: macOS Mach-O binary signing (local-only, Apple Dev ID ready) – v3, phased

Status
- This v3 spec refines v2 for a world without macOS CI runners.
- All signing and notarization steps are performed locally on a macOS developer machine.
- CI remains responsible only for producing unsigned Mach-O binaries (via osxcross) and packaging them.

Scope
- Provide local Makefile targets to:
  - Sign macOS Mach-O binaries (arm64 and x86_64) with:
    - A currently configured local certificate (self-signed or enterprise) and
    - Later, an Apple “Developer ID Application” certificate when available.
  - Optionally notarize and staple signed binaries/zips with notarytool when Apple credentials exist.
- Produce per-arch zip archives suitable for direct download (from GitLab release assets) and safe
  execution under Gatekeeper.
- Keep the existing DMG flow (release-app / release-dmg / release-dmg-sign) unchanged.

Non-goals
- No macOS signing or notarization in CI (no macOS runners available).
- No changes to existing CI Makefile integration beyond what is needed for local workflows.
- No behavioral changes to the Rust binary; this is purely packaging/signing.

-------------------------------------------------------------------------------
Phase 0 – Preconditions, artifacts, and terminology
-------------------------------------------------------------------------------

Artifacts
- Linux and CI builds (unchanged):
  - dist/aifo-coder-macos-arm64
  - dist/aifo-coder-macos-x86_64
  - These are unsigned, plain Mach-O binaries.

Local macOS developer actions (new):
- A developer on macOS:
  - Either builds the Mach-O binaries locally with the Makefile (release-for-mac) or
  - Copies the dist/ artifacts from a CI build (e.g., from GitLab CI artifacts) into the project dist/.
  - Runs local Makefile targets to sign, zip, and optionally notarize.

Terminology
- “Binary”: raw Mach-O executable (dist/aifo-coder-macos-<arch>).
- “Zip”: .zip archive containing one signed binary + README.md + NOTICE + LICENSE; notarization input.
- “DMG”: drag-and-drop macOS disk image built by release-dmg / release-dmg-sign.
- “Developer ID Application certificate”:
  - Apple certificate for distributing macOS apps outside the Mac App Store; required for notarization.
- “Local cert”: any keypair/cert installed in the developer’s keychain (self-signed or enterprise).

Key constraints
- codesign, notarytool, stapler are macOS-only; all signing/notarization is local.
- CI remains responsible only for building unsigned macOS binaries via osxcross.
- Apple will notarize only .zip/.dmg/.pkg; our per-arch zips are the primary notarization container.
- Gatekeeper:
  - Signed binaries with a recognized Apple identity + notarization (and stapling) give the best UX.
  - Self-signed certs are acceptable for internal/local testing but will cause Gatekeeper prompts.

-------------------------------------------------------------------------------
Phase 1 – Certificate strategy (local-only)
-------------------------------------------------------------------------------

1.1 Current certificate: local/self-signed
- Use-case:
  - Immediate need: sign binaries with an existing cert in the developer’s keychain (e.g. self-signed
    “Migros AI Foundation Code Signer”) without notarization.
- Behavior:
  - codesign with basic flags (no hardened runtime) is permitted for non-Apple certs.
  - Notarization is automatically skipped if the cert is not Apple-trusted.

Self-signed certificate creation (recap)
- In Keychain Access (login keychain):
  - Menu: Keychain Access → Certificate Assistant → Create a Certificate…
  - Name: e.g. “Migros AI Foundation Code Signer”.
  - Identity Type: Self Signed Root.
  - Certificate Type: Code Signing.
  - Key Size: 2048 or 4096.
  - Location: login.
- Verify:
  - Certificate appears in “login”.
  - A private key entry exists under it.
- Use this Common Name as SIGN_IDENTITY for local signing.

1.2 Future certificate: Developer ID Application (Apple Dev Program)
- For public distribution, we will later use a “Developer ID Application” certificate.
- Important:
  - Only the Account Holder can create Developer ID certificates.
- Create via Apple Developer portal:
  - Certificates → “+” → Developer ID Application.
  - Upload a CSR generated from Keychain Access (“Request a Certificate From a CA”).
  - Download and install the resulting .cer into login keychain.
- Retrieve identity:
  - security find-identity -p codesigning -v
  - Look for: “Developer ID Application: <Org> (<TEAMID>)”.
- When using this identity:
  - Makefile will choose hardened runtime flags and support notarization.
  - NOTARY_PROFILE may be configured for notarytool (again, local-only).

1.3 Local environment expectations
- macOS host with:
  - Xcode Command Line Tools (xcrun, codesign, notarytool, stapler, hdiutil).
  - Keychain with:
    - Current local cert (self-signed or enterprise) for immediate signing.
    - Optionally, Developer ID Application cert later.
  - Optional notarytool profile for notarization:
    - Pre-created with xcrun notarytool store-credentials --keychain-profile "<name>" ...

-------------------------------------------------------------------------------
Phase 2 – Makefile extensions (local signing and zipping)
-------------------------------------------------------------------------------

2.1 Variables (clarified semantics)
- Existing:
  - DIST_DIR ?= dist
  - BIN_NAME ?= aifo-coder
  - APP_NAME, APP_BUNDLE_ID, DMG_NAME, SIGN_IDENTITY, NOTARY_PROFILE, DMG_BG.
- Semantics for signing path:
  - SIGN_IDENTITY:
    - Common Name of the certificate in the login keychain, e.g.:
      - “Migros AI Foundation Code Signer” (self-signed; current behavior), or
      - “Developer ID Application: Migros AI Foundation (TEAMID)” (future).
  - NOTARY_PROFILE:
    - Optional keychain profile for notarytool.
    - When empty, notarization steps are skipped gracefully without error.

2.2 New local-only Makefile targets
- Add Darwin-guarded targets:
  - sign-macos-binaries
  - zip-macos-binaries
  - notarize-macos-binary-zips
  - release-macos-binary-signed (aggregate)
- These are intended for local developers; they are never invoked in CI.

2.2.1 sign-macos-binaries (local, Darwin-only)
Purpose:
- Sign dist/aifo-coder-macos-arm64 and dist/aifo-coder-macos-x86_64 in-place using SIGN_IDENTITY.

Behavior:
- Guard:
  - If uname -s != Darwin, print a clear message and exit 1.
- Input checks:
  - Require:
    - dist/aifo-coder-macos-arm64
    - dist/aifo-coder-macos-x86_64
  - If any missing: error with a hint (build or copy them first).
- Extended attributes:
  - xattr -cr dist/aifo-coder-macos-arm64 || true
  - xattr -cr dist/aifo-coder-macos-x86_64 || true

Signing mode selection:
- Determine whether SIGN_IDENTITY is an Apple Developer identity:
  - Use security find-certificate -a -c "$SIGN_IDENTITY" -Z -p to get the certificate.
  - Optionally pipe to openssl x509 -noout -subject.
  - If subject contains “Developer ID Application” or “Apple Distribution” or “Apple Development”:
    - Treat as Apple/trustable identity; set APPLE_DEV=1.
  - Else:
    - Non-Apple or self-signed; APPLE_DEV=0.

Signing flags:
- If APPLE_DEV=1:
  - SIGN_FLAGS="--force --timestamp --options runtime --verbose=4"
- If APPLE_DEV=0:
  - SIGN_FLAGS="--force --verbose=4"
- Keychain:
  - Default: KEYCHAIN="$(security default-keychain -d user | tr -d ' \"')"
  - Use this in codesign via --keychain "$KEYCHAIN" where possible.

Signing strategy per binary:
- For each binary B:
  1) Attempt:
     - codesign $SIGN_FLAGS --keychain "$KEYCHAIN" -s "$SIGN_IDENTITY" "$B"
  2) If that fails:
     - Extract SHA-1 hash of the certificate:
       - SIG_SHA1="$(security find-certificate -a -c "$SIGN_IDENTITY" -Z 2>/dev/null | awk '/^SHA-1 hash:/{print $3; exit}')"
     - If non-empty, try:
       - codesign $SIGN_FLAGS --keychain "$KEYCHAIN" -s "$SIG_SHA1" "$B"
  3) If still failing:
     - As a last local-fallback for non-Apple identities only (APPLE_DEV=0):
       - Print warning that identity-based signing failed and fallback to ad-hoc:
         - codesign $SIGN_FLAGS -s - "$B"
     - If APPLE_DEV=1 (Developer ID), do NOT fallback to ad-hoc; error out instead:
       - We want deterministic failure when Apple identity cannot be used.
- Verification:
  - For each signed binary:
    - codesign --verify --strict --verbose=4 "$B"
    - codesign -dv --verbose=4 "$B" (log informational).
    - spctl --assess --type exec --verbose=4 "$B" || true
  - For APPLE_DEV=1, if spctl fails, warn but do not auto-fix; developer must investigate.

Outcome:
- For self-signed/local cert:
  - Binaries are now signed (ad-hoc fallback allowed).
- For Developer ID:
  - Binaries are properly signed with hardened runtime, ready for notarization.
  - If identity is unusable, the target fails (no ad-hoc fallback).

2.2.2 zip-macos-binaries (local, platform-independent)
Purpose:
- Create per-arch zip archives containing signed binaries and documentation.

Inputs:
- dist/aifo-coder-macos-arm64
- dist/aifo-coder-macos-x86_64
- README.md, NOTICE, LICENSE from repo root.

Outputs:
- dist/aifo-coder-macos-arm64.zip
- dist/aifo-coder-macos-x86_64.zip

Behavior:
- This target can run on any platform (but is intended mainly on macOS after signing).
- Steps (per arch):
  - Ensure dist/ exists.
  - Use staging dirs:
    - dist/.zip-stage-arm64
    - dist/.zip-stage-x86_64
  - For arm64:
    - mkdir -p dist/.zip-stage-arm64
    - cp dist/aifo-coder-macos-arm64 dist/.zip-stage-arm64/aifo-coder-macos-arm64
    - cp README.md NOTICE LICENSE into stage dir (best-effort; error if missing, since these are required).
    - (cd dist/.zip-stage-arm64 && zip -9r ../aifo-coder-macos-arm64.zip .)
    - rm -rf dist/.zip-stage-arm64
  - Repeat analogously for x86_64.
- Preconditions:
  - Do not rebuild binaries; simply error if they are missing.
- No signing occurs here; this target relies on sign-macos-binaries having run first.

2.2.3 notarize-macos-binary-zips (local, Darwin-only, optional)
Purpose:
- Notarize the per-arch macOS binary zips and staple tickets.

Inputs:
- dist/aifo-coder-macos-arm64.zip
- dist/aifo-coder-macos-x86_64.zip
- SIGN_IDENTITY (ideally Developer ID Application)
- NOTARY_PROFILE (optional, but required for actual notarization).

Behavior:
- Guard:
  - If uname -s != Darwin: print message, exit 1 or (optionally) silently no-op; spec recommends exit 1 so misuse is clear.
- Pre-check:
  - If NOTARY_PROFILE is empty:
    - Print clear message: “NOTARY_PROFILE unset; skipping notarization/stapling (non-fatal).”
    - Exit 0.
  - If xcrun notarytool is unavailable:
    - Print message: “notarytool not found; skipping notarization/stapling.”
    - Exit 0.
- Zip existence:
  - Require both zip files.
  - If missing, error and suggest running zip-macos-binaries.

Notarization flow:
- Submit and wait for each zip:
  - xcrun notarytool submit dist/aifo-coder-macos-arm64.zip \
      --keychain-profile "$NOTARY_PROFILE" --wait
  - xcrun notarytool submit dist/aifo-coder-macos-x86_64.zip \
      --keychain-profile "$NOTARY_PROFILE" --wait
- On any failure:
  - Print notarytool output.
  - Exit with non-zero status (so developer sees the failure).
- Stapling:
  - Always attempt:
    - xcrun stapler staple dist/aifo-coder-macos-arm64.zip || true
    - xcrun stapler staple dist/aifo-coder-macos-x86_64.zip || true
  - Best-effort on raw binaries:
    - xcrun stapler staple dist/aifo-coder-macos-arm64 || true
    - xcrun stapler staple dist/aifo-coder-macos-x86_64 || true
- Validation:
  - xcrun stapler validate dist/aifo-coder-macos-arm64.zip || true
  - xcrun stapler validate dist/aifo-coder-macos-x86_64.zip || true

Outcome:
- With Developer ID + NOTARY_PROFILE:
  - Per-arch zips are notarized and stapled.
  - Raw binaries may or may not accept staples, but will still be recognized.
- With self-signed/local cert or missing NOTARY_PROFILE:
  - Target becomes a no-op or prints explicit “skipped” messages.
  - Binaries remain signed but not notarized.

2.2.4 release-macos-binary-signed (local aggregate)
Purpose:
- One-step local pipeline to produce signed (and optionally notarized) macOS binary zips.

Behavior:
- Darwin-only; on non-macOS, exit 1 with explanation.
- Precondition:
  - dist/aifo-coder-macos-arm64 and dist/aifo-coder-macos-x86_64 must exist.
- Sequence:
  - make sign-macos-binaries
  - make zip-macos-binaries
  - make notarize-macos-binary-zips
- Notes:
  - With only a self-signed cert and no NOTARY_PROFILE:
    - Binaries are signed.
    - Zips are created.
    - notarize-macos-binary-zips is effectively a no-op with a clear message.
  - With Developer ID + NOTARY_PROFILE:
    - Full signing + notarization pipeline runs.

-------------------------------------------------------------------------------
Phase 3 – Local developer workflows (no CI)
-------------------------------------------------------------------------------

3.1 Local workflow with current (self-signed) certificate
Goal:
- Immediately produce signed macOS binaries for internal usage (not notarized).

Steps (macOS developer):
1) Build macOS binaries:
   - Option A: from host:
     - make release-for-mac
       - Produces dist/* tar.gz and target/aarch64-apple-darwin/release/aifo-coder; developer can then copy to dist/aifo-coder-macos-arm64 etc.
   - Option B: from CI artifacts:
     - Download dist/aifo-coder-macos-arm64 and dist/aifo-coder-macos-x86_64 from GitLab release.
     - Place them into ./dist on the mac host.

2) Sign binaries with the local/self-signed cert:
   - Ensure SIGN_IDENTITY matches the CN in login keychain, e.g.:
     - export SIGN_IDENTITY="Migros AI Foundation Code Signer"
   - Run:
     - make release-macos-binary-signed
   - Since NOTARY_PROFILE is typically empty, notarize-macos-binary-zips will print a skip message.

3) Verify:
   - codesign --verify --deep --strict --verbose=4 dist/aifo-coder-macos-arm64
   - codesign --verify --deep --strict --verbose=4 dist/aifo-coder-macos-x86_64
   - spctl --assess --type exec --verbose=4 dist/aifo-coder-macos-arm64 || true
   - spctl --assess --type exec --verbose=4 dist/aifo-coder-macos-x86_64 || true

Result:
- Signed, non-notarized binaries and zips suitable for internal distribution.
- External users may see Gatekeeper warnings unless they trust the self-signed cert.

3.2 Local workflow with Developer ID Application + notarization
Goal:
- Produce signed and notarized binaries/zips for public distribution.

Pre-setup:
- Ensure Developer ID Application cert is installed in login keychain.
- Create a notarytool profile once:
  - xcrun notarytool store-credentials --keychain-profile "AifoNotary" \
      --team-id "<TEAMID>" \
      --key-id "<KEY_ID>" \
      --issuer "<ISSUER_ID>" \
      --private-key /path/to/AuthKey_XXXXXX.p8

Steps:
1) Build or copy macOS binaries into dist/ as above.
2) Export environment variables:
   - export SIGN_IDENTITY="Developer ID Application: <Org Name> (<TEAMID>)"
   - export NOTARY_PROFILE="AifoNotary"
3) Run:
   - make release-macos-binary-signed
4) Wait for:
   - sign-macos-binaries (with hardened runtime)
   - zip-macos-binaries
   - notarize-macos-binary-zips (submits, waits, staples, validates)

Result:
- dist/aifo-coder-macos-arm64 (signed, notarized)
- dist/aifo-coder-macos-x86_64 (signed, notarized)
- dist/aifo-coder-macos-arm64.zip (stapled)
- dist/aifo-coder-macos-x86_64.zip (stapled)

3.3 Interaction with DMG signing (release-dmg-sign)
- DMG flow remains as implemented:
  - release-app
  - release-dmg
  - release-dmg-sign
- Recommended patterns:
  - For GUI users:
    - make release-dmg-sign SIGN_IDENTITY="Developer ID Application: ..." NOTARY_PROFILE="AifoNotary"
    - DMG is signed and notarized as before.
  - For CLI users:
    - make release-macos-binary-signed (as above).
- It is permissible to:
  - Reuse the same SIGN_IDENTITY and NOTARY_PROFILE for both DMG and binary zips.

-------------------------------------------------------------------------------
Phase 4 – Release packaging and documentation updates (local + CI)
-------------------------------------------------------------------------------

4.1 How CI and local workflows fit together
- CI:
  - Continues to:
    - Build Linux binary and tarball.
    - Build macOS arm64/x86_64 binaries via osxcross.
    - Package them into aifo-coder-macos.tar.gz and publish as unsigned artifacts.
  - No codesign, no notarytool.

- Local macOS developer:
  - Pulls CI artifacts (Linux and macOS).
  - Runs local signing targets to produce signed/notarized macOS zips.
  - Optionally attaches these zips to GitLab releases manually or via a separate job that simply
    uploads existing signed files (no signing in that job).

4.2 Release asset recommendations
- Linux:
  - aifo-coder-linux-x86_64.tar.gz
  - aifo-coder-linux-x86_64
- macOS:
  - aifo-coder-macos-arm64.zip (signed; notarized if Developer ID + NOTARY_PROFILE used)
  - aifo-coder-macos-x86_64.zip (same)
  - aifo-coder-macos-arm64 (signed, primarily for advanced users; may rely on online notarization lookup)
  - aifo-coder-macos-x86_64 (same)
  - aifo-coder-macos.tar.gz (legacy, unsigned; document that it is not notarized)
  - DMG from release-dmg-sign (signed + notarized; recommended for GUI users).

4.3 Documentation updates (follow-ups)
- README / INSTALL docs:
  - Document that CI macOS artifacts are unsigned and that official macOS binaries are signed and
    notarized by running local Makefile targets with a Developer ID cert.
  - Clarify that:
    - For direct CLI use: prefer the signed/notarized per-arch zips.
    - For GUI drag-and-drop: prefer the DMG from release-dmg-sign.

-------------------------------------------------------------------------------
Phase 5 – Consistency checks and corner cases (local-only)
-------------------------------------------------------------------------------

5.1 Using a non-Apple (self-signed) cert
- Acceptable for:
  - Internal/local distribution and testing.
- Behavior:
  - sign-macos-binaries:
    - Uses basic codesign flags (--force --verbose=4) without hardened runtime.
    - On identity failure, may fallback to ad-hoc (-s -) with warnings.
  - notarize-macos-binary-zips:
    - Skips notarization (NOTARY_PROFILE likely unset).
- Gatekeeper:
  - External users will see Gatekeeper prompts; they may need to trust the cert or override security.

5.2 Developer ID preconditions
- For notarization, Apple requires:
  - Developer ID Application cert.
  - Hardened runtime (--options runtime --timestamp).
  - No ad-hoc signatures.
  - Recognized certificate (self-signed certs will be rejected by notarytool).
- The spec enforces:
  - When APPLE_DEV=1, sign-macos-binaries will not fallback to ad-hoc; it will fail if the identity
    cannot be used.
  - notarize-macos-binary-zips is only effective with NOTARY_PROFILE set and notarytool available.

5.3 Stapling to raw binaries vs zips
- Apple’s preferred stapling targets:
  - Zips, DMGs, app bundles.
- Raw Mach-O binaries:
  - May or may not accept stapling; failures are treated as non-fatal.
- Spec mandates:
  - Always staple zips after successful notarization.
  - Attempt to staple binaries but ignore failures (log warnings).
  - Recommend end-users prefer the zipped or DMG artifacts for best offline behavior.

5.4 Architecture strategy
- This spec continues with per-arch binaries and zips:
  - dist/aifo-coder-macos-arm64[.zip]
  - dist/aifo-coder-macos-x86_64[.zip]
- Optional future extension:
  - Build a universal binary via lipo and sign/notarize that single binary.
- If universal is added later:
  - Make Makefile targets clearly selectable between per-arch and universal.

-------------------------------------------------------------------------------
Phase 6 – Developer checklist and troubleshooting
-------------------------------------------------------------------------------

6.1 Developer pre-flight checklist (macOS)
- Confirm:
  - xcrun, codesign, notarytool, stapler are installed (Xcode CLT).
  - login keychain unlocked.
  - SIGN_IDENTITY matches an actual certificate CN in login keychain:
    - security find-certificate -a -c "$SIGN_IDENTITY" -Z
  - If using Developer ID, verify the certificate subject and TEAMID.

6.2 Common issues
- “code object is not signed at all”:
  - Ensure sign-macos-binaries ran successfully and codesign verify passes.
- “notarytool: error ... invalid signature”:
  - Likely using a self-signed cert or ad-hoc signature; notarization only works with Developer ID.
- “resource fork, Finder info, or similar detritus not allowed”:
  - Make sure xattr -cr has been applied before signing and zipping.
- “Operation not permitted” during codesign:
  - Keychain locked or access to private key denied; unlock keychain and allow codesign to use it.

6.3 Local regression testing
- After implementing Makefile targets:
  - On macOS:
    - make build-launcher
    - Build dist/aifo-coder-macos-arm64/x86_64 (via release-for-mac or copying).
    - make sign-macos-binaries
    - make zip-macos-binaries
    - If Developer ID + NOTARY_PROFILE ready: make notarize-macos-binary-zips
  - Verify signatures and stapling with codesign/spctl/notarytool/stapler.
  - Ensure Makefile behavior is a no-op on non-Darwin hosts with clear messages.

Outcome
- Local developers can sign macOS binaries now with existing certificates and later with a
  Developer ID Application cert, without changing CI.
- Signed (and optionally notarized) macOS binaries and zips are produced entirely on macOS, ready to
  be uploaded to GitLab releases as official artifacts.
````
