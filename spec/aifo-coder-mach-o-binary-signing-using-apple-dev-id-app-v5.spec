AIFO Coder: macOS Mach-O binary signing (local-only, Apple Dev ID ready) – v5, phased

Status
- This v5 spec refines v4 and consolidates around existing Makefile targets:
  - We ALREADY have targets to build macOS binaries:
    - build-launcher (host-native; macOS builds)
    - build-launcher-macos-cross, build-launcher-macos-cross-arm64, build-launcher-macos-cross-x86_64 (osxcross)
  - We DO NOT add redundant “build-macos-binaries-local”; instead, we standardize how to derive signing inputs from these existing targets.

Scope
- Provide local-only Makefile targets to:
  - Normalize already-built macOS binaries into canonical dist/ paths.
  - Sign those binaries with:
    - A currently configured local certificate (self-signed or enterprise) and
    - Later, an Apple “Developer ID Application” certificate when available.
  - Optionally notarize and staple signed binary zips using notarytool when Apple credentials exist.
- Produce per-arch zip archives suitable for direct download (e.g., attaching to GitLab releases) and
  safe execution under Gatekeeper.
- Keep the existing DMG flow (release-app / release-dmg / release-dmg-sign) unchanged and orthogonal.

Non-goals
- No signing or notarization in CI (no macOS runners).
- No changes to osxcross builder or CI macOS build jobs; they continue to produce unsigned binaries.
- No runtime behavior changes in the Rust binary.

-------------------------------------------------------------------------------
Phase 0 – Preconditions, artifacts, and terminology
-------------------------------------------------------------------------------

Artifacts (canonical paths for signing)
- We standardize on the following signing inputs in dist/:
  - dist/aifo-coder-macos-arm64
  - dist/aifo-coder-macos-x86_64

How these binaries are produced (no duplication)
- On macOS hosts (local, native):
  - Existing Makefile target:
    - build-launcher
      - On macOS (Darwin), uses host rustup/cargo and builds:
        - target/aarch64-apple-darwin/release/aifo-coder    (Apple Silicon)
        - target/x86_64-apple-darwin/release/aifo-coder     (Intel)
      - For signing, we normalize these into dist/ via a new “normalize” target (see Phase 2).

- On Linux / CI via osxcross:
  - Existing Makefile target:
    - build-launcher-macos-cross
      - Builds macOS arm64 and x86_64 using the macos-cross-rust-builder image:
        - target/aarch64-apple-darwin/release/aifo-coder
        - target/x86_64-apple-darwin/release/aifo-coder
      - Additional convenience:
        - build-launcher-macos-cross-arm64
        - build-launcher-macos-cross-x86_64
      - CI jobs use these and then copy into dist/aifo-coder-macos-{arm64,x86_64}.
  - Local Linux developers may also use these, but signing and notarization remain macOS-only.

Terminology
- “Binary”: raw Mach-O executable in dist/aifo-coder-macos-<arch>.
- “Zip”: .zip archive containing one signed binary + README.md + NOTICE + LICENSE; notarization input.
- “DMG”: drag-and-drop macOS disk image built by release-dmg / release-dmg-sign.
- “Developer ID Application certificate”: Apple certificate for distributing macOS apps/binaries outside the Mac App Store; required for notarization.
- “Local cert”: any code-signing certificate installed on the Mac keychain (self-signed, enterprise, or Apple).

Key constraints
- codesign, notarytool, stapler, hdiutil are macOS-only.
- All signing/notarization runs on a macOS host.
- Apple notarizes only .zip/.dmg/.pkg; our per-arch zips are the primary notarization container.
- Gatekeeper behavior:
  - Signed + notarized + stapled = best UX.
  - Self-signed and/or non-notarized = prompts on machines that do not trust the certificate.

-------------------------------------------------------------------------------
Phase 1 – Certificate strategy (local only)
-------------------------------------------------------------------------------

1.1 Current state: local/self-signed certificate
- Immediate goal:
  - Let developers sign binaries with a local certificate (self-signed or enterprise) for internal use.
- Behavior:
  - Self-signed certs can be used with codesign.
  - Not acceptable for notarization; notarytool will reject them.
  - Binaries may still trigger Gatekeeper warnings on machines that do not trust the cert.

