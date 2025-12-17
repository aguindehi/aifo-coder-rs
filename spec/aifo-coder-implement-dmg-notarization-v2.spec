# aifo-coder implement notarized macOS CLI DMGs (v2)

## Context

We currently ship the macOS CLI as per-arch “signed zip” artifacts that contain a signed executable. Even when
`codesign` validates, Gatekeeper may still reject or warn on first run with:

- `spctl --assess --type exec ...` → `rejected` / `source=Unnotarized Developer ID`

We also cannot reliably staple a notarization ticket to a bare CLI executable:

- `xcrun stapler staple <binary>` → `Stapler is incapable of working with Document files.`

Therefore, notarizing + stapling the *container* (not the raw binary) is required for a no-warning distribution
experience, especially for offline machines or networks that block Apple ticket fetch.

Apple-supported staplable distribution artifacts include `.dmg`, `.pkg`, and `.app` bundles. For the CLI,
we will implement **per-arch notarized DMGs**.

This v2 spec integrates the DMG flow into the existing Makefile `publish-release` pipeline, replacing the macOS zip
path as the default production artifact while keeping the old zip targets available for one migration cycle.

## Goals

- Provide a macOS distribution format for the CLI that starts on macOS without Gatekeeper malware warnings.
- Replace per-arch macOS “signed zip” artifacts in the default local release/publish flow with per-arch:
  **signed + notarized + stapled DMG** artifacts.
- DMG filename is versioned; binary inside DMG is stable (unversioned) and named `aifo-coder`.
- Integrate with existing Makefile helpers and conventions (signing macros, notarization profile).
- Add local (Darwin-only) verification gates since there are no macOS runners.
- Wire the new `release-macos-cli-dmg-signed` into the Makefile `publish-release` flow.

## Non-goals (v2)

- Building a `.pkg` installer (deferred).
- Modifying the existing `.app` DMG flow (`release-dmg*`) (keep it intact).
- Guaranteeing that a copied-out executable has a stapled ticket (not possible with stapler limitations on raw binaries).
- CI-based notarization (requires macOS); we keep notarization as a local (Darwin-only) gate.

## Issues found in v1 plan and concrete corrections (v2)

1) **Coexistence with existing `.app` DMG flow**
   - The repository already has a macOS `.app` + `.dmg` signing/notarization flow.
   - Correction: introduce *CLI-specific* targets and variables prefixed with `macos-cli-dmg` and do not change existing
     `release-app`, `release-dmg`, `release-dmg-sign`.

2) **Version source and tag semantics**
   - v1 mentions `VERSION` and possibly `MACOS_ZIP_VERSION`, but does not align with `RELEASE_TAG_EFFECTIVE`.
   - Correction:
     - `MACOS_DMG_VERSION ?= $(MACOS_ZIP_VERSION)` so DMG version aligns with the existing zip naming default.
     - Publish flow uses `RELEASE_TAG_EFFECTIVE` to derive the release tag; the DMG version should match the same release
       version string to avoid confusion and collisions.

3) **Naming consistency vs existing zips**
   - v1 omits “-signed” suffix for DMG; this is preferable.
   - Correction: standardize DMG names with no `-signed` suffix:
     - `dist/aifo-coder-<ver>-macos-arm64.dmg`
     - `dist/aifo-coder-<ver>-macos-x86_64.dmg`
   - Keep legacy zips unchanged for one migration cycle (they already include `-signed.zip`).

4) **Release content drift risk**
   - Zip packaging currently requires `README.md`, `NOTICE`, `LICENSE`; v1 plan suggests “same set” but does not enforce
     single source of truth.
   - Correction: define `MACOS_CLI_RELEASE_FILES := README.md NOTICE LICENSE` and reuse it for both zip and DMG staging.
     Keep `docs/` conditional.

5) **Notarization behavior must be “fail hard” in the production chain**
   - Existing zip notarization target is best-effort (`NOTARY_PROFILE` unset => exit 0).
   - Correction:
     - DMG notarization target in the production chain must fail if `NOTARY_PROFILE` is empty.
     - Also fail if `SIGN_IDENTITY` is not a Developer ID identity (notarization requires it).
     - Optional convenience target may exist for local-only “best effort”, but must not be on the production chain.

