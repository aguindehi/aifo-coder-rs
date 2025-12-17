# aifo-coder implement DMG notarization (v1)

## Context

We currently ship macOS CLI artifacts as `.zip` files containing a signed executable. Even though `codesign` validates,
Gatekeeper rejects the downloaded binary with:

- `spctl --assess --type exec ...` → `rejected` / `source=Unnotarized Developer ID`

We attempted to staple a notarization ticket to the raw executable, but `stapler` refuses:

- `xcrun stapler staple <binary>` → `Stapler is incapable of working with Document files.`

This indicates we cannot rely on stapled tickets for bare CLI binaries, so zip distribution is not a reliable "no warnings"
path (especially for offline machines or networks that block Apple ticket fetch).

Apple-supported distribution artifacts that Gatekeeper handles reliably (stapling + offline validation) include:
- `.dmg`
- `.pkg`
- `.app` bundles

We will implement **per-arch notarized DMGs** for the CLI binary.

## Goals

- Provide a macOS distribution format for the CLI that starts on macOS without Gatekeeper malware warnings.
- Replace the per-arch macOS “signed zip” artifacts in the local release flow with per-arch **signed + notarized + stapled
  DMG** artifacts.
- DMG filename is versioned; binary inside DMG is stable (unversioned) and named `aifo-coder`.
- Integrate with existing Makefile helpers and conventions (signing macros, notarization profile).
- Add local (Darwin-only) verification gates since there are no macOS runners.

## Non-goals (v1)

- Building a `.pkg` installer (explicitly deferred to a later iteration).
- Modifying the existing `.app` DMG flow (keep it intact).
- Guaranteeing that a copied-out executable has a stapled ticket (not possible with stapler limitations on raw binaries).
  The intent is that a notarized DMG provides a Gatekeeper-accepted origin and prevents the "malware" dialog at open/run.

## Key constraints / decisions

### 1) One DMG per arch

We ship:
- `dist/aifo-coder-${VERSION}-macos-arm64.dmg`
- `dist/aifo-coder-${VERSION}-macos-x86_64.dmg`

Rationale: matches existing per-arch zip approach, avoids universal binary complexities, and keeps notarization simple.

### 2) Version the DMG, not the binary inside

Inside each DMG:
- `aifo-coder` (executable)
- ancillary files (same set as existing zip distribution, if present):
  - `README.md` (or existing README filename used today)
  - `LICENSE`, `NOTICE` (if shipped today)
  - `docs/` (optional; align with current release content)

Rationale: stable binary name simplifies installation instructions and scripts.

### 3) Notarize and staple the DMG (not the binary)

- Notarize the DMG with `xcrun notarytool submit ... --wait`
- Staple the ticket to the DMG with `xcrun stapler staple <dmg>`
- Validate with `xcrun stapler validate <dmg>` and `spctl --assess --type open --verbose=4 <dmg>`

Rationale: `stapler` rejects raw executable files as “Document”; DMG is a first-class staplable container.

### 4) Hardened runtime + timestamp for the binary

- Continue to sign the CLI binaries using the existing Makefile signing helper that enables:
  - `--options runtime`
  - `--timestamp`
  - Developer ID Application identity

This remains a prerequisite for successful notarization of the DMG contents.

## Proposed Makefile integration (high level)

### New variables (names are indicative; align with existing naming patterns)

- `MACOS_DMG_VERSION` (or reuse existing `MACOS_ZIP_VERSION` if it already provides the correct version string)
- `MACOS_CLI_DMG_ARM64 = dist/aifo-coder-$(MACOS_DMG_VERSION)-macos-arm64.dmg`
- `MACOS_CLI_DMG_X86_64 = dist/aifo-coder-$(MACOS_DMG_VERSION)-macos-x86_64.dmg`

Stage directories:
- `dist/.dmg-cli-root-arm64/`
- `dist/.dmg-cli-root-x86_64/`

### New targets (Darwin-only)

1) `release-macos-cli-dmg`
   - Pre-req: normalized binaries exist
     - `dist/aifo-coder-macos-arm64`
     - `dist/aifo-coder-macos-x86_64`
   - Creates stage dir per arch:
     - copies `dist/aifo-coder-macos-${arch}` to stage as `aifo-coder`
     - copies release docs (README/LICENSE/NOTICE/...) into stage
   - Creates DMG per arch using `hdiutil create`:
     - create an intermediate UDRW dmg from folder
     - convert to UDZO (compressed) output

2) `release-macos-cli-dmg-sign`
   - Signs each DMG using existing `MACOS_SIGN_ONE_BINARY` helper.
   - Must keep `MACOS_REQUIRE_DARWIN` guard.

3) `release-macos-cli-dmg-notarize`
   - Requires `NOTARY_PROFILE` to be set (production requirement).
   - Runs:
     - `xcrun notarytool submit "$(DMG)" --keychain-profile "$(NOTARY_PROFILE)" --wait`
     - `xcrun stapler staple "$(DMG)"`
   - Avoid best-effort behavior for production: a failed notarization must fail the target.

4) `release-macos-cli-dmg-verify`
   - Runs release gates:
     - `xcrun stapler validate "$(DMG)"`
     - `spctl --assess --type open --verbose=4 "$(DMG)"`

