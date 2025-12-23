AIFO Coder: macOS Mach-O binary signing (local-only, Apple Dev ID ready) – v4, phased

Status
- This v4 spec refines v3 for a pure local workflow:
  - All building, signing, zipping, and optional notarization happen on a macOS developer machine.
  - CI is not involved in signing/notarization at all; it only builds Linux images/binaries as today.

Scope
- Provide Makefile targets for macOS developers to:
  - Build macOS Mach-O binaries locally.
  - Sign those binaries with:
    - A currently configured local certificate (self-signed or enterprise) and
    - Later, an Apple “Developer ID Application” certificate when available.
  - Optionally notarize and staple signed binary zips using notarytool when Apple credentials exist.
- Produce per-arch zip archives suitable for direct download (e.g., attaching to GitLab releases) and
  safe execution under Gatekeeper.
- Keep the existing DMG flow (release-app / release-dmg / release-dmg-sign) unchanged and orthogonal.

Non-goals
- No macOS signing or notarization in CI.
- No CI-based upload of signed artifacts (publish remains a manual step for signed binaries/zips).
- No behavioral changes to the Rust binary; this is purely build/packaging/signing.

-------------------------------------------------------------------------------
Phase 0 – Preconditions, artifacts, and terminology
-------------------------------------------------------------------------------

Artifacts (local)
- macOS binaries built locally via Makefile:
  - target/aarch64-apple-darwin/release/aifo-coder
  - target/x86_64-apple-darwin/release/aifo-coder (optional on Apple Silicon)
- For signing/zipping, we standardize on:
  - dist/aifo-coder-macos-arm64
  - dist/aifo-coder-macos-x86_64

Terminology
- “Binary”: raw Mach-O executable (dist/aifo-coder-macos-<arch>).
- “Zip”: .zip archive containing one signed binary + README.md + NOTICE + LICENSE; notarization input.
- “DMG”: drag-and-drop macOS disk image built by release-dmg / release-dmg-sign (existing flow).
- “Developer ID Application certificate”:
  - Apple certificate for distributing macOS apps/binaries outside the Mac App Store; required for
    public notarization-ready builds.
- “Local cert”:
  - Any certificate installed on the developer’s Mac keychain (self-signed, enterprise, or Apple).

Key constraints
- codesign, notarytool, stapler, hdiutil are macOS-only.
- All signing/notarization is done locally on a macOS host.
- Apple will notarize only .zip/.dmg/.pkg; our per-arch zips are the primary notarization container.
- Gatekeeper behavior:
  - Signed + notarized + stapled artifacts provide best UX.
  - Self-signed and/or non-notarized artifacts will cause warnings unless the certificate is trusted.

-------------------------------------------------------------------------------
Phase 1 – Certificate strategy (local only)
-------------------------------------------------------------------------------

1.1 Current state: local/self-signed certificate
- Immediate goal:
  - Allow developers to sign binaries with a local certificate (self-signed or enterprise) to reduce
    Gatekeeper friction on their own machines or in a trusted team.
- Behavior and limitations:
  - Self-signed certs can be used with codesign.
  - These signatures are NOT acceptable for notarization; notarytool will reject them.
  - Binaries may still trigger Gatekeeper warnings on machines that do not trust the certificate.

Self-signed certificate (recap)
- Create via Keychain Access (login keychain):
  - Menu: Keychain Access → Certificate Assistant → Create a Certificate…
  - Name: e.g., “Migros AI Foundation Code Signer”
  - Identity Type: Self Signed Root
  - Certificate Type: Code Signing
  - Key Size: 2048 or 4096
  - Location: login
- Verify:
  - Certificate appears in “login” keychain.
  - Private key entry exists under that certificate.
- Use this Common Name as SIGN_IDENTITY for local signing Make targets.

1.2 Future: Developer ID Application certificate (Apple Developer Program)
- For public distribution (external users), we will later obtain an Apple Developer ID Application cert.
- This must be created by the Apple Developer Account Holder.
- Creation steps:
  - Apple Developer portal → Certificates, Identifiers & Profiles → Certificates → “+”.
  - Select “Developer ID Application”.
  - Upload CSR generated via Keychain Access (“Request a Certificate From a CA”).
  - Download and install resulting .cer into login keychain (with its corresponding private key).