Self-signed certificate (recap)
- Create via Keychain Access (login keychain):
  - Keychain Access → Certificate Assistant → Create a Certificate…
  - Name: e.g. “Migros AI Foundation Code Signer”
  - Identity Type: Self Signed Root
  - Certificate Type: Code Signing
  - Key Size: 2048 or 4096
  - Location: login
- Verify:
  - Certificate appears in “login” keychain.
  - Private key entry exists under that certificate.
- Use this Common Name as SIGN_IDENTITY for local signing.

1.2 Future: Developer ID Application certificate (Apple Developer Program)
- For public distribution, we will later use a Developer ID Application cert.
- Account Holder must create it:
  - Apple Developer portal → Certificates → “+” → Developer ID Application.
  - Upload CSR from Keychain Access (“Request a Certificate From a CA”).
  - Install .cer into login keychain (with private key).
- Identity string:
  - security find-identity -p codesigning -v
  - Look for: “Developer ID Application: <Org Name> (<TEAMID>)”.
- With this identity:
  - Our signing Make target will enable hardened runtime flags and timestamps.
  - notarytool can be used to notarize.

1.3 Notary credentials for local developer (optional, later)
- Developer must have:
  - App Store Connect API key (Issuer ID, Key ID, .p8 private key).
- Configure notarytool profile once:
  - xcrun notarytool store-credentials \
      --keychain-profile "<profile-name>" \
      --team-id "<TEAMID>" \
      --key-id "<KEY_ID>" \
      --issuer "<ISSUER_ID>" \
      --private-key /path/to/AuthKey_XXXXXX.p8
- Use this profile name as NOTARY_PROFILE in Make targets.

1.4 Local environment expectations
- macOS host with:
  - Xcode Command Line Tools installed (xcrun, codesign, notarytool, stapler, hdiutil).
  - Keychain with:
    - A self-signed or enterprise code signing cert for immediate local signing.
    - Optionally, a Developer ID Application cert for public builds.
  - Optional: notarytool keychain profile configured.

-------------------------------------------------------------------------------
Phase 2 – Makefile extensions (normalize + local signing/zipping)
-------------------------------------------------------------------------------

2.1 Variables (semantics and reuse)
- Existing variables (relevant):
  - DIST_DIR ?= dist
  - BIN_NAME ?= aifo-coder
  - APP_NAME, APP_BUNDLE_ID, DMG_NAME, SIGN_IDENTITY, NOTARY_PROFILE, DMG_BG.
- Semantics:
  - SIGN_IDENTITY:
    - Common Name of the certificate to use with codesign.
    - Examples:
      - “Migros AI Foundation Code Signer” (self-signed).
      - “Developer ID Application: Migros AI Foundation (TEAMID)” (Apple Dev ID).
  - NOTARY_PROFILE:
    - Optional keychain profile for xcrun notarytool.
    - When empty, notarization is skipped cleanly (no error).

2.2 Normalize existing macOS binaries into dist/ (no duplicate build target)
Goal:
- Use the binaries already produced by existing targets (`build-launcher` or osxcross) and standardize the signing inputs.

New target: release-macos-binaries-normalize-local (Darwin-only)
- Behavior:
  - Only runs on macOS (uname -s == Darwin); otherwise print message and exit 1.
  - Checks for existing local macOS binaries in target/:
    - Prefer:
      - target/aarch64-apple-darwin/release/aifo-coder
      - target/x86_64-apple-darwin/release/aifo-coder
    - If only one arch is present (e.g. only arm64), only that one is normalized; the other is skipped with a message.
  - Copies them into canonical dist paths:
    - mkdir -p dist
    - cp target/aarch64-apple-darwin/release/aifo-coder dist/aifo-coder-macos-arm64  (if exists)
    - cp target/x86_64-apple-darwin/release/aifo-coder dist/aifo-coder-macos-x86_64 (if exists)
  - Optionally, run file(1) to validate Mach-O type:
    - file dist/aifo-coder-macos-arm64 | grep -qi 'Mach-O 64-bit arm64'
    - file dist/aifo-coder-macos-x86_64 | grep -qi 'Mach-O 64-bit x86_64'
  - Does NOT run cargo build by itself:
    - Upstream targets are responsible for building (e.g. `make build-launcher`).
    - This avoids duplicate “build-macos-binaries-local”.