6) **Verification gates must be DMG-appropriate**
   - Use `spctl --assess --type open` for DMG (not `--type exec`).
   - Also run `codesign --verify --strict --verbose=4` and `xcrun stapler validate`.
   - Correction: add `release-macos-cli-dmg-verify` to enforce these checks.

7) **publish-release integration**
   - Existing `publish-release` currently calls the zip-based macOS publish target.
   - Correction: add `publish-release-macos-cli-dmg-signed` and switch `publish-release` to call it.
   - Keep zip publish target available, but mark as legacy and remove from the default publish path.

8) **Testing / guardrails**
   - Notarization cannot be tested in CI (no macOS runners).
   - Correction: add a lightweight, Linux-capable self-check target that asserts required Makefile targets/variables and
     output naming patterns exist (regression guard).

## Key decisions (v2)

### 1) One DMG per arch

We ship:
- `dist/aifo-coder-${MACOS_DMG_VERSION}-macos-arm64.dmg`
- `dist/aifo-coder-${MACOS_DMG_VERSION}-macos-x86_64.dmg`

Rationale: matches existing per-arch approach, avoids universal binary complexities, keeps notarization simple.

### 2) Version the DMG, not the binary inside

Inside each DMG:
- `aifo-coder` (executable, stable filename)
- `README.md`, `NOTICE`, `LICENSE` (same set as the legacy zip flow)
- `docs/` (optional; same behavior as zip packaging)

### 3) Notarize and staple the DMG (not the binary)

- Notarize the DMG with `xcrun notarytool submit ... --wait`
- Staple the ticket to the DMG with `xcrun stapler staple <dmg>`
- Validate with:
  - `xcrun stapler validate <dmg>`
  - `spctl --assess --type open --verbose=4 <dmg>`

Rationale: `stapler` rejects raw executables; DMG is a first-class staplable container.

### 4) Binary signing prerequisites

The CLI binaries must be signed with hardened runtime and timestamp using the existing signing helper.
This remains a prerequisite for notarization of the DMG contents.

## Makefile integration (concrete)

### New variables

- `MACOS_DMG_VERSION ?= $(MACOS_ZIP_VERSION)`
- `MACOS_CLI_DMG_ARM64 = dist/aifo-coder-$(MACOS_DMG_VERSION)-macos-arm64.dmg`
- `MACOS_CLI_DMG_X86_64 = dist/aifo-coder-$(MACOS_DMG_VERSION)-macos-x86_64.dmg`
- `MACOS_CLI_RELEASE_FILES ?= README.md NOTICE LICENSE`
- `MACOS_CLI_DMG_VOLNAME ?= aifo-coder`

Stage directories:
- `dist/.dmg-cli-root-arm64/`
- `dist/.dmg-cli-root-x86_64/`

Additional versioned outputs (naming aligned with zip version by default):
- `MACOS_DMG_VERSION ?= $(MACOS_ZIP_VERSION)`
- `dist/aifo-coder-$(MACOS_DMG_VERSION)-macos-arm64.dmg`
- `dist/aifo-coder-$(MACOS_DMG_VERSION)-macos-x86_64.dmg`

### New targets (Darwin-only)

1) `release-macos-cli-dmg`
   - Requires normalized + signed binaries exist:
     - `dist/aifo-coder-macos-arm64`
     - `dist/aifo-coder-macos-x86_64`
   - Creates stage dirs:
     - copies `dist/aifo-coder-macos-${arch}` to stage as `aifo-coder`
     - copies `$(MACOS_CLI_RELEASE_FILES)` and `docs/` when present
   - Creates DMG per arch with `hdiutil create` (UDZO).

2) `release-macos-cli-dmg-sign`
   - Signs each DMG using existing `MACOS_SIGN_ONE_BINARY`.

