AIFO Coder: macOS Mach-O binary signing with Apple Developer ID Application (v2, phased)

Status
- This v2 spec refines and extends v1 after validating it against the current Makefile, .gitlab-ci.yml,
  and release process.
- Scope is limited to signing/notarizing standalone macOS binaries (Mach-O) and producing installable
  zips that integrate cleanly with the existing DMG flow and GitLab release job.

High‑level goals
- Ensure macOS aifo-coder binaries (arm64 and x86_64) are:
  - Signed with a “Developer ID Application” certificate suitable for Gatekeeper.
  - Submitted for notarization in an acceptable container (.zip) and stapled where possible.
- Produce per‑arch zips for direct download from GitLab releases (in addition to the existing DMG).
- Keep the osxcross Linux CI build unchanged; signing + notarization runs only on macOS.
- Integrate with:
  - Makefile: new Darwin‑only targets for signing/notarization and zip creation.
  - .gitlab-ci.yml: new macOS job consuming dist/ binaries and producing signed/notarized zips.
  - publish-release: updated to prefer signed zips/binaries when available, but still work when
    signing is disabled (missing runner or secrets).

Non‑goals
- Do not change the existing DMG signing/notarization behavior (release-dmg-sign).
- Do not change build outputs for non‑macOS targets.
- Do not introduce new runtime behavior in the Rust binary; this is a packaging/signing concern.

-------------------------------------------------------------------------------
Phase 0 – Preconditions and terminology
-------------------------------------------------------------------------------

Artifacts and paths (existing)
- Linux CI builds macOS binaries using osxcross:
  - dist/aifo-coder-macos-arm64
  - dist/aifo-coder-macos-x86_64
- publish-release currently:
  - Copies these into the job root.
  - Renames the Linux binary to aifo-coder-linux-x86_64.
  - Builds:
    - aifo-coder-linux-x86_64.tar.gz
    - aifo-coder-macos.tar.gz (contains both macOS binaries + README/NOTICE/LICENSE).
- The macOS DMG flow (release-app / release-dmg / release-dmg-sign) is host‑local on Darwin only.

Terminology
- “Binary”: the raw Mach-O executable file (not in any container).
- “Zip”: a .zip archive containing one signed binary + README.md + NOTICE + LICENSE.
- “DMG”: the signed/notarized drag‑and‑drop image; already handled by release-dmg-sign.
- “Developer ID Application certificate”: Apple certificate used to sign apps distributed outside
  the Mac App Store, required for notarization.
- “Notary profile”: notarytool keychain profile name used for submission.

Key constraints
- Signing + notarization MUST run on macOS (xcrun/codesign/notarytool/stapler).
- Linux/osxcross builds remain unchanged; they just produce unsigned Mach-O binaries.
- Apple only notarizes .zip/.dmg/.pkg; .tar.gz cannot be sent to notarytool.

-------------------------------------------------------------------------------
Phase 1 – Apple certificate and notary credentials
-------------------------------------------------------------------------------

1.1 Developer ID Application certificate (must use this certificate type)
- In Apple Developer portal (Certificates, Identifiers & Profiles):
  - Choose “Developer ID Application”.
  - This is the correct cert for macOS apps and binaries distributed outside the Mac App Store.
  - “Mac App Distribution” and “Apple Distribution” are for App Store/Ad Hoc, not for standalone
    DMGs/zips.
  - “Developer ID Installer” is needed only if we later introduce a .pkg installer.

1.2 CSR creation and certificate issuance
- On a Mac (dev or CI runner):
  - Open Keychain Access → Certificate Assistant → Request a Certificate From a Certificate Authority.
  - Choose:
    - Key size: 2048‑bit RSA.
    - Common Name: descriptive (e.g., “Migros AI Foundation aifo-coder”).
    - Save to disk; do not email the CSR.
- Account Holder flow:
  - Log into Apple Developer → Certificates → “+”.
  - Select “Developer ID Application”.
  - Upload CSR; download the resulting .cer file.

