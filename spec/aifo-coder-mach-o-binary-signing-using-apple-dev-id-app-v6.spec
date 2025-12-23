AIFO Coder: macOS Mach-O binary signing (local-only, Apple Dev ID ready) – v6, phased

Status
- This v6 spec supersedes v5 and clarifies:
  - Exact Makefile variable expectations and target names.
  - Darwin vs non-Darwin behavior and failure modes.
  - Idempotency and re-runnability of targets.
  - Required preconditions and invariants for each phase.
- It remains strictly local-only for signing/notarization; CI behavior is unchanged.

-------------------------------------------------------------------------------
Phase 0 – Preconditions, artifacts, naming, and invariants
-------------------------------------------------------------------------------

0.1 Scope recap (unchanged intent)
- Provide local-only Makefile targets to:
  - Normalize already-built macOS binaries into canonical dist/ paths.
  - Sign those binaries with:
    - A currently configured local certificate (self-signed or enterprise) and/or
    - An Apple “Developer ID Application” certificate when available.
  - Optionally notarize and staple signed binary zips using notarytool when Apple credentials exist.
- Outputs:
  - Per-arch signed binaries and zips, suitable for direct download and safe execution under Gatekeeper.
- Non-goals:
  - No signing or notarization in CI.
  - No changes to osxcross builder or CI macOS build jobs.
  - No runtime behavior changes in the Rust binary.
  - No DMG flow changes (release-app / release-dmg / release-dmg-sign remain as-is).

0.2 Canonical artifacts and paths
- We standardize on the following signing inputs in dist/:
  - dist/aifo-coder-macos-arm64
  - dist/aifo-coder-macos-x86_64
- Their corresponding zip artifacts:
  - dist/aifo-coder-macos-arm64.zip
  - dist/aifo-coder-macos-x86_64.zip
- Source binaries (produced by existing targets, not by this spec):
  - target/aarch64-apple-darwin/release/aifo-coder
  - target/x86_64-apple-darwin/release/aifo-coder

Invariants:
- Targets in this spec MUST NOT invoke cargo build directly.
- Targets in this spec MUST NOT change the Rust code or its configuration.
- All artifacts in dist/ MUST be treated as replaceable; targets are free to overwrite them.

0.3 Existing Makefile targets (referenced, not modified)
- build-launcher:
  - On macOS (Darwin) uses host rustup/cargo and builds:
    - target/aarch64-apple-darwin/release/aifo-coder (when Apple Silicon toolchain available).
    - target/x86_64-apple-darwin/release/aifo-coder (when Intel toolchain available).
- build-launcher-macos-cross, build-launcher-macos-cross-arm64, build-launcher-macos-cross-x86_64:
  - Run on Linux via osxcross; produce the same target/.../aifo-coder binaries.
- release-app / release-dmg / release-dmg-sign:
  - Existing DMG pipeline; unchanged by this spec.

0.4 Makefile variable semantics (MUST)
The following variables are assumed to exist or be added to the Makefile:

- DIST_DIR ?= dist
  - All normalized binaries and zips live under this directory.
- BIN_NAME ?= aifo-coder
  - Base binary name inside target/; used to derive paths.
- APP_NAME, APP_BUNDLE_ID, DMG_NAME, DMG_BG:
  - Used by DMG flow; referenced but unchanged.
- SIGN_IDENTITY (no default; MUST be provided by user for meaningful signing):
  - Common Name (CN) of the certificate for codesign.
  - Examples:
    - "AI Foundation Code Signer" (self-signed / internal).
    - "Developer ID Application: Migros AI Foundation (TEAMID)" (Apple Dev ID).
- NOTARY_PROFILE (optional; empty by default):
  - A keychain profile name configured for xcrun notarytool.
  - When empty or unset, notarization targets MUST be a no-op with clear logging and exit 0.