- Identity to use:
  - Retrieve via:
    - security find-identity -p codesigning -v
  - Look for entries such as:
    - “Developer ID Application: <Org Name> (<TEAMID>)”
- With this identity:
  - sign-macos-binaries will enable hardened runtime flags and timestamps.
  - notarize-macos-binary-zips may be used (if NOTARY_PROFILE is configured).

1.3 Notary credentials for local developer (optional, later)
- For notarization, the developer must have:
  - An App Store Connect API key (Issuer ID, Key ID, private key .p8) bound to their team.
- They configure a notarytool keychain profile once on their local machine:
  - xcrun notarytool store-credentials \
      --keychain-profile "<profile-name>" \
      --team-id "<TEAMID>" \
      --key-id "<KEY_ID>" \
      --issuer "<ISSUER_ID>" \
      --private-key /path/to/AuthKey_XXXXXX.p8
- The profile name is used as NOTARY_PROFILE in the Makefile.

1.4 Local environment expectations
- A macOS host with:
  - Xcode Command Line Tools installed (xcrun, codesign, notarytool, stapler, hdiutil).
  - Keychain with:
    - A self-signed or enterprise code signing cert for immediate local signing.
    - Optionally, a Developer ID Application cert for public distribution.
  - Optional: notarytool profile configured for notarization.

-------------------------------------------------------------------------------
Phase 2 – Makefile extensions (build + local signing/zipping)
-------------------------------------------------------------------------------

2.1 Variables (semantics)
- Existing variables (relevant):
  - DIST_DIR ?= dist
  - BIN_NAME ?= aifo-coder
  - APP_NAME, APP_BUNDLE_ID, DMG_NAME, SIGN_IDENTITY, NOTARY_PROFILE, DMG_BG
- Extended semantics for signing path:
  - SIGN_IDENTITY:
    - Common Name of the certificate to be used by codesign.
    - Examples:
      - “Migros AI Foundation Code Signer” (self-signed).
      - “Developer ID Application: Migros AI Foundation (TEAMID)” (Apple Dev ID).
  - NOTARY_PROFILE:
    - Optional keychain profile for xcrun notarytool.
    - When empty, notarization is skipped cleanly (no error).

2.2 Targets to build macOS binaries into dist/ locally
Goal:
- Provide a local one-liner to build Mach-O binaries into dist/ in the exact paths expected by the
  signing targets.

New target: build-macos-binaries-local (Darwin-only)
- Behavior:
  - Only runs on macOS (uname -s == Darwin); otherwise prints an explanatory message and exits 1.
  - Detects host arch:
    - arm64 or aarch64 → builds aarch64-apple-darwin by default; x86_64 optional.
    - x86_64 → builds x86_64-apple-darwin by default; arm64 optional.
  - Uses rustup stable if available, otherwise plain cargo.
  - Does NOT rely on Docker or osxcross.
- Steps (conceptual):
  - Choose target triples:
    - On Apple Silicon (arm64):
      - aarch64-apple-darwin (always)
      - optionally x86_64-apple-darwin if rustup target installed (the target is present locally).
    - On Intel mac:
      - x86_64-apple-darwin (always).
  - For each chosen target T:
    - rustup target add T (best-effort) or skip if not needed.
    - cargo build $(CARGO_FLAGS) --release --target T
    - Copy:
      - aarch64-apple-darwin → dist/aifo-coder-macos-arm64
      - x86_64-apple-darwin → dist/aifo-coder-macos-x86_64
    - Optionally run file(1) on the result to sanity-check the Mach-O architecture.

2.3 New local-only signing/packaging targets
We add Darwin-guarded Makefile targets:

- sign-macos-binaries
- zip-macos-binaries
- notarize-macos-binary-zips
- release-macos-binary-signed (aggregate)

These targets:
- Are intended to be run after build-macos-binaries-local.
- Must not be called from CI; they are for local developer use only.

2.3.1 sign-macos-binaries (Darwin-only)
Purpose:
- Sign dist/aifo-coder-macos-arm64 and dist/aifo-coder-macos-x86_64 in-place using SIGN_IDENTITY.

Behavior:
- Guard:
  - If uname -s != Darwin: print a clear message and exit 1.