1.3 Install certificate to login keychain
- Double‑click the .cer to add it to the login keychain.
- Verify that:
  - The certificate appears in the “login” keychain.
  - It has a paired private key (expand to see the private key entry).
- Retrieve the identity string to use as SIGN_IDENTITY:
  - Run: security find-identity -p codesigning -v
  - Identify the line for the Developer ID Application cert; note the Common Name:
    “Developer ID Application: <Org Name> (<TEAMID>)”.

1.4 Optional CI import via .p12
- If the CI macOS runner does not persist the cert/key:
  - Export a password‑protected .p12 from Keychain Access:
    - Right‑click the Developer ID Application certificate → Export.
    - Choose .p12; set a strong password.
  - Base64‑encode the .p12 and store in GitLab CI as protected variables:
    - P12_BASE64: base64 of the .p12.
    - P12_PASSWORD: password for the .p12.
    - KEYCHAIN_PASSWORD: password for a temporary keychain used in CI.

1.5 Notary credentials and profile
- Create an App Store Connect API key:
  - In App Store Connect → Users and Access → Keys → “+”.
  - Select appropriate access; note:
    - Issuer ID.
    - Key ID.
    - Download the .p8 file.
- On the macOS runner, choose one of two patterns:

A) Runner‑level profile (preferred for static runners)
- Store AuthKey_XXXXXX.p8 somewhere secure on the runner.
- Once, run:
  - xcrun notarytool store-credentials --keychain-profile "AifoNotary" \
      --team-id "$APPLE_TEAM_ID" \
      --key-id "$NOTARY_KEY_ID" \
      --issuer "$NOTARY_ISSUER_ID" \
      --private-key /path/to/AuthKey_XXXXXX.p8
- Use NOTARY_PROFILE=AifoNotary in Makefile/CI.

B) Job‑level profile (preferred for ephemeral runners)
- In GitLab protected variables:
  - NOTARY_PRIVATE_KEY_BASE64: base64 of the .p8.
  - NOTARY_KEY_ID, NOTARY_ISSUER_ID, APPLE_TEAM_ID.
- In CI job:
  - Decode NOTARY_PRIVATE_KEY_BASE64 to AuthKey.p8.
  - Run notarytool store-credentials with --keychain-profile "$NOTARY_PROFILE" and those IDs.

-------------------------------------------------------------------------------
Phase 2 – Makefile extensions (Darwin‑only signing for Mach-O binaries)
-------------------------------------------------------------------------------

2.1 Variables (augmentation, no breaking behavior)
- Existing variables:
  - DIST_DIR ?= dist
  - BIN_NAME ?= aifo-coder
  - APP_NAME, APP_BUNDLE_ID, DMG_NAME, SIGN_IDENTITY, NOTARY_PROFILE, DMG_BG.
- Extend semantics for SIGN_IDENTITY and NOTARY_PROFILE:
  - SIGN_IDENTITY:
    - Intended to be either:
      - The Developer ID Application Common Name (e.g., “Developer ID Application: Migros AG (TEAMID)”).
      - Or a local self‑signed identity name for non‑notarized internal builds.
  - NOTARY_PROFILE:
    - When non‑empty and used with an Apple Developer ID identity, the Make targets will attempt
      notarization using notarytool.

2.2 New Darwin‑only targets for Mach-O binaries
- Add the following phony targets, guarded under the same Darwin conditional used for release-dmg.*:
  - sign-macos-binaries
  - zip-macos-binaries
  - notarize-macos-binary-zips
  - release-macos-binary-signed (aggregate)

2.2.1 sign-macos-binaries
- Purpose:
  - Sign dist/aifo-coder-macos-arm64 and dist/aifo-coder-macos-x86_64 in place using SIGN_IDENTITY.