Required behavior:
- If SIGN_IDENTITY is unset/empty:
  - release-macos-binaries-sign MUST still run and attempt ad-hoc signing when necessary, but:
    - It MUST log clearly that no identity was provided.
    - It MUST treat Developer ID–style validation paths as unavailable.
- If NOTARY_PROFILE is unset/empty:
  - release-macos-binaries-zips-notarize MUST:
    - Print an informative message.
    - Exit successfully (status 0) without attempting notarization.

0.5 Platform constraints and detection
- All codesign/notarytool/stapler operations are macOS-only (Darwin).
- All targets that rely on those tools MUST:
  - Detect uname -s.
  - If uname -s != Darwin:
    - Print an explicit message:
      - "This target requires macOS (Darwin); current platform: <uname>"
    - Exit with non-zero status (1).
- Non-signing/non-notarization targets (like zipping) MAY run on any platform:
  - But they MUST NOT expect macOS tools or keychains.
  - They MUST still depend on normalized dist/ binaries.

0.6 Idempotency and re-runnability
All targets introduced or defined in this spec MUST be safe to re-run:

- normalize target:
  - MAY overwrite dist/ binaries every time.
- sign target:
  - MUST be safe when binaries are already signed (codesign --force).
  - MUST re-sign in place, overwriting the previous signature.
- zip target:
  - MUST overwrite .zip artifacts each run.
- notarize target:
  - Re-submitting already notarized zips is allowed (Apple handles duplicates).
  - stapler operations MUST be idempotent and treat failures gracefully.

-------------------------------------------------------------------------------
Phase 1 – Certificate strategy and classification (local only)
-------------------------------------------------------------------------------

1.1 Certificate types and classification
We distinguish two broad categories:

- Apple Developer identities (APPLE_DEV=1):
  - Subject typically includes one of:
    - "Developer ID Application"
    - "Apple Distribution"
    - "Apple Development"
  - Suitable for hardened runtime, timestamps, and notarization.
- Non-Apple identities or self-signed (APPLE_DEV=0):
  - Any certificate that does NOT match the above subject patterns.
  - Usable for internal testing only.
  - Not accepted by Apple notarization.

Detection (MUST be implemented in Makefile shell):
- Given SIGN_IDENTITY:
  - Run:
    - security find-certificate -a -c "$SIGN_IDENTITY" -Z -p 2>/dev/null
  - If output contains one of:
    - "Developer ID Application"
    - "Apple Distribution"
    - "Apple Development"
  - Then set:
    - APPLE_DEV=1
  - Else:
    - APPLE_DEV=0

1.2 Self-signed / non-Apple identities (APPLE_DEV=0)
Behavior:
- Signing:
  - MUST use:
    - SIGN_FLAGS="--force --verbose=4"
  - May omit hardened runtime and timestamp options.
- Notarization:
  - release-macos-binaries-zips-notarize SHOULD detect APPLE_DEV=0 and:
    - Print that Developer ID is required for notarization.
    - Exit 0 without calling notarytool, even if NOTARY_PROFILE is set.
- Gatekeeper behavior:
  - Binaries may still trigger warnings on machines that do not trust the cert.
  - This is acceptable for internal or developer-only flows.

1.3 Apple Developer identities (APPLE_DEV=1)
Behavior:
- Signing:
  - MUST use hardened runtime and timestamp:
    - SIGN_FLAGS="--force --timestamp --options runtime --verbose=4"
- Notarization:
  - Allowed when NOTARY_PROFILE is set and notarytool is installed.
- Failure mode for Apple Developer identities:
  - We DO NOT fall back to ad-hoc signing when SIGN_IDENTITY appears to be Apple Developer:
    - Misconfiguration MUST be treated as a hard error.
    - The user MUST be guided to inspect identities with:
      - security find-identity -p codesigning -v

1.4 SIGN_IDENTITY resolution and fallback
Given SIGN_IDENTITY (string):

- Primary codesign attempt:
  - Use:
    - codesign $SIGN_FLAGS --keychain "$KEYCHAIN" -s "$SIGN_IDENTITY" "$BINARY"
