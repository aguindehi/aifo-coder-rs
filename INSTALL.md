
Prerequisites:
- Docker installed and running
- GNU Make (recommended)
- Optional: Rust toolchain (for building the launcher locally)

Quick install:
```bash
make build
make install
aifo-coder --help
```

Notes:
- To build only full (fat) images: make build-fat
- To build only slim images: make build-slim
- The wrapper script aifo-coder will try to build the Rust launcher with cargo; if cargo is missing, it can build using Docker.
- Images drop apt and procps by default to reduce surface area. Keep them by passing KEEP_APT=1:
```bash
make KEEP_APT=1 build
```

Troubleshooting:
- Ensure your user can run Docker commands without sudo.
- If builds are slow, Docker may be pulling base layers; subsequent runs are faster.
- To list locally built images:
```bash
make docker-images
```

Useful Makefile targets:
- Build images:
  - make build, make build-fat, make build-slim
- Rebuild images without cache:
  - make rebuild, make rebuild-fat, make rebuild-slim
- Rebuild existing local images for your prefix:
  - make rebuild-existing, make rebuild-existing-nocache
- Build the Rust launcher:
  - make build-launcher
- Run tests:
  - make test
- macOS app and DMG (Darwin hosts only):
  - make release-app, make release-dmg, make release-dmg-sign
- Utilities:
  - make docker-images, make docker-enter, make checksums, make sbom, make loc

macOS code signing with a self‑signed certificate (no Apple Developer account)
- Create a self‑signed “Code Signing” certificate in your login keychain using Keychain Access:
  1) Open Keychain Access → Keychain: login → Menu: Keychain Access → Certificate Assistant → Create a Certificate…
  2) Name: choose a clear name (e.g., Migros AI Foundation Code Signer)
  3) Identity Type: Self Signed Root
  4) Certificate Type: Code Signing (ensures Extended Key Usage includes Code Signing)
  5) Key Size: 2048 (or 4096), Location: login keychain
  6) Ensure the certificate and its private key appear in the login keychain.
- Verify codesign can find and use it:
```bash
security find-identity -p basic -v | grep -i 'Code Sign' || true
security find-certificate -a -c "Migros AI Foundation Code Signer" -Z 2>/dev/null | sed -n '1,12p'
```
- Build and sign on macOS:
```bash
make release-dmg-sign SIGN_IDENTITY="Migros AI Foundation Code Signer"
```
- The Makefile will use basic signing flags for non‑Apple identities and will skip notarization automatically.

Tips:
- If prompted for key access, allow codesign to use the private key.
- If your login keychain is locked:
```bash
security unlock-keychain -p "<your-password>" login.keychain-db
```
- Clear extended attributes if you hit quarantine/signing issues:
```bash
xattr -cr dist/aifo-coder.app dist/aifo-coder.dmg
```