- Behavior:
  - Darwin‑only; on non‑Darwin hosts, print an error and exit 1.
  - Check for existence:
    - dist/aifo-coder-macos-arm64
    - dist/aifo-coder-macos-x86_64
    - If either missing, fail with a clear message.
  - Clear extended attributes:
    - xattr -cr dist/aifo-coder-macos-arm64 || true
    - xattr -cr dist/aifo-coder-macos-x86_64 || true
  - Determine signing flags:
    - For Apple Developer ID Application identities:
      - Use: codesign --force --timestamp --options runtime --verbose=4
    - For self‑signed or unknown identities:
      - Use: codesign --force --verbose=4
    - Heuristic:
      - Use security find-certificate -a -c "$SIGN_IDENTITY" -Z to fetch certificate and inspect subject;
      - If subject contains “Developer ID Application” (or “Apple Distribution”), treat as Apple identity and enable hardened runtime.
    - Always prefer using the default user keychain (security default-keychain -d user).
  - Signing process:
    - For each binary:
      - Try codesign with SIGN_IDENTITY name and default keychain.
      - If that fails and certificate SHA1 was discovered, try codesign with SHA1 selector.
      - On repeated failure:
        - Do NOT silently fallback to ad‑hoc for this target; return an error because binaries
          must be signed with a real identity for notarization. Ad‑hoc fallback is acceptable only
          in the DMG helper for purely internal, non‑notarized flows.
  - Verification:
    - For each binary:
      - codesign --verify --strict --verbose=4 <binary>
      - codesign -dv --verbose=4 <binary> (log to stdout for diagnostics).
      - spctl --assess --type exec --verbose=4 <binary> || true (best‑effort; non‑zero is warning).

2.2.2 zip-macos-binaries
- Purpose:
  - Create per‑arch zip archives containing the signed binaries and documentation.
- Inputs:
  - dist/aifo-coder-macos-arm64 (expected already signed by sign-macos-binaries).
  - dist/aifo-coder-macos-x86_64 (expected already signed by sign-macos-binaries).
  - README.md, NOTICE, LICENSE (from repo root).
- Outputs:
  - dist/aifo-coder-macos-arm64.zip
  - dist/aifo-coder-macos-x86_64.zip
- Behavior:
  - On Darwin and non‑Darwin, this target can operate (pure zip creation).
  - Steps:
    - Ensure dist/ exists.
    - Use a temporary staging dir per arch:
      - dist/.zip-stage-arm64
      - dist/.zip-stage-x86_64
    - Install:
      - Binary renamed to a stable name (e.g., aifo-coder-macos-arm64 inside the zip).
      - README.md, NOTICE, LICENSE.
    - Zip:
      - (cd stage && zip -9r ../aifo-coder-macos-arm64.zip .)
      - (cd stage && zip -9r ../aifo-coder-macos-x86_64.zip .)
    - Remove the staging directories after success.
  - Preconditions:
    - Require that binaries exist; do not attempt to infer or rebuild.

2.2.3 notarize-macos-binary-zips
- Purpose:
  - Submit the .zip archives for notarization and staple tickets to the zips and (best‑effort) raw binaries.
- Inputs:
  - dist/aifo-coder-macos-arm64.zip
  - dist/aifo-coder-macos-x86_64.zip
  - SIGN_IDENTITY with a Developer ID Application certificate.
  - NOTARY_PROFILE with a ready notarytool profile.
- Behavior (Darwin only):
  - If NOTARY_PROFILE is empty or xcrun notarytool is unavailable:
    - Print a clear message and skip notarization/stapling; exit 0 (non‑fatal).
  - If either zip is missing:
    - Fail fast; instruct caller to run zip-macos-binaries.
  - Submit:
    - xcrun notarytool submit dist/aifo-coder-macos-arm64.zip \
        --keychain-profile "$NOTARY_PROFILE" --wait
    - Same for dist/aifo-coder-macos-x86_64.zip.
  - Staple (best‑effort):
    - xcrun stapler staple dist/aifo-coder-macos-arm64.zip || true
    - xcrun stapler staple dist/aifo-coder-macos-x86_64.zip || true
    - Attempt to staple raw binaries (Apple may reject; treat as best‑effort):
      - xcrun stapler staple dist/aifo-coder-macos-arm64 || true
      - xcrun stapler staple dist/aifo-coder-macos-x86_64 || true
  - Validate:
    - xcrun stapler validate dist/aifo-coder-macos-arm64.zip || true
    - xcrun stapler validate dist/aifo-coder-macos-x86_64.zip || true
  - If notarytool returns a failure:
    - Fail the Make target with non‑zero status, printing notarytool’s output.