- Fallback to SHA-1 hash:
  - If primary attempt fails:
    - Extract SHA-1:
      - SIG_SHA1="$(security find-certificate -a -c "$SIGN_IDENTITY" -Z 2>/dev/null | awk '/^SHA-1 hash:/{print $3; exit}')"
    - If SIG_SHA1 is non-empty:
      - Retry:
        - codesign $SIGN_FLAGS --keychain "$KEYCHAIN" -s "$SIG_SHA1" "$BINARY"
- Ad-hoc fallback (APPLE_DEV=0 only):
  - If both above attempts fail AND APPLE_DEV=0:
    - Log a warning explaining:
      - The named cert could not be used.
      - Falling back to ad-hoc signing ("-s -") for local testing.
    - Run:
      - codesign $SIGN_FLAGS -s - "$BINARY"
  - If APPLE_DEV=1:
    - MUST NOT fallback to ad-hoc.
    - MUST exit non-zero with a clear error message (see 2.3.1).

1.5 Keychain expectations
- Assume default user keychain:
  - KEYCHAIN="$(security default-keychain -d user | tr -d ' \"')"
- All security and codesign invocations SHOULD specify:
  - --keychain "$KEYCHAIN"
- Developers MUST ensure:
  - login keychain is unlocked.
  - Certificates and private keys exist for SIGN_IDENTITY.

-------------------------------------------------------------------------------
Phase 2 – Normalize existing macOS binaries into dist/
-------------------------------------------------------------------------------

2.1 New Make target: release-macos-binaries-normalize-local (Darwin-only)
Goal:
- Normalize existing macOS binaries built by other targets into canonical dist/ paths.

Target name:
- release-macos-binaries-normalize-local

Platform guard:
- MUST:
  - if [ "$$(uname -s)" != "Darwin" ]; then \
        echo "release-macos-binaries-normalize-local requires macOS (Darwin), found $$(uname -s)" >&2; \
        exit 1; \
    fi

Inputs:
- Optional:
  - target/aarch64-apple-darwin/release/aifo-coder
  - target/x86_64-apple-darwin/release/aifo-coder
- These MUST NOT be built by this target; they MUST already exist from:
  - make build-launcher
  - or other build-launcher-macos-* targets.

Outputs:
- dist/aifo-coder-macos-arm64 (if arm64 source exists)
- dist/aifo-coder-macos-x86_64 (if x86_64 source exists)

Behavior (MUST):
- Ensure DIST_DIR exists:
  - mkdir -p "$${DIST_DIR:-dist}"
- For each arch:
  - If source binary exists:
    - cp "target/aarch64-apple-darwin/release/aifo-coder" "dist/aifo-coder-macos-arm64"
    - cp "target/x86_64-apple-darwin/release/aifo-coder" "dist/aifo-coder-macos-x86_64"
    - Optionally (SHOULD):
      - Validate Mach-O type via file(1):
        - file "dist/aifo-coder-macos-arm64" | grep -qi 'Mach-O 64-bit.*arm64'
        - file "dist/aifo-coder-macos-x86_64" | grep -qi 'Mach-O 64-bit.*x86_64'
      - If validation fails:
        - Print a warning and exit non-zero.
  - If source binary does NOT exist:
    - Print a non-fatal message, e.g.:
      - "No target/aarch64-apple-darwin/release/aifo-coder found; skipping arm64."
      - "No target/x86_64-apple-darwin/release/aifo-coder found; skipping x86_64."
- If both architectures are missing:
  - MUST exit non-zero with an explicit error, e.g.:
    - "No macOS binaries found to normalize; run 'make build-launcher' first."

Idempotency:
- Overwrite dist/ binaries unconditionally.

-------------------------------------------------------------------------------
Phase 3 – Signing and zipping macOS binaries
-------------------------------------------------------------------------------

3.1 New Make target: release-macos-binaries-sign (Darwin-only)
Purpose:
- Sign dist/ binaries in place using SIGN_IDENTITY or ad-hoc as per 1.4.

