AIFO Coder: macOS Mach-O binary signing with Apple Developer ID Application (v1)
Scope and goals
- Sign and notarize macOS binaries (arm64, x86_64) for distribution outside the Mac App Store.
- Artifacts must run under Gatekeeper without manual overrides, ideally offline (stapled).
- Keep existing DMG signing/notarization; add signed/notarized standalone binary zips.

Certificate choice (validated)
- Use “Developer ID Application” to sign macOS apps distributed outside the Mac App Store.
- “Developer ID Installer” is only needed for .pkg installers (not required for raw binaries/DMG).
- “Apple/Mac Development/Distribution” are for Store/ad-hoc or development; do not use them.
- Only the Account Holder can issue Developer ID certificates for the team.

Obtain and install the Developer ID Application certificate
1) Generate a CSR on a Mac
   - Keychain Access → Certificate Assistant → Request a Certificate From a CA → save to disk.
   - Use RSA 2048-bit; choose a clear Common Name; keep the private key safe.
2) Issue the cert (Account Holder action)
   - Apple Developer portal → Certificates → “+” → Developer ID Application → upload CSR → download .cer.
3) Install locally
   - Double-click .cer to add it to the login keychain; confirm it pairs with the private key.
4) Optional for CI import
   - Export a password-protected .p12 (includes private key) from Keychain Access.
   - Store base64 of .p12 and its password as protected, masked CI variables.

Notarization credentials (required for notarytool)
- Create an App Store Connect API key (Issuer ID, Key ID, .p8 private key).
- Store securely on runner or pass via protected CI variables.
- Prefer notarytool with API key over Apple ID; pre-create a keychain profile or create per job.

Tooling and host requirements (consistency check)
- Codesign and notarization must run on macOS (xcrun/codesign/notarytool/stapler).
- Building can happen in Linux (osxcross) as done today, but signing must occur on macOS.
- Ensure Xcode Command Line Tools installed on the macOS runner/host.

Makefile integration (new Darwin-only targets)
Variables
- SIGN_IDENTITY: exact CN string from Keychain, e.g.
  “Developer ID Application: <Org Name> (<TEAMID>)”
- NOTARY_PROFILE: notarytool keychain profile name, e.g. “AifoNotary”
- ENTITLEMENTS (optional): entitlements.plist; not needed for a basic CLI.

Targets (sequence and details)
- sign-macos-bin
  Inputs: dist/aifo-coder-macos-arm64, dist/aifo-coder-macos-x86_64
  Steps per binary:
    1) Clear extended attributes: xattr -cr <binary>
    2) codesign:
       codesign --force --timestamp --options runtime \
         -s "$SIGN_IDENTITY" [-e "$ENTITLEMENTS"] <binary>
    3) Verify:
       codesign --verify --deep --strict --verbose=4 <binary>
       codesign -dv --verbose=4 <binary>
       spctl --assess --type exec --verbose=4 <binary> (best-effort)
- zip-macos-bin
  Create per-arch zip archives:
    dist/aifo-coder-macos-arm64.zip
    dist/aifo-coder-macos-x86_64.zip
  Include the signed binary plus README.md, LICENSE, NOTICE.
- notarize-macos-zip
  Submit zip(s) for notarization:
    xcrun notarytool submit dist/aifo-coder-macos-*.zip \
      --keychain-profile "$NOTARY_PROFILE" --wait
  Staple after approval:
    xcrun stapler staple dist/aifo-coder-macos-*.zip
    Attempt to staple binaries as well (best-effort):
      xcrun stapler staple dist/aifo-coder-macos-arm64 || true
      xcrun stapler staple dist/aifo-coder-macos-x86_64 || true
  Validate:
    xcrun stapler validate dist/aifo-coder-macos-*.zip || true
- release-macos-bin-signed (aggregate)
  Depends on producing macOS binaries first (local build or CI artifacts).
  Runs: sign-macos-bin → zip-macos-bin → notarize-macos-zip.

Notes on stapling raw binaries (gap addressed)
- Apple strongly supports stapling tickets to bundles or DMGs/zips.
- Standalone Mach-O may not support stapling in all cases; staple the zip container and DMG.
- If stapling the binary fails, keep the zip stapled; Gatekeeper can verify online when unzipped.
- For best offline behavior, prefer the signed/notarized DMG; zips are provided for convenience.

Optional universal binary (consistency)
- We currently build per-arch binaries. If desired, create a universal using lipo:
  lipo -create -output dist/aifo-coder-macos-universal \
       dist/aifo-coder-macos-arm64 dist/aifo-coder-macos-x86_64