- Input checks:
  - Require:
    - dist/aifo-coder-macos-arm64
    - dist/aifo-coder-macos-x86_64
  - If either is missing:
    - Error out and suggest running make build-macos-binaries-local first.
- Extended attributes:
  - Run (best-effort):
    - xattr -cr dist/aifo-coder-macos-arm64 || true
    - xattr -cr dist/aifo-coder-macos-x86_64 || true
  - This helps avoid resource fork / Finder info issues.

Signing mode selection:
- Determine whether SIGN_IDENTITY refers to an Apple Developer identity or a generic/local cert:
  - Use:
    - security find-certificate -a -c "$SIGN_IDENTITY" -Z -p
  - Optionally pipe to:
    - openssl x509 -noout -subject
  - If subject contains “Developer ID Application”, “Apple Distribution”, or “Apple Development”:
    - Treat as APPLE_DEV=1.
  - Else:
    - APPLE_DEV=0 (self-signed or non-Apple cert).

Signing flags:
- If APPLE_DEV=1:
  - SIGN_FLAGS="--force --timestamp --options runtime --verbose=4"
  - This enables hardened runtime, required for notarization.
- If APPLE_DEV=0:
  - SIGN_FLAGS="--force --verbose=4"
  - No hardened runtime; not notarizable, but fine for internal use.

Keychain selection:
- Use the default user keychain:
  - KEYCHAIN="$(security default-keychain -d user | tr -d ' \"')"
- Pass this via:
  - --keychain "$KEYCHAIN" where possible.

Signing strategy per binary B:
- For each of dist/aifo-coder-macos-arm64, dist/aifo-coder-macos-x86_64:
  1) Try:
     - codesign $SIGN_FLAGS --keychain "$KEYCHAIN" -s "$SIGN_IDENTITY" "$B"
  2) If that fails:
     - Extract SHA-1 hash:
       - SIG_SHA1="$(security find-certificate -a -c "$SIGN_IDENTITY" -Z 2>/dev/null | awk '/^SHA-1 hash:/{print $3; exit}')"
     - If non-empty:
       - codesign $SIGN_FLAGS --keychain "$KEYCHAIN" -s "$SIG_SHA1" "$B"
  3) If still failing:
     - If APPLE_DEV=0 (non-Apple identity):
       - Print a warning and fall back to ad-hoc:
         - codesign $SIGN_FLAGS -s - "$B"
       - This ensures a signature exists for local testing even if the identity is misconfigured.
     - If APPLE_DEV=1:
       - Do NOT fallback to ad-hoc.
       - Print an error explaining that Developer ID signing failed and exit non-zero.
       - This avoids producing binaries that look “signed” but are not notarizable.

Verification:
- For each signed binary:
  - codesign --verify --strict --verbose=4 "$B"
  - codesign -dv --verbose=4 "$B" (for diagnostic output).
  - spctl --assess --type exec --verbose=4 "$B" || true (gatekeeper check; non-fatal).
- If APPLE_DEV=1 and codesign verification fails:
  - Exit non-zero and print diagnostics.
- If APPLE_DEV=0 and codesign verification fails:
  - Print a warning to the developer; they can choose whether to proceed (but exit non-zero).

Outcome:
- For self-signed/local cert:
  - Binaries are signed (possibly ad-hoc) and can be run locally with fewer surprises.
- For Developer ID:
  - Binaries are properly signed with hardened runtime and are ready for notarization.
  - If the identity is misconfigured, the target fails (no misleading ad-hoc fallback).

2.3.2 zip-macos-binaries (platform-independent)
Purpose:
- Create per-arch zip archives containing the signed binaries and documentation.

Inputs:
- dist/aifo-coder-macos-arm64
- dist/aifo-coder-macos-x86_64
- README.md, NOTICE, LICENSE (from repo root)

Outputs:
- dist/aifo-coder-macos-arm64.zip
- dist/aifo-coder-macos-x86_64.zip

Behavior:
- This target is logically platform-independent, but intended for use primarily on macOS after signing.
- Steps (per arch):
  - Ensure dist/ exists.
  - Use staging dirs:
    - dist/.zip-stage-arm64
    - dist/.zip-stage-x86_64
  - For arm64:
    - mkdir -p dist/.zip-stage-arm64
    - cp dist/aifo-coder-macos-arm64 dist/.zip-stage-arm64/aifo-coder-macos-arm64
    - cp README.md NOTICE LICENSE into stage dir; if any missing, error out (these are required).
    - (cd dist/.zip-stage-arm64 && zip -9r ../aifo-coder-macos-arm64.zip .)
    - rm -rf dist/.zip-stage-arm64
  - For x86_64:
    - Same pattern, with dist/.zip-stage-x86_64 and aifo-coder-macos-x86_64.