Target name:
- release-macos-binaries-sign

Platform guard:
- Same pattern as 2.1:
  - Fail with exit 1 if uname -s != Darwin.

Inputs:
- At least one of:
  - dist/aifo-coder-macos-arm64
  - dist/aifo-coder-macos-x86_64
- These MUST be produced by:
  - release-macos-binaries-normalize-local
  - or a CI/other workflow that copies binaries into dist/.

Pre-checks (MUST):
- If both dist/aifo-coder-macos-arm64 and dist/aifo-coder-macos-x86_64 are missing:
  - Print a helpful message:
    - "No dist/aifo-coder-macos-* binaries to sign."
    - "Hint: run 'make build-launcher' and 'make release-macos-binaries-normalize-local' first."
  - Exit non-zero.

Extended attributes cleanup (SHOULD):
- For each existing binary B:
  - xattr -cr "$$B" 2>/dev/null || true

SIGN_IDENTITY presence:
- If SIGN_IDENTITY is empty/unset:
  - APPLE_DEV MUST be treated as 0.
  - Log:
    - "SIGN_IDENTITY not set; attempting ad-hoc signing for local use."
  - Skip security-based APPLE_DEV detection and directly sign ad-hoc (no certificate lookups).
  - This path MUST NOT be used for notarization (notarytool will fail later; see 3.3).

APPLE_DEV detection:
- When SIGN_IDENTITY is non-empty:
  - Determine APPLE_DEV as described in 1.1.
  - Log:
    - "Detected Apple Developer identity" or
    - "Using non-Apple/local identity".

Signing flags:
- If APPLE_DEV=1:
  - SIGN_FLAGS="--force --timestamp --options runtime --verbose=4"
- If APPLE_DEV=0:
  - SIGN_FLAGS="--force --verbose=4"

Keychain resolution:
- KEYCHAIN="$$(security default-keychain -d user | tr -d '\'' \"'\'')"
- MUST be used with security and codesign where applicable.

Signing algorithm per binary B (MUST):
1) If SIGN_IDENTITY is empty:
   - codesign $$SIGN_FLAGS -s - "$$B"
2) Else (SIGN_IDENTITY set):
   a) Try:
      - codesign $$SIGN_FLAGS --keychain "$$KEYCHAIN" -s "$$SIGN_IDENTITY" "$$B"
   b) If (a) fails:
      - SIG_SHA1="$$(security find-certificate -a -c "$$SIGN_IDENTITY" -Z 2>/dev/null | awk '\''/^SHA-1 hash:/{print $$3; exit}'\'')"
      - If $$SIG_SHA1 non-empty:
        - codesign $$SIGN_FLAGS --keychain "$$KEYCHAIN" -s "$$SIG_SHA1" "$$B"
   c) If both (a) and (b) fail:
      - If APPLE_DEV=0:
        - Log warning and fall back to ad-hoc:
          - codesign $$SIGN_FLAGS -s - "$$B"
      - If APPLE_DEV=1:
        - Print detailed error including:
          - The failing SIGN_IDENTITY.
          - A hint to run:
            - security find-identity -p codesigning -v
        - Exit non-zero.

Post-sign verification (MUST):
- For each binary B:
  - codesign --verify --strict --verbose=4 "$$B"
  - codesign -dv --verbose=4 "$$B" || true   # diagnostics; failure here is non-fatal by itself.
  - spctl --assess --type exec --verbose=4 "$$B" || true
- For APPLE_DEV=1:
  - If codesign --verify fails:
    - Exit non-zero.
- For APPLE_DEV=0:
  - If codesign --verify fails:
    - Log a warning and exit non-zero (developer must decide whether to ignore).

Idempotency:
- codesign --force ensures re-signing is safe.

3.2 New Make target: release-macos-binaries-zips (platform-independent)
Purpose:
- Create per-arch .zip archives with signed binaries and required docs.
- Does NOT depend on macOS-specific tools.