Relationship to existing targets:
- On macOS:
  - Developer runs:
    - make build-launcher
    - make release-macos-binaries-normalize-local
  - or:
    - directly run release-app/release-dmg-sign for DMG, plus normalize for raw binaries.
- On Linux:
  - normalize-macos-binaries-local is not intended to run; signing/notarization is macOS-only.

2.3 New local-only signing/packaging targets (Darwin-only for signing/notarization)
We add three new targets plus one aggregate, all Darwin-guarded:

- release-macos-binaries-sign
- release-macos-binaries-zips
- release-macos-binaries-zips-notarize
- release-macos-binary-signed (aggregate)

These targets:
- Operate on dist/aifo-coder-macos-arm64 and dist/aifo-coder-macos-x86_64.
- Do NOT build binaries themselves; they assume normalize-macos-binaries-local and/or osxcross-based builds have populated dist/.

2.3.1 release-macos-binaries-sign (Darwin-only)
Purpose:
- Sign dist/aifo-coder-macos-arm64 and dist/aifo-coder-macos-x86_64 inplace using SIGN_IDENTITY.

Behavior:
- Guard:
  - If uname -s != Darwin: print a clear message and exit 1.
- Input checks:
  - Require:
    - At least one of dist/aifo-coder-macos-arm64 and dist/aifo-coder-macos-x86_64 to exist.
  - If both are missing:
    - Error out and suggest running:
      - make build-launcher (on macOS) and release-macos-binaries-normalize-local
      - or using osxcross and copying into dist/.
- Extended attributes:
  - xattr -cr dist/aifo-coder-macos-arm64  || true
  - xattr -cr dist/aifo-coder-macos-x86_64 || true

Signing mode selection:
- Determine whether SIGN_IDENTITY is an Apple Developer identity:
  - security find-certificate -a -c "$SIGN_IDENTITY" -Z -p
    - If subject contains “Developer ID Application”, “Apple Distribution”, or “Apple Development”:
      - APPLE_DEV=1
    - Else:
      - APPLE_DEV=0 (self-signed or non-Apple cert).

Signing flags:
- If APPLE_DEV=1:
  - SIGN_FLAGS="--force --timestamp --options runtime --verbose=4"
- If APPLE_DEV=0:
  - SIGN_FLAGS="--force --verbose=4"

Keychain:
- Default user keychain:
  - KEYCHAIN="$(security default-keychain -d user | tr -d ' \"')"
- Include --keychain "$KEYCHAIN" in codesign where possible.

Signing strategy per binary B:
- For each existing B in [dist/aifo-coder-macos-arm64, dist/aifo-coder-macos-x86_64]:
  1) Try:
     - codesign $SIGN_FLAGS --keychain "$KEYCHAIN" -s "$SIGN_IDENTITY" "$B"
  2) If that fails:
     - Extract SHA-1 hash:
       - SIG_SHA1="$(security find-certificate -a -c "$SIGN_IDENTITY" -Z 2>/dev/null | awk '/^SHA-1 hash:/{print $3; exit}')"
     - If non-empty:
       - codesign $SIGN_FLAGS --keychain "$KEYCHAIN" -s "$SIG_SHA1" "$B"
  3) If still failing:
     - If APPLE_DEV=0 (non-Apple identity):
       - Print warning and fall back to ad-hoc:
         - codesign $SIGN_FLAGS -s - "$B"
       - This ensures a signature for local testing even if identity is misconfigured.
     - If APPLE_DEV=1:
       - Do NOT fallback to ad-hoc.
       - Error out with a message guiding to fix Developer ID setup.

Verification:
- For each signed B:
  - codesign --verify --strict --verbose=4 "$B"
  - codesign -dv --verbose=4 "$B" (diagnostics).
  - spctl --assess --type exec --verbose=4 "$B" || true
- For APPLE_DEV=1:
  - If codesign verification fails: exit non-zero.
- For APPLE_DEV=0:
  - If codesign verification fails: print a warning and exit non-zero (developer must decide).

Outcome:
- Self-signed/local cert:
  - Binaries get signed (possibly ad-hoc) for internal use.
- Developer ID:
  - Binaries are signed with hardened runtime and ready for notarization.
  - Misconfiguration causes a clear error (no ad-hoc).