- Preconditions:
  - Require that binaries exist; do not rebuild them automatically.
- No signing occurs here; sign-macos-binaries must be run first.

2.3.3 notarize-macos-binary-zips (Darwin-only, optional)
Purpose:
- Notarize the per-arch macOS binary zips and staple tickets.

Inputs:
- dist/aifo-coder-macos-arm64.zip
- dist/aifo-coder-macos-x86_64.zip
- SIGN_IDENTITY (ideally Developer ID Application).
- NOTARY_PROFILE (optional; required for actual notarization).

Behavior:
- Guard:
  - If uname -s != Darwin: print a clear message and exit 1 (notarization is macOS-only).
- Pre-check:
  - If NOTARY_PROFILE is empty or unset:
    - Print: “NOTARY_PROFILE unset; skipping notarization/stapling (non-fatal).”
    - Exit 0.
  - If xcrun notarytool is not available:
    - Print: “xcrun notarytool not found; skipping notarization/stapling (non-fatal).”
    - Exit 0.
- Zip existence:
  - Require both zip files.
  - If either is missing, error and suggest running make zip-macos-binaries.

Notarization flow:
- Submit and wait for each zip:
  - xcrun notarytool submit dist/aifo-coder-macos-arm64.zip \
      --keychain-profile "$NOTARY_PROFILE" \
      --wait
  - xcrun notarytool submit dist/aifo-coder-macos-x86_64.zip \
      --keychain-profile "$NOTARY_PROFILE" \
      --wait
- On any failure:
  - Print notarytool’s output.
  - Exit non-zero.

Stapling:
- After successful submissions:
  - xcrun stapler staple dist/aifo-coder-macos-arm64.zip || true
  - xcrun stapler staple dist/aifo-coder-macos-x86_64.zip || true
- Best-effort staples on raw binaries:
  - xcrun stapler staple dist/aifo-coder-macos-arm64 || true
  - xcrun stapler staple dist/aifo-coder-macos-x86_64 || true

Validation:
- Best-effort:
  - xcrun stapler validate dist/aifo-coder-macos-arm64.zip || true
  - xcrun stapler validate dist/aifo-coder-macos-x86_64.zip || true

Outcome:
- With Developer ID + NOTARY_PROFILE:
  - Per-arch zips are notarized and stapled.
  - Raw binaries may or may not accept staples; either way, Gatekeeper recognizes the notarization.
- With self-signed/local cert or missing NOTARY_PROFILE:
  - Target becomes a no-op with explicit skip messages.
  - Binaries remain signed but not notarized.

2.3.4 release-macos-binary-signed (Darwin-only aggregate)
Purpose:
- Provide a single Make target for macOS developers that:
  - Builds binaries locally.
  - Signs them.
  - Zips them.
  - Optionally notarizes/staples them.

Behavior:
- Guard:
  - If uname -s != Darwin: print message and exit 1.
- Sequence:
  - make build-macos-binaries-local
  - make sign-macos-binaries
  - make zip-macos-binaries
  - make notarize-macos-binary-zips
- Notes:
  - With self-signed cert and NOTARY_PROFILE unset:
    - Binaries are signed.
    - Zips are created.
    - Notarization step prints a skip message and exits 0.
  - With Developer ID + NOTARY_PROFILE:
    - Full pipeline runs, producing notarized, stapled zips suitable for public distribution.

-------------------------------------------------------------------------------
Phase 3 – Local developer workflows
-------------------------------------------------------------------------------

3.1 Local self-signed workflow (short term)
Goal:
- Allow developers to produce signed macOS binaries/zips for internal/local use.

Steps:
1) On macOS, ensure Rust toolchain and certificate:
   - Install Rust: https://rustup.rs
   - Create or reuse a local code signing certificate:
     - e.g., SIGN_IDENTITY="AI Foundation Code Signer"