Target name:
- release-macos-binaries-zips

Inputs:
- Optional:
  - dist/aifo-coder-macos-arm64
  - dist/aifo-coder-macos-x86_64
- Required docs:
  - README.md (repo root)
  - NOTICE (repo root)
  - LICENSE (repo root)

Outputs:
- dist/aifo-coder-macos-arm64.zip (if arm64 binary present)
- dist/aifo-coder-macos-x86_64.zip (if x86_64 binary present)

Behavior (MUST):
- Ensure DIST_DIR exists:
  - mkdir -p "$${DIST_DIR:-dist}"
- For each arch <arch> in {arm64, x86_64}:
  - BINARY="dist/aifo-coder-macos-<arch>"
  - If BINARY exists:
    - Validate docs (MUST):
      - If any of README.md, NOTICE, LICENSE is missing:
        - Print a clear error and exit non-zero.
    - Prepare staging directory:
      - STAGE="dist/.zip-stage-<arch>"
      - rm -rf "$$STAGE"
      - mkdir -p "$$STAGE"
    - Copy artifacts into stage:
      - cp "$$BINARY" "$$STAGE/aifo-coder-macos-<arch>"
      - cp README.md NOTICE LICENSE "$$STAGE/"
    - Create zip from within stage:
      - (cd "$$STAGE" && zip -9r "../aifo-coder-macos-<arch>.zip" .)
    - Clean up:
      - rm -rf "$$STAGE"
  - If BINARY does not exist:
    - Print a non-fatal message:
      - "dist/aifo-coder-macos-<arch> missing; skipping zip for <arch>."
- If neither arm64 nor x86_64 BINARY exists:
  - MUST exit non-zero with friendly guidance:
    - "No macOS binaries in dist/ to zip; run normalization and signing first."

Idempotency:
- Existing .zip files MUST be overwritten.

Note:
- This target should be callable on Linux or macOS; it MUST NOT invoke codesign or notarytool.

3.3 New Make target: release-macos-binaries-zips-notarize (Darwin-only)
Purpose:
- Notarize per-arch zips using Apple notarytool and staple tickets.

Target name:
- release-macos-binaries-zips-notarize

Platform guard:
- MUST require Darwin; exit 1 otherwise.

Inputs:
- Optional (but at least one strongly expected):
  - dist/aifo-coder-macos-arm64.zip
  - dist/aifo-coder-macos-x86_64.zip
- NOTARY_PROFILE:
  - Name of notarytool keychain profile.
- APPLE_DEV detection:
  - Same as in 3.1; MUST verify APPLE_DEV=1 before notarization.

Pre-checks (MUST):
- If NOTARY_PROFILE is empty/unset:
  - Print:
    - "NOTARY_PROFILE unset; skipping macOS notarization and stapling."
  - Exit 0.
- If APPLE_DEV=0:
  - Print:
    - "SIGN_IDENTITY is not a Developer ID identity; notarization requires Developer ID. Skipping."
  - Exit 0.
- If xcrun notarytool is not available (non-zero exit for xcrun notarytool --help):
  - Print:
    - "xcrun notarytool not found; skipping notarization/stapling."
  - Exit 0.

Zips processing (MUST):
- For each zip Z in:
  - dist/aifo-coder-macos-arm64.zip
  - dist/aifo-coder-macos-x86_64.zip
- If Z exists:
  - Submit and wait:
    - xcrun notarytool submit "$$Z" --keychain-profile "$$NOTARY_PROFILE" --wait
  - If submission fails:
    - Print notarytool output.
    - Exit non-zero.

Stapling (SHOULD):
- After successful notarization submissions:
  - For each zip Z that exists:
    - xcrun stapler staple "$$Z" || true
  - Best-effort raw binary stapling (non-fatal):
    - xcrun stapler staple dist/aifo-coder-macos-arm64  || true
    - xcrun stapler staple dist/aifo-coder-macos-x86_64 || true