3) `release-macos-cli-dmg-notarize`
   - Requires `NOTARY_PROFILE` (fail if missing).
   - Requires Developer ID identity (fail if not).
   - Runs:
     - `xcrun notarytool submit "$(DMG)" --keychain-profile "$(NOTARY_PROFILE)" --wait`
     - `xcrun stapler staple "$(DMG)"`

4) `release-macos-cli-dmg-verify`
   - Runs:
     - `codesign --verify --strict --verbose=4 "$(DMG)"`
     - `xcrun stapler validate "$(DMG)"`
     - `spctl --assess --type open --verbose=4 "$(DMG)"`

5) Aggregate: `release-macos-cli-dmg-signed`
   - Chain:
     1. `release-macos-binaries-normalize-local`
     2. `release-macos-binaries-sign`
     3. `release-macos-cli-dmg`
     4. `release-macos-cli-dmg-sign`
     5. `release-macos-cli-dmg-notarize`
     6. `release-macos-cli-dmg-verify`

### Publish-release integration

- Add `publish-release-macos-cli-dmg-signed` and update `publish-release` to call it.
- Keep `publish-release-macos-signed` (zip flow) for one migration cycle:
  - Mark as legacy and remove from default `publish-release`.
  - Optionally keep as a manual target for users who insist on zips.

## Verification & validation

### Automated (Darwin-only) gates

For each CLI DMG:
- `codesign --verify --strict --verbose=4 <dmg>`
- `xcrun stapler validate <dmg>`
- `spctl --assess --type open --verbose=4 <dmg>`

### Manual release checklist

1) Validate stapling + Gatekeeper acceptance:
- `xcrun stapler validate dist/aifo-coder-${MACOS_DMG_VERSION}-macos-arm64.dmg`
- `spctl --assess --type open --verbose=4 dist/aifo-coder-${MACOS_DMG_VERSION}-macos-arm64.dmg`

2) Simulate Finder “download/open/copy/run” workflow:
- Download DMG to `~/Downloads` (quarantine applied).
- Open DMG via Finder.
- Copy `aifo-coder` out to a writable location.
- Run `./aifo-coder --version` and confirm no malware dialog.

## Lightweight tests / guardrails (Linux/CI-capable)

Add a Makefile self-check target (e.g. `check-macos-cli-dmg-plan`) runnable on Linux that:
- asserts required Makefile target names exist (grep-based)
- asserts DMG output naming patterns exist (grep-based)

This is a regression guard, not a notarization test.

## Migration path

### Previous approach (legacy)
- Publish `...-macos-<arch>-signed.zip`
- Users unzip and run binary → Gatekeeper rejects as unnotarized

### New approach (v2)
- Publish `...-macos-<arch>.dmg` per arch
- DMG is signed + notarized + stapled
- Users open DMG and run/copy binary with reduced/no Gatekeeper warnings

### Backward compatibility
- Keep zip targets for one release cycle but remove them from default `publish-release`.
- After one stable release, consider removing zip notarization steps from the default paths to reduce maintenance.

## Condensed phased implementation plan (≤ 7 phases)

Phase 1: Align naming and release content list (single source of truth)
- Define DMG naming, stage dirs, and `MACOS_CLI_RELEASE_FILES`.

Phase 2: Build CLI DMGs (per arch)
- Implement `release-macos-cli-dmg`.

Phase 3: Sign CLI DMGs
- Implement `release-macos-cli-dmg-sign`.

Phase 4: Notarize + staple CLI DMGs (production-hard-fail)
- Implement `release-macos-cli-dmg-notarize` with hard requirements.

Phase 5: Verify CLI DMGs (Darwin-only gates)
- Implement `release-macos-cli-dmg-verify`.

Phase 6: Integrate into publish-release
- Add `release-macos-cli-dmg-signed` aggregate.
- Add `publish-release-macos-cli-dmg-signed` and switch `publish-release` to call it.
- Keep zip publish target as legacy.

Phase 7: Add Linux/CI guardrail self-check
- Add `check-macos-cli-dmg-plan` to prevent accidental removal of DMG flow wiring.