2.2.4 release-macos-binary-signed (aggregate)
- Purpose:
  - Single Make entrypoint on a macOS host that takes already built binaries and produces signed,
    notarized zips (if credentials are available).
- Dependencies and sequence:
  - Precondition: dist/aifo-coder-macos-arm64 and dist/aifo-coder-macos-x86_64 exist (e.g., copied
    from Linux CI artifacts or built locally).
  - Steps:
    - make sign-macos-binaries
    - make zip-macos-binaries
    - make notarize-macos-binary-zips
- Behavior:
  - On non‑Darwin hosts: print an explanatory error and exit 1 (signing cannot be done).

-------------------------------------------------------------------------------
Phase 3 – GitLab CI integration (macOS signing job)
-------------------------------------------------------------------------------

3.1 New job: sign-macos-binaries (macOS runner)
- Stage: release or build (choose release for clearer dependency on build artifacts).
- Runner:
  - macOS runner with:
    - Xcode Command Line Tools installed (xcrun/codesign/notarytool/stapler).
    - Either:
      - Pre‑installed Developer ID Application certificate in login keychain, or
      - Ability to import .p12 via CI variables.
- Needs:
  - build-launcher-macos:
    - Provides dist/aifo-coder-macos-arm64.
  - build-launcher-macos-x86_64:
    - Provides dist/aifo-coder-macos-x86_64.
- Variables (protected, masked):
  - SIGN_IDENTITY (required).
  - NOTARY_PROFILE (optional; if empty, notarization is skipped gracefully).
  - Optional certificate import:
    - P12_BASE64, P12_PASSWORD, KEYCHAIN_PASSWORD.
  - Optional notary profile bootstrap (for ephemeral runners):
    - NOTARY_KEY_ID, NOTARY_ISSUER_ID, APPLE_TEAM_ID, NOTARY_PRIVATE_KEY_BASE64.

3.1.1 Script outline
- Step 1: Validate environment
  - Ensure xcrun, codesign, notarytool, stapler are present; log versions.
  - If codesign is missing, fail the job.

- Step 2 (optional): Import Developer ID cert into a temporary keychain
  - Only if P12_BASE64 is provided; otherwise rely on a pre‑configured login keychain.
  - Commands:
    - security create-keychain -p "$KEYCHAIN_PASSWORD" build.keychain
    - security default-keychain -s build.keychain
    - security unlock-keychain -p "$KEYCHAIN_PASSWORD" build.keychain
    - printf '%s' "$P12_BASE64" | base64 -d > dev_id.p12
    - security import dev_id.p12 -k build.keychain -P "$P12_PASSWORD" \
        -T /usr/bin/codesign -T /usr/bin/security
    - security set-key-partition-list -S apple-tool:,apple: -s \
        -k "$KEYCHAIN_PASSWORD" build.keychain
    - security find-identity -p codesigning -v || true

- Step 3 (optional): Create notarytool profile at runtime
  - Only if NOTARY_PROFILE is non‑empty and NOTARY_PRIVATE_KEY_BASE64 is present.
  - Commands:
    - printf '%s' "$NOTARY_PRIVATE_KEY_BASE64" | base64 -d > AuthKey.p8
    - xcrun notarytool store-credentials --keychain-profile "$NOTARY_PROFILE" \
        --team-id "$APPLE_TEAM_ID" \
        --key-id "$NOTARY_KEY_ID" \
        --issuer "$NOTARY_ISSUER_ID" \
        --private-key ./AuthKey.p8

- Step 4: Run Make targets
  - Invoke:
    - make release-macos-binary-signed SIGN_IDENTITY="$SIGN_IDENTITY" NOTARY_PROFILE="$NOTARY_PROFILE"