5) Aggregate: `release-macos-cli-dmg-signed`
   - Recommended order:
     1. build / normalize binaries (existing targets)
     2. sign binaries (existing target for macos binaries)
     3. `release-macos-cli-dmg`
     4. `release-macos-cli-dmg-sign`
     5. `release-macos-cli-dmg-notarize`
     6. `release-macos-cli-dmg-verify`

### Replace zip in the default macOS signed release flow

- If there is an existing aggregate target like `release-macos-binary-signed`, it currently produces notarized zips.
- In v1 we will:
  - Either replace that target’s zip steps with DMG steps, or
  - Introduce a new aggregate `release-macos-cli-dmg-signed` and adjust the publish/upload target to use DMGs instead of
    zips (preferred for clarity).

We should keep the legacy zip targets for one migration cycle but remove them from the default publish path.

## Verification & validation

### Automated (Darwin-only) validation gates in Makefile

For each DMG:
- `codesign --verify --strict --verbose=4 <dmg>`
- `xcrun stapler validate <dmg>`
- `spctl --assess --type open --verbose=4 <dmg>`

Note: `spctl --type exec` on the extracted `aifo-coder` may still show quarantine-driven behavior; the DMG checks are the
primary offline-verifiable acceptance mechanism.

### Manual release checklist (since no macOS runners)

On a macOS machine after producing the DMGs:

1) Ensure DMG is stapled and accepted:
- `xcrun stapler validate dist/aifo-coder-${VERSION}-macos-arm64.dmg`
- `spctl --assess --type open --verbose=4 dist/aifo-coder-${VERSION}-macos-arm64.dmg`

2) Simulate real user download path:
- Copy DMG to `~/Downloads` (or download it) to ensure quarantine is present.
- Open DMG via Finder.
- Copy `aifo-coder` out to a writable location (e.g., `~/bin`).
- Run `./aifo-coder --version` and confirm no malware dialog appears.

### Linux/CI tests (no macOS)

We can add a lightweight check that does not require macOS tools:
- A unit/integration test (or a `make` target check) that asserts the Makefile contains the DMG targets and that their
  outputs match expected naming patterns.
- If the repository uses Rust tests, prefer adding a small test that checks a parsed “release artifact names” function if
  such a function exists. If not, keep this as a Makefile self-check target.

This is optional in v1; the primary correctness gates must run on macOS due to Apple tooling.

## Risks and mitigations

- Risk: A notarized DMG does not prevent *all* possible warnings when users copy out a raw binary.
  - Mitigation: Provide recommended install instructions and validate the common Finder workflow. DMG notarization is the
    standard accepted approach for distributing non-AppStore software.

- Risk: Notarization is skipped accidentally (e.g., NOTARY_PROFILE missing).
  - Mitigation: Make `release-macos-cli-dmg-notarize` fail hard if NOTARY_PROFILE is empty, and make the aggregate release
    target depend on it for production.

- Risk: Ancillary file list diverges from existing zip distribution.
  - Mitigation: Reuse the same file set used by zip packaging (single source of truth variable or helper).

## Migration path (from zip distribution)

### Previous approach
- Produce `...-signed.zip` per arch.
- Notarize zip and attempt to staple (zip and/or binary).
- Users unzip and run binary → Gatekeeper rejects as unnotarized.

### New approach (v1)
- Produce `...macos-${arch}.dmg` per arch (versioned DMG).
- DMG contains `aifo-coder` (unversioned filename).
- Sign + notarize + staple DMG.
- Publish DMGs in place of zips for macOS downloads.

### Backward compatibility
- Keep zip targets for one release cycle for users who prefer them, but mark as legacy/not recommended.
- After one stable release, remove zip notarization steps to reduce maintenance burden.

## Condensed phased implementation plan (≤ 9 phases)

Phase 1: Specification + naming decisions
- Finalize DMG filenames and which auxiliary files are included.
- Confirm whether to keep a `-signed` suffix in DMG filenames (recommended: omit; notarized implies signed).

Phase 2: DMG build (CLI) targets
- Add Makefile targets to stage per-arch directories and build per-arch DMGs.
- Ensure internal binary name is `aifo-coder`.

Phase 3: DMG signing + notarization
- Add targets to sign DMGs.
- Add targets to notarize and staple DMGs.
- Fail hard when `NOTARY_PROFILE` is missing in production target chain.

Phase 4: Verification gates
- Add Darwin-only validation target that runs `codesign --verify`, `stapler validate`, and `spctl --type open`.

Phase 5: Wire into release/publish flow
- Switch the default macOS “signed release artifacts” from zip to dmg.
- Keep legacy zip targets available but not part of default publish.

Phase 6: Documentation + operator checklist
- Update release docs/README to recommend DMG download and copy-to-path install.
- Add the local verification checklist for release operators.

## Notes on “tests” and local workflows

- Because Apple notarization tooling requires macOS, we treat notarization verification as a Darwin-only build gate, not as
  a CI test.
- Where feasible, add a Makefile “self-check” target that fails if required variables/targets are missing, to protect
  against accidental removal in future refactors.