Validation (SHOULD):
- For each zip Z:
  - xcrun stapler validate "$$Z" || true

Behavior when no zips exist:
- If neither dist/aifo-coder-macos-arm64.zip nor dist/aifo-coder-macos-x86_64.zip exists:
  - Print:
    - "No macOS binary zips found in dist/ to notarize."
  - Exit non-zero.

3.4 Aggregate target: release-macos-binary-signed (Darwin-only)
Purpose:
- Provide a single high-level entrypoint for macOS developers:
  - Build + normalize + sign + zip + optional notarization.

Target name:
- release-macos-binary-signed

Platform guard:
- MUST require Darwin; exit 1 otherwise.

Behavior (MUST):
- Sequentially invoke:
  - make build-launcher
  - make release-macos-binaries-normalize-local
  - make release-macos-binaries-sign
  - make release-macos-binaries-zips
  - make release-macos-binaries-zips-notarize
- This target MUST propagate failures:
  - If any of the invoked targets exits non-zero:
    - The aggregate MUST stop and return that exit code.

Notes:
- This target intentionally builds using build-launcher for host arch only.
- To produce both architectures:
  - Developers MAY:
    - Use osxcross or separate toolchains.
    - Then rerun normalize + sign + zip + notarize.

-------------------------------------------------------------------------------
Phase 4 – CI and local workflows
-------------------------------------------------------------------------------

4.1 CI behavior (unchanged)
- CI responsibilities:
  - Build Linux binary and tarball.
  - Build macOS cross binaries using osxcross and produce tarballs.
  - CI MUST NOT:
    - Run codesign.
    - Run notarytool or stapler.
    - Depend on SIGN_IDENTITY or NOTARY_PROFILE.
- CI MAY:
  - Copy macOS cross-built binaries into:
    - dist/aifo-coder-macos-arm64
    - dist/aifo-coder-macos-x86_64
  - This can simplify local signing workflows but is not required.

4.2 Local developer workflows – self-signed (short term)
Goal:
- Produce signed (non-notarized) binaries and zips for internal use.

Preconditions:
- macOS host with:
  - Rust toolchain installed (via rustup).
  - Self-signed or enterprise code signing cert available.
- Configure:
  - export SIGN_IDENTITY="Migros AI Foundation Code Signer"
  - unset NOTARY_PROFILE

Typical flow:
1) Build and produce normalized binaries:
   - make release-macos-binary-signed
   - This will:
     - make build-launcher
     - normalize
     - sign using SIGN_IDENTITY (APPLE_DEV=0)
     - zip
     - notarization step will skip (NOTARY_PROFILE unset).

2) Verify signatures manually (optional but recommended):
   - codesign --verify --deep --strict --verbose=4 dist/aifo-coder-macos-arm64       # if exists
   - codesign --verify --deep --strict --verbose=4 dist/aifo-coder-macos-x86_64     # if exists
   - spctl --assess --type exec --verbose=4 dist/aifo-coder-macos-arm64  || true
   - spctl --assess --type exec --verbose=4 dist/aifo-coder-macos-x86_64 || true

Result:
- dist/aifo-coder-macos-arm64       (signed, non-notarized; when built)
- dist/aifo-coder-macos-x86_64      (signed, non-notarized; when built)
- dist/aifo-coder-macos-arm64.zip   (signed binary + docs, non-notarized)
- dist/aifo-coder-macos-x86_64.zip  (same)

4.3 Local developer workflows – Developer ID (medium/long term)
Goal:
- Produce signed and notarized macOS binary zips suitable for public distribution.

Pre-setup:
- Developer has:
  - Developer ID Application certificate in login keychain.
  - notarytool profile configured (once):
    - xcrun notarytool store-credentials --keychain-profile "AifoNotary" --team-id "<TEAMID>" --key-id "<KEY_ID>" --issuer "<ISSUER_ID>" --private-key /path/to/AuthKey_XXXXXX.p8