- Step 5: Post‑verification and artifacts
  - Explicitly verify:
    - codesign --verify --deep --strict --verbose=4 dist/aifo-coder-macos-arm64
    - codesign --verify --deep --strict --verbose=4 dist/aifo-coder-macos-x86_64
    - spctl --assess --type exec --verbose=4 dist/aifo-coder-macos-arm64 || true
    - spctl --assess --type exec --verbose=4 dist/aifo-coder-macos-x86_64 || true
  - Artifacts:
    - dist/aifo-coder-macos-arm64 (signed)
    - dist/aifo-coder-macos-x86_64 (signed)
    - dist/aifo-coder-macos-arm64.zip (signed, notarized if NOTARY_PROFILE configured)
    - dist/aifo-coder-macos-x86_64.zip (signed, notarized if NOTARY_PROFILE configured)

3.1.2 Rules and fallback behavior
- Rules:
  - Run on tags (release pipelines).
  - Optional: allow manual/mr pipelines with protected variable flag (e.g., SIGN_IDENTITY set).
- Fallback when CI infra is not ready:
  - If no macOS runner or secrets are configured:
    - The job can be skipped; publish-release will still expose the unsigned macOS binaries and
      a macOS tarball, but releases for macOS will require Gatekeeper overrides.
  - Once infra is ready:
    - Mark sign-macos-binaries as required for release tags.

-------------------------------------------------------------------------------
Phase 4 – publish-release integration
-------------------------------------------------------------------------------

4.1 Current behavior (for context)
- publish-release:
  - Needs:
    - build-launcher (Linux).
    - build-launcher-macos.
    - build-launcher-macos-x86_64.
  - Builds:
    - aifo-coder-linux-x86_64.tar.gz (Linux).
    - aifo-coder-macos.tar.gz (packed macOS binaries + docs).
  - Exposes assets:
    - aifo-coder-linux-x86_64.tar.gz
    - aifo-coder-macos.tar.gz
    - aifo-coder-linux-x86_64
    - aifo-coder-macos-arm64
    - aifo-coder-macos-x86_64

4.2 Target v2 behavior (preferred artifacts if signing job ran)
- Additional needs:
  - sign-macos-binaries (artifacts: true).
- Use a conditional to prefer signed/notarized zip artifacts when present:
  - If dist/aifo-coder-macos-arm64.zip and dist/aifo-coder-macos-x86_64.zip are present in artifacts:
    - Copy them into publish-release workspace root.
    - Add them as release assets.
  - Keep aifo-coder-macos.tar.gz for compatibility but document that zips are signed/notarized
    whereas the tar.gz may not be.

4.3 Release assets table (after v2)
- Linux:
  - aifo-coder-linux-x86_64.tar.gz
  - aifo-coder-linux-x86_64
- macOS:
  - aifo-coder-macos-arm64.zip  (signed; notarized if infra configured)
  - aifo-coder-macos-x86_64.zip (signed; notarized if infra configured)
  - aifo-coder-macos-arm64      (signed binary, primarily for advanced users; may rely on online
                                 notarization lookup if stapling to raw binary failed)
  - aifo-coder-macos-x86_64     (signed binary, same caveats)
  - aifo-coder-macos.tar.gz     (legacy tarball; may or may not be notarized; retain for backward
                                 compatibility but not the primary distributed artifact)

-------------------------------------------------------------------------------
Phase 5 – Consistency checks and corner cases
-------------------------------------------------------------------------------

5.1 Using a non‑Apple (self‑signed) cert
- The DMG signing path (release-dmg-sign) can use a self‑signed certificate for internal distribution.
- For Mach-O binary zips in this spec:
  - sign-macos-binaries will fail if codesign cannot use SIGN_IDENTITY (and will NOT fallback to ad‑hoc),
    because notarization is not possible without a Developer ID Application identity.
  - If SIGN_IDENTITY references a self‑signed cert:
    - sign-macos-binaries still succeeds (binaries become signed), but notarize-macos-binary-zips will:
      - Detect that NOTARY_PROFILE is unset or that notarytool fails.
      - Skip notarization and stapling, printing a warning.
    - This is acceptable for internal, non‑public distributions; Gatekeeper dialogs are expected.