2.3.2 release-macos-binaries-zips (platform-independent)
Purpose:
- Create per-arch .zip archives containing signed binaries and documentation.

Inputs:
- dist/aifo-coder-macos-arm64 (optional, but recommended).
- dist/aifo-coder-macos-x86_64 (optional, but recommended).
- README.md, NOTICE, LICENSE (from repo root).

Outputs:
- dist/aifo-coder-macos-arm64.zip  (if arm64 binary present).
- dist/aifo-coder-macos-x86_64.zip (if x86_64 binary present).

Behavior:
- Can run on any platform (though primary use is on macOS after signing).
- Steps per arch:
  - Ensure dist/ exists.
  - For each available binary:
    - Use a staging directory:
      - dist/.zip-stage-arm64 or dist/.zip-stage-x86_64.
    - cp binary into stage with fixed name:
      - aifo-coder-macos-arm64 or aifo-coder-macos-x86_64.
    - cp README.md NOTICE LICENSE into stage directory:
      - If any missing: error out (docs are required).
    - (cd stage && zip -9r ../aifo-coder-macos-<arch>.zip .)
    - rm -rf stage.
- Preconditions:
  - Do not rebuild binaries.
  - Do not require both arches; handle whichever exist.

2.3.3 release-macos-binaries-zips-notarize (Darwin-only, optional)
Purpose:
- Notarize the per-arch zips and staple tickets.

Inputs:
- dist/aifo-coder-macos-arm64.zip  (if built).
- dist/aifo-coder-macos-x86_64.zip (if built).
- SIGN_IDENTITY (ideally Developer ID Application).
- NOTARY_PROFILE (optional; required for actual notarization).

Behavior:
- Guard:
  - If uname -s != Darwin: print a clear message and exit 1.
- Pre-check:
  - If NOTARY_PROFILE is empty/unset:
    - Print “NOTARY_PROFILE unset; skipping notarization/stapling (non-fatal).”
    - Exit 0.
  - If xcrun notarytool is not available:
    - Print “xcrun notarytool not found; skipping notarization/stapling (non-fatal).”
    - Exit 0.
- Zips:
  - For each existing zip (arm64, x86_64):
    - xcrun notarytool submit <zip> --keychain-profile "$NOTARY_PROFILE" --wait
    - On failure: print output and exit non-zero.
- Stapling:
  - For each existing zip:
    - xcrun stapler staple <zip> || true
  - Best-effort raw binary staples:
    - xcrun stapler staple dist/aifo-coder-macos-arm64  || true
    - xcrun stapler staple dist/aifo-coder-macos-x86_64 || true
- Validation:
  - For each zip:
    - xcrun stapler validate <zip> || true

Outcome:
- With Developer ID + NOTARY_PROFILE:
  - Zips notarized and stapled.
- With self-signed/local cert or missing NOTARY_PROFILE:
  - Notarization step becomes a no-op with clear messages.
  - Binaries remain signed but not notarized.

2.3.4 release-macos-binary-signed (Darwin-only aggregate)
Purpose:
- Single local entrypoint for macOS developers to produce signed (and optionally notarized) macOS binary zips.

Behavior:
- Guard:
  - If uname -s != Darwin: print message and exit 1.
- Sequence:
  - make build-launcher        # builds native macOS binary for host arch (arm64 or x86_64).
  - make release-macos-binaries-normalize-local
  - make release-macos-binaries-sign
  - make release-macos-binaries-zips
  - make release-macos-binaries-zips-notarize
- Notes:
  - If developer wants both arches:
    - They can separately build both (e.g. via osxcross on Linux or on macOS with appropriate toolchains) and copy/normalize before signing.
  - Self-signed + no NOTARY_PROFILE:
    - Binaries signed.
    - Zips created.
    - Notarization step prints a skip message.
  - Developer ID + NOTARY_PROFILE:
    - Full signing + notarization pipeline.

-------------------------------------------------------------------------------
Phase 3 – Local developer workflows (purely local)
-------------------------------------------------------------------------------

3.1 Local self-signed workflow (short term)
Goal:
- Produce signed (non-notarized) macOS binaries and zips for internal/local use.