2) Build, sign, and zip:
   - export SIGN_IDENTITY="Migros AI Foundation Code Signer"
   - unset NOTARY_PROFILE (or leave empty).
   - make release-macos-binary-signed
3) Verify on local machine:
   - codesign --verify --deep --strict --verbose=4 dist/aifo-coder-macos-arm64
   - codesign --verify --deep --strict --verbose=4 dist/aifo-coder-macos-x86_64
   - spctl --assess --type exec --verbose=4 dist/aifo-coder-macos-arm64 || true
   - spctl --assess --type exec --verbose=4 dist/aifo-coder-macos-x86_64 || true

Result:
- dist/aifo-coder-macos-arm64 (signed, non-notarized)
- dist/aifo-coder-macos-x86_64 (signed, non-notarized)
- dist/aifo-coder-macos-arm64.zip (signed)
- dist/aifo-coder-macos-x86_64.zip (signed)

Notes:
- External users who do not trust the self-signed cert will still get Gatekeeper prompts.
- For internal teams, you can distribute the cert (or manage trust centrally) to reduce warnings.

3.2 Local Developer ID workflow (medium/long term)
Goal:
- Produce signed and notarized macOS binary zips suitable for public distribution.

Pre-setup:
- Developer has:
  - Developer ID Application certificate installed in login keychain.
  - Notary profile configured via:
    - xcrun notarytool store-credentials --keychain-profile "AifoNotary" ...

Steps:
1) On macOS, build and sign:
   - export SIGN_IDENTITY="Developer ID Application: <Org Name> (<TEAMID>)"
   - export NOTARY_PROFILE="AifoNotary"
   - make release-macos-binary-signed
2) Wait for the full pipeline:
   - build-macos-binaries-local
   - sign-macos-binaries (with hardened runtime)
   - zip-macos-binaries
   - notarize-macos-binary-zips

Result:
- dist/aifo-coder-macos-arm64 (signed, notarized)
- dist/aifo-coder-macos-x86_64 (signed, notarized)
- dist/aifo-coder-macos-arm64.zip (stapled)
- dist/aifo-coder-macos-x86_64.zip (stapled)

3.3 Relationship to DMG signing (release-dmg-sign)
- DMG flow (existing) remains the primary GUI-friendly install path.
- Binary zips from this spec are primarily for:
  - CLI users downloading from GitLab.
  - Scripted installation flows (curl + unzip + move into PATH).
- Developer may choose:
  - make release-dmg-sign SIGN_IDENTITY="Developer ID Application: ..." NOTARY_PROFILE="AifoNotary"
    - for DMG builds.
  - make release-macos-binary-signed
    - for raw Mach-O binaries + zips.

It is valid to reuse the same SIGN_IDENTITY and NOTARY_PROFILE for both flows.

-------------------------------------------------------------------------------
Phase 4 – Release packaging (manual upload + docs)
-------------------------------------------------------------------------------

4.1 What CI provides
- CI continues to do:
  - Linux binary and tarball builds.
  - macOS cross builds (if enabled) but WITHOUT signing or notarization.
- These CI artifacts are considered “unsigned/unnotarized”.
- This spec assumes official macOS signed artifacts are produced manually on a Mac using Make targets.

4.2 Recommended release assets for macOS (manual upload)
After running make release-macos-binary-signed with a Developer ID + NOTARY_PROFILE, the developer
should manually upload (e.g., in GitLab release UI):

- macOS:
  - aifo-coder-macos-arm64.zip (signed; notarized/stapled when Apple Dev ID used)
  - aifo-coder-macos-x86_64.zip (same)
  - Optionally:
    - aifo-coder-macos-arm64 (signed raw binary; for advanced users)
    - aifo-coder-macos-x86_64 (signed raw binary)
  - DMG from release-dmg-sign (recommended for GUI users)
- Linux:
  - Assets as per existing pipeline (unchanged).

4.3 Documentation updates
- README / INSTALL should:
  - Explicitly state that:
    - CI macOS tarballs and binaries are unsigned and may require Gatekeeper overrides.
    - Official signed, notarized macOS binaries are produced by local Make targets and attached to
      releases manually.
  - Recommend:
    - CLI users: use per-arch signed/notarized zips.
    - GUI users: use signed/notarized DMG.

-------------------------------------------------------------------------------
Phase 5 – Consistency checks and corner cases
-------------------------------------------------------------------------------