Environment:
- export SIGN_IDENTITY="Developer ID Application: <Org Name> (<TEAMID>)"
- export NOTARY_PROFILE="AifoNotary"

Flow:
1) Run:
   - make release-macos-binary-signed
2) Internally:
   - build-launcher builds host-arch binary.
   - normalize-local populates dist/aifo-coder-macos-<arch>.
   - binaries are signed with hardened runtime and timestamp.
   - zips are built.
   - zips are submitted to notarytool and stapled when successful.

Result:
- dist/aifo-coder-macos-<arch>       (signed with Developer ID).
- dist/aifo-coder-macos-<arch>.zip   (notarized and stapled when Apple Dev ID used).

4.4 Relationship to DMG signing (unchanged)
- DMG flow remains independent:
  - release-app
  - release-dmg
  - release-dmg-sign
- It is valid to reuse the same SIGN_IDENTITY and NOTARY_PROFILE for both:
  - make release-dmg-sign SIGN_IDENTITY="Developer ID Application: ..." NOTARY_PROFILE="AifoNotary"
  - make release-macos-binary-signed

-------------------------------------------------------------------------------
Phase 5 – Error handling, consistency checks, and corner cases
-------------------------------------------------------------------------------

5.1 Error handling principles
- All targets MUST:
  - Print clear, human-readable messages on:
    - Missing prerequisites (binaries, docs).
    - Missing tools (codesign, notarytool, stapler, file, zip).
    - Misconfigured certificates or keychains.
  - Return non-zero exit codes on:
    - Missing required inputs (where no reasonable fallback exists).
    - Fatal signing or notarization failures.
- Silent failures are forbidden.

5.2 Specific error scenarios and required behavior
- normalize-local:
  - If neither arm64 nor x86_64 binary exists:
    - Exit 1 with a message to run build-launcher.
- sign:
  - Missing dist binaries:
    - Exit 1 with guidance to run normalize-local.
  - SIGN_IDENTITY not set:
    - Use ad-hoc signing and print a big warning that notarization is not possible.
  - SIGN_IDENTITY set, APPLE_DEV=1, codesign fails even after SHA-1 fallback:
    - Exit 1, instructing developer to run:
      - security find-identity -p codesigning -v
- zips:
  - Missing README.md/NOTICE/LICENSE:
    - Exit 1 with a clear message listing missing files.
  - No binaries to zip:
    - Exit 1 with guidance to run preceding steps.
- notarize:
  - NOTARY_PROFILE unset or APPLE_DEV=0:
    - Print skip reason, exit 0.
  - notarytool missing:
    - Print skip reason, exit 0.
  - no zips present:
    - Exit 1 with guidance to run zipping step.
  - notarytool submit failure:
    - Exit 1 and print tool output.

5.3 Local verification commands (recommended)
On macOS, after running the pipeline, developers SHOULD validate:

- Binaries:
  - codesign --verify --deep --strict --verbose=4 dist/aifo-coder-macos-*
  - spctl --assess --type exec --verbose=4 dist/aifo-coder-macos-* || true
- Zips (if notarized):
  - xcrun stapler validate dist/aifo-coder-macos-*.zip || true
- Notary history:
  - xcrun notarytool history --keychain-profile "$$NOTARY_PROFILE" || true

5.4 Architecture strategy
- This spec standardizes on per-arch artifacts:
  - dist/aifo-coder-macos-arm64[.zip]
  - dist/aifo-coder-macos-x86_64[.zip]
- Future optional extension (NOT in v6 implementation scope):
  - A universal binary via lipo.
  - Additional targets could offer:
    - dist/aifo-coder-macos-universal
    - dist/aifo-coder-macos-universal.zip
  - If added, the Makefile MUST allow choosing per-arch vs universal in a non-confusing way.

5.5 Dead code and testability
- Make targets introduced here MUST be usable in local regression testing:
  - make build-launcher
  - make release-macos-binaries-normalize-local
  - make release-macos-binaries-sign
  - make release-macos-binaries-zips
  - make release-macos-binaries-zips-notarize (with NOTARY_PROFILE when desired)