- Sign and notarize the universal instead of separate zips; not mandatory.

GitLab CI integration (new macOS signing job)
Job: sign-macos-binaries
- Runner: macOS host with Xcode CLI tools (tags like “macos”). Must not use Linux containers.
- needs (artifacts):
  - build-launcher-macos (dist/aifo-coder-macos-arm64)
  - build-launcher-macos-x86_64 (dist/aifo-coder-macos-x86_64)
- variables (protected, masked as needed):
  - SIGN_IDENTITY
  - NOTARY_PROFILE
  - Optional cert import:
    - P12_BASE64, P12_PASSWORD, KEYCHAIN_PASSWORD
  - Optional notary profile creation:
    - NOTARY_KEY_ID, NOTARY_ISSUER_ID, APPLE_TEAM_ID, NOTARY_PRIVATE_KEY_BASE64
- script outline:
  1) Ensure tools: xcrun, codesign, notarytool, stapler
  2) If importing cert:
     security create-keychain -p "$KEYCHAIN_PASSWORD" build.keychain
     security default-keychain -s build.keychain
     security unlock-keychain -p "$KEYCHAIN_PASSWORD" build.keychain
     printf '%s' "$P12_BASE64" | base64 -d > dev_id.p12
     security import dev_id.p12 -k build.keychain -P "$P12_PASSWORD" \
       -T /usr/bin/codesign -T /usr/bin/security
     security set-key-partition-list -S apple-tool:,apple: -s \
       -k "$KEYCHAIN_PASSWORD" build.keychain
     security find-identity -p codesigning -v
  3) If creating notary profile at runtime:
     printf '%s' "$NOTARY_PRIVATE_KEY_BASE64" | base64 -d > AuthKey.p8
     xcrun notarytool store-credentials --keychain-profile "$NOTARY_PROFILE" \
       --team-id "$APPLE_TEAM_ID" --key-id "$NOTARY_KEY_ID" \
       --issuer "$NOTARY_ISSUER_ID" --private-key ./AuthKey.p8
  4) Run: make release-macos-bin-signed
  5) Post-verify both binaries:
     codesign --verify --deep --strict --verbose=4 dist/aifo-coder-macos-arm64
     codesign --verify --deep --strict --verbose=4 dist/aifo-coder-macos-x86_64
     spctl --assess --type exec --verbose=4 dist/aifo-coder-macos-arm64 || true
     spctl --assess --type exec --verbose=4 dist/aifo-coder-macos-x86_64 || true
  6) Artifacts:
     - dist/aifo-coder-macos-arm64.zip
     - dist/aifo-coder-macos-x86_64.zip
     - dist/aifo-coder-macos-arm64
     - dist/aifo-coder-macos-x86_64
- rules:
  - Run on tags; optionally allow MR opt-in when variables are present.
- fallback behavior:
  - If no macOS runner or secrets, skip this job; publish unsigned macOS binaries with a note.
  - Prefer making this job required for release tags once infra is ready.

Publish-release integration (assets)
- Add links for the stapled, notarized zips and the standalone signed binaries.
- Keep existing Linux tar.gz and macOS DMG artifacts.

Validation and consistency checks
- Identity string must match exactly; confirm via:
  security find-identity -p codesigning -v
- Use codesign flags “--timestamp --options runtime” for hardened runtime compliance.
- Notarize only .zip/.dmg/.pkg; Apple does not notarize .tar.gz.
- Ensure binaries are signed before notarization; notarytool will reject unsigned inputs.
- If spctl fails but codesign verify passes and notarization succeeded, prefer stapled DMG/zip.

Security and maintenance
- Mask and protect P12/keys in CI; scope to protected branches/tags.
- Rotate Developer ID certs and App Store Connect API keys before expiry; keep inventory of IDs.
- Clean up temporary files (dev_id.p12, AuthKey.p8); avoid storing secrets on disk when possible.

Developer workflow (macOS host)
- make release-for-mac
- export SIGN_IDENTITY and NOTARY_PROFILE (and optional ENTITLEMENTS)
- make release-macos-bin-signed
- Verify with codesign/spctl; distribute the zips/DMG.

Known limitations and decisions
- Signing cannot be performed inside the Linux osxcross container; a macOS host is required.
- Standalone binaries may rely on online verification after unzip when not stapled; stapled DMG
  or stapled zip provides better offline behavior.
- Entitlements are not needed for this CLI; avoid adding unless necessary.

Outcome
- Users can download signed, notarized macOS zips or the DMG from GitLab releases and run the CLI
  directly with Gatekeeper satisfied.