5.1 Self-signed vs Developer ID behavior
- Self-signed / non-Apple cert:
  - sign-macos-binaries:
    - Uses basic flags (no hardened runtime).
    - May fall back to ad-hoc if the identity cannot be used.
  - notarize-macos-binary-zips:
    - Will skip notarization (NOTARY_PROFILE usually unset; even if set, Apple will reject).
- Developer ID Application:
  - sign-macos-binaries:
    - Uses hardened runtime + timestamp.
    - Does NOT fallback to ad-hoc; failure is fatal so developer must fix configuration.
  - notarize-macos-binary-zips:
    - Performs full submit/wait/staple when NOTARY_PROFILE is set and tools exist.

5.2 Stapling raw binaries vs zips
- Preferred stapling targets:
  - Zips, DMGs, app bundles.
- Raw Mach-O binaries:
  - May or may not support stapling; these failures are non-fatal.
- Policy:
  - Always attempt to staple zips.
  - Always try stapling the raw binaries but ignore failures (log warnings).
  - Users should prefer zips or DMG for best offline Gatekeeper behavior.

5.3 Architecture strategy
- This spec standardizes on per-arch artifacts:
  - dist/aifo-coder-macos-arm64[.zip]
  - dist/aifo-coder-macos-x86_64[.zip]
- Optional future extension (not part of v4):
  - Generate a universal binary via lipo and sign/notarize that single binary.

5.4 Error handling and developer feedback
- All targets must:
  - Print clear messages on failures (missing binaries, missing certs, missing tools).
  - Exit with non-zero status when they cannot complete the tasks required for a safe artifact.
- Particularly:
  - sign-macos-binaries with APPLE_DEV=1 and identity errors should:
    - Explain that a Developer ID cert is required and how to inspect identities with security find-identity.
  - notarize-macos-binary-zips should clearly distinguish:
    - “notarytool not installed” vs “NOTARY_PROFILE unset” vs real notarization failure.

-------------------------------------------------------------------------------
Phase 6 – Developer checklist and troubleshooting
-------------------------------------------------------------------------------

6.1 Pre-flight checklist (macOS)
- Confirm:
  - xcrun, codesign, notarytool, stapler, hdiutil are in PATH:
    - xcrun --version
    - codesign --version
    - xcrun notarytool --help
    - xcrun stapler --help
  - login keychain is unlocked (if needed):
    - security default-keychain -d user
    - security unlock-keychain -p "<password>" login.keychain-db
  - SIGN_IDENTITY matches an existing certificate CN:
    - security find-certificate -a -c "$SIGN_IDENTITY" -Z
  - For Developer ID:
    - Use security find-identity -p codesigning -v to ensure identity is visible.
    - Confirm subject includes “Developer ID Application” and the correct TEAMID.

6.2 Common issues
- “code object is not signed at all”:
  - Re-run make sign-macos-binaries and ensure codesign verification passes.
- “codesign: invalid signature” or “resource fork, Finder info, or similar detritus not allowed”:
  - Make sure xattr -cr has been applied before signing and zipping.
- “notarytool: error ... invalid signature”:
  - Likely using a self-signed cert or ad-hoc; notarization requires Developer ID.
- “notarytool: authentication failure”:
  - Check NOTARY_PROFILE configuration; run xcrun notarytool history --keychain-profile "<profile>".
- “Operation not permitted” during codesign:
  - Keychain may be locked or codesign lacks permission to use the private key; unlock keychain and
    accept access prompts.

6.3 Regression testing (local)
- On a macOS machine with appropriate tools:
  - make build-macos-binaries-local
  - make sign-macos-binaries
  - make zip-macos-binaries
  - Optionally: make notarize-macos-binary-zips
- Validate:
  - codesign --verify --deep --strict --verbose=4 dist/aifo-coder-macos-*
  - xcrun notarytool history --keychain-profile "$NOTARY_PROFILE" (if used).
  - xcrun stapler validate dist/aifo-coder-macos-*.zip || true

Outcome
- With v4 implemented:
  - macOS developers can locally build, sign, and (optionally) notarize Mach-O binaries.
  - No CI runners are needed for signing; CI remains Linux-only.
  - Signed zips + DMG can be manually attached to GitLab releases as official macOS artifacts.