Steps:
1) On macOS, ensure Rust toolchain and local cert:
   - Install Rust via https://rustup.rs
   - Create or reuse a self-signed cert:
     - e.g., SIGN_IDENTITY="Migros AI Foundation Code Signer"
2) Build, normalize, sign, zip:
   - export SIGN_IDENTITY="Migros AI Foundation Code Signer"
   - unset NOTARY_PROFILE
   - make release-macos-binary-signed
3) Verify:
   - codesign --verify --deep --strict --verbose=4 dist/aifo-coder-macos-arm64       # if exists
   - codesign --verify --deep --strict --verbose=4 dist/aifo-coder-macos-x86_64     # if exists
   - spctl --assess --type exec --verbose=4 dist/aifo-coder-macos-arm64  || true
   - spctl --assess --type exec --verbose=4 dist/aifo-coder-macos-x86_64 || true

Result:
- dist/aifo-coder-macos-arm64       (signed, non-notarized, when built)
- dist/aifo-coder-macos-x86_64      (signed, non-notarized, when built)
- dist/aifo-coder-macos-arm64.zip   (signed; non-notarized)
- dist/aifo-coder-macos-x86_64.zip  (signed; non-notarized)

3.2 Local Developer ID workflow (medium/long term)
Goal:
- Produce signed and notarized macOS binary zips suitable for public distribution.

Pre-setup:
- Developer has:
  - Developer ID Application cert in login keychain.
  - notarytool profile configured:
    - xcrun notarytool store-credentials --keychain-profile "AifoNotary" ...

Steps:
1) On macOS:
   - export SIGN_IDENTITY="Developer ID Application: <Org Name> (<TEAMID>)"
   - export NOTARY_PROFILE="AifoNotary"
   - make release-macos-binary-signed
2) Pipeline:
   - build-launcher (host arch).
   - release-macos-binaries-normalize-local.
   - release-macos-binaries-sign (with hardened runtime).
   - release-macos-binaries-zips.
   - release-macos-binaries-zips-notarize.

Result:
- dist/aifo-coder-macos-<arch>       (signed, notarized or at least notarizable).
- dist/aifo-coder-macos-<arch>.zip   (stapled for each available arch).

3.3 Relationship to DMG signing (release-dmg-sign)
- DMG flow (unchanged) remains the primary GUI-friendly path:
  - release-app
  - release-dmg
  - release-dmg-sign
- Binary zips from this spec primarily target:
  - CLI users (curl + unzip + add to PATH).
  - Scripted installation flows.
- Developer can use:
  - make release-dmg-sign SIGN_IDENTITY="Developer ID Application: ..." NOTARY_PROFILE="AifoNotary"
    - for DMG.
  - make release-macos-binary-signed
    - for raw binaries + zips.
- It is valid to reuse the same SIGN_IDENTITY and NOTARY_PROFILE for both.

-------------------------------------------------------------------------------
Phase 4 – Release packaging (manual upload + docs)
-------------------------------------------------------------------------------

4.1 CI vs local responsibilities
- CI:
  - Builds Linux binary and tarball.
  - Builds macOS cross binaries (unsigned) and produces macOS tar.gz.
  - No signing or notarization.
- Local macOS developer:
  - Builds native macOS binaries (and/or reuses CI artifacts).
  - Runs local signing/notarization Make targets.
  - Manually attaches signed/notarized artifacts to GitLab releases.

4.2 Recommended macOS release assets (manual upload)
After running make release-macos-binary-signed with Developer ID + NOTARY_PROFILE, developer should upload:

- macOS:
  - aifo-coder-macos-arm64.zip    (signed; notarized/stapled when Apple Dev ID used)
  - aifo-coder-macos-x86_64.zip   (same, if produced)
  - Optionally:
    - aifo-coder-macos-arm64      (signed raw binary)
    - aifo-coder-macos-x86_64     (signed raw binary)
  - DMG from release-dmg-sign     (recommended for GUI users)
- Linux:
  - Existing CI artifacts (unchanged).

4.3 Documentation updates (follow-up)
- README / INSTALL should be updated to state:
  - CI macOS tarballs and binaries are unsigned/unnotarized and may require Gatekeeper overrides.
  - Official macOS signed/notarized binaries are produced locally with Make targets and attached manually.
  - CLI users: prefer signed/notarized per-arch zips.
  - GUI users: prefer signed/notarized DMG.