5.2 Notarization preconditions
- Binaries must be:
  - Signed with an Apple Developer ID Application certificate.
  - Built with hardened runtime options (“--options runtime --timestamp”) for modern compliance.
- Notarytool will reject:
  - Unsigned or ad‑hoc unsigned binaries inside the zip.
  - Binaries signed with certificates not recognized by Apple (including self‑signed certs).
- The spec mandates:
  - If developer wants notarization, they must configure SIGN_IDENTITY with a Developer ID Application
    identity and provide NOTARY_PROFILE.
  - The Makefile will not silently degrade to ad‑hoc for this path.

5.3 Stapling behavior
- Apple supports stapling tickets primarily to:
  - Bundles (app bundles).
  - DMGs.
  - Zips.
- Raw Mach-O binaries may not always accept a stapled ticket.
- The spec mandates:
  - Always stapling the zips, if notarytool submission succeeded.
  - Attempting to staple the raw binaries, but treating failures as non‑fatal.
  - For best offline Gatekeeper behavior, macOS users should prefer:
    - The signed/notarized DMG or
    - The signed/notarized zips.

5.4 Multi‑arch vs universal binary
- This spec retains per‑arch zips for clarity and simplicity.
- Optional extension (not required now):
  - Build a universal binary using lipo:
    - lipo -create -output dist/aifo-coder-macos-universal \
        dist/aifo-coder-macos-arm64 dist/aifo-coder-macos-x86_64
  - Sign, notarize, and zip the universal binary instead.
- If implemented later, ensure the Makefile and CI jobs explicitly choose one strategy:
  - Either per‑arch zips, or universal binary zip, but not both as primary artifacts.

5.5 Interaction with existing DMG flow
- The DMG flow remains the canonical “drag‑and‑drop” install path for macOS.
- This spec adds:
  - Signed/notarized per‑arch zips for direct CLI usage.
- No change to:
  - release-app
  - release-dmg
  - release-dmg-sign
- However, doc updates (README/docs) should clarify:
  - For GUI users: prefer DMG.
  - For CLI‑only users or automated distribution: prefer the per‑arch zips.

-------------------------------------------------------------------------------
Phase 6 – Developer and CI operator workflows
-------------------------------------------------------------------------------

6.1 Local macOS developer workflow
- Once the certificate and notary profile are configured:
  - Build macOS binaries locally (or copy from Linux CI artifacts into dist/).
  - Run:
    - make release-macos-binary-signed SIGN_IDENTITY="Developer ID Application: <Org Name> (<TEAMID>)" \
        NOTARY_PROFILE="AifoNotary"
  - Artifacts:
    - dist/aifo-coder-macos-arm64 (signed, notarized).
    - dist/aifo-coder-macos-x86_64 (signed, notarized).
    - dist/aifo-coder-macos-arm64.zip (stapled).
    - dist/aifo-coder-macos-x86_64.zip (stapled).
  - Optional: also build DMG:
    - make release-dmg-sign SIGN_IDENTITY="Developer ID Application: <Org Name> (<TEAMID>)" \
        NOTARY_PROFILE="AifoNotary"

6.2 CI operator checklist
- Ensure:
  - macOS runner is available with Xcode CLT.
  - Developer ID Application certificate is either:
    - Installed in login keychain, or
    - Importable via P12_BASE64/P12_PASSWORD/KEYCHAIN_PASSWORD.
  - Notary credentials (API key) are stored as protected variables if NOTARY_PROFILE is created at runtime.
  - sign-macos-binaries job is wired into the pipeline:
    - needs build-launcher-macos and build-launcher-macos-x86_64.
    - runs on tags.
    - exposes zipped artifacts for publish-release.

Outcome
- With this v2 spec implemented:
  - macOS users can download:
    - A signed/notarized DMG (GUI install).
    - Signed/notarized per‑arch zips (CLI usage).
  - All artifacts will be compatible with Gatekeeper without manual overrides, assuming identities
    and notarization are correctly configured.
  - Linux CI remains responsible only for building unsigned Mach-O binaries via osxcross; macOS hosts
    perform all signing/notarization operations.