- There MUST NOT be unused targets; each target is part of at least one recommended workflow.

-------------------------------------------------------------------------------
Phase 6 – Release packaging and documentation
-------------------------------------------------------------------------------

6.1 Recommended macOS release assets
After running make release-macos-binary-signed with Developer ID + NOTARY_PROFILE, the recommended release assets are:

- macOS:
  - aifo-coder-<version>-macos-arm64.zip    (signed; notarized and stapled when Apple Dev ID used)
  - aifo-coder-<version>-macos-x86_64.zip   (same, if produced)
  - Optionally:
    - aifo-coder-macos-arm64      (signed raw binary)
    - aifo-coder-macos-x86_64     (signed raw binary)
  - DMG from release-dmg-sign     (recommended for GUI users)
- Linux:
  - Existing CI artifacts (unchanged).

6.4 Attaching signed macOS zip assets to GitLab Releases (local-only signing)
Goal:
- Keep CI unsigned while still publishing signed/notarized per-arch zip assets as release links.

Approach:
- Upload locally-produced signed zips to the GitLab Generic Package Registry for the tag, then add release links.

Conventions (MUST):
- Signed zip filenames MUST be versioned to avoid collisions:
  - aifo-coder-<version>-macos-arm64.zip
  - aifo-coder-<version>-macos-x86_64.zip
- The Generic Package Registry path SHOULD be:
  - /projects/<id>/packages/generic/<project>/<tag>/<filename>

Workflow:
1) On macOS, produce signed/notarized zips:
   - make release-macos-binary-signed SIGN_IDENTITY="Developer ID Application: ..." NOTARY_PROFILE="<profile>"
2) Rename (or generate) the zips to include the release version/tag:
   - dist/aifo-coder-<version>-macos-*.zip
3) Upload to the Generic Package Registry (token with api scope):
   - curl --header "PRIVATE-TOKEN: <token>" --upload-file dist/aifo-coder-<version>-macos-arm64.zip \
       "<CI_API_V4_URL>/projects/<id>/packages/generic/<project>/<tag>/aifo-coder-<version>-macos-arm64.zip"
   - curl --header "PRIVATE-TOKEN: <token>" --upload-file dist/aifo-coder-<version>-macos-x86_64.zip \
       "<CI_API_V4_URL>/projects/<id>/packages/generic/<project>/<tag>/aifo-coder-<version>-macos-x86_64.zip"
4) In the tag pipeline, run a manual CI job that:
   - Verifies the uploaded zips exist (HEAD request).
   - Adds release asset links pointing to those package registry URLs.

Invariants:
- CI MUST NOT run codesign/notarytool/stapler.
- CI MUST NOT require SIGN_IDENTITY or NOTARY_PROFILE.
- The “attach signed zips” job MUST only link already-uploaded artifacts.

6.2 Documentation expectations
- README / INSTALL (or equivalent user-facing docs) SHOULD be updated (outside this spec’s scope) to state:
  - CI macOS tarballs and binaries are unsigned/unnotarized and may require Gatekeeper overrides.
  - Official macOS signed/notarized binaries are produced locally on macOS using:
    - make release-macos-binary-signed
  - CLI users:
    - Prefer signed/notarized per-arch zips.
  - GUI users:
    - Prefer signed/notarized DMG from release-dmg-sign.

6.3 Outcome
With this v6 spec implemented:

- We reuse existing build targets (build-launcher and macOS cross-builders) without duplication.
- macOS developers can:
  - Normalize existing binaries into dist/.
  - Sign them using local or Developer ID certificates.
  - Package them into per-arch zips with required docs.
  - Optionally notarize and staple them.
- CI remains free of signing credentials and macOS-only tooling.
- The Makefile behavior is:
  - Deterministic.
  - Idempotent where reasonable.
  - Explicit about failure modes and platform constraints.