-------------------------------------------------------------------------------
Phase 5 – Consistency checks and corner cases
-------------------------------------------------------------------------------

5.1 Self-signed vs Developer ID behavior
- Self-signed / non-Apple cert:
  - sign-macos-binaries:
    - Uses basic flags (no hardened runtime).
    - May fall back to ad-hoc if identity fails.
  - notarize-macos-binary-zips:
    - Typically skipped (NOTARY_PROFILE unset; Apple will reject self-signed anyway).
- Developer ID Application:
  - sign-macos-binaries:
    - Uses hardened runtime + timestamp.
    - Does NOT fallback to ad-hoc; failure is fatal.
  - notarize-macos-binary-zips:
    - Performs full submit/wait/staple when NOTARY_PROFILE set and tools installed.

5.2 Stapling raw binaries vs zips
- Preferred stapling targets:
  - Zips, DMGs, app bundles.
- Raw Mach-O binaries:
  - May not accept staples; treat failures as non-fatal.
- Policy:
  - Always attempt to staple zips.
  - Attempt to staple binaries but ignore failure (log warning).
  - Recommend that users install from zips or DMG for best offline Gatekeeper behavior.

5.3 Architecture strategy
- This spec standardizes on per-arch artifacts:
  - dist/aifo-coder-macos-arm64[.zip]
  - dist/aifo-coder-macos-x86_64[.zip]
- Optional future extension:
  - Provide a universal binary via lipo and sign/notarize that single binary.
  - If implemented, ensure Makefile allows choosing per-arch vs universal to avoid confusion.

5.4 Error handling and developer feedback
- All targets must:
  - Print clear messages upon:
    - Missing binaries.
    - Missing certs.
    - Missing tools (codesign/notarytool/stapler).
  - Exit with non-zero status when they cannot safely complete.
- Specifically:
  - sign-macos-binaries with APPLE_DEV=1 and failure must:
    - Explain that a Developer ID cert is required and suggest inspecting identities with:
      - security find-identity -p codesigning -v
  - notarize-macos-binary-zips should clearly distinguish:
    - “NOTARY_PROFILE unset”.
    - “notarytool not installed”.
    - Real notarization failures.

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
  - login keychain is unlocked:
    - security default-keychain -d user
    - security unlock-keychain -p "<password>" login.keychain-db
  - SIGN_IDENTITY matches an existing certificate CN:
    - security find-certificate -a -c "$SIGN_IDENTITY" -Z
  - For Developer ID:
    - security find-identity -p codesigning -v
    - Confirm identity “Developer ID Application: ... (TEAMID)”.

6.2 Common issues
- “code object is not signed at all”:
  - Run make release-macos-binaries-sign and verify with codesign.
- “codesign: invalid signature” or “resource fork, Finder info, or similar detritus not allowed”:
  - Apply xattr -cr before signing and zipping.
- “notarytool: error ... invalid signature”:
  - Likely using a self-signed or ad-hoc signature; notarization requires Developer ID.
- “notarytool: authentication failure”:
  - Check NOTARY_PROFILE; run xcrun notarytool history --keychain-profile "<profile>".
- “Operation not permitted” during codesign:
  - Keychain locked or privacy prompts not accepted; unlock keychain and allow access.

6.3 Local regression testing
- On macOS:
  - make build-launcher
  - make release-macos-binaries-normalize-local
  - make release-macos-binaries-sign
  - make release-macos-binaries-zips
  - Optionally: make release-macos-binaries-zips-notarize
- Validate:
  - codesign --verify --deep --strict --verbose=4 dist/aifo-coder-macos-*
  - xcrun stapler validate dist/aifo-coder-macos-*.zip || true
  - xcrun notarytool history --keychain-profile "$NOTARY_PROFILE" (if used).

Outcome
- With v5 implemented:
  - We reuse existing build targets (build-launcher, build-launcher-macos-cross-*) without duplication.
  - macOS developers can:
    - Normalize existing binaries into dist/.
    - Sign and optionally notarize per-arch binaries and zips.
  - CI remains untouched for signing; macOS artifacts for public distribution are created manually on a Mac using Makefile targets and then uploaded to releases.

````
