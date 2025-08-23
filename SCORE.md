# aifo-coder Source Code Scorecard

Date: 2025-08-23
Time: 12:00
Author: Amir Guindehi <amir.guindehi@mgb.ch>
Scope: Rust CLI launcher, Makefile, Dockerfile, AppArmor template, wrapper script, README, packaging targets.

Overall grade: A- (89/100)

Grade summary (category — grade [score/10]):
- Architecture & Design — A- [9]
- Rust Code Quality — B+ [8]
- Security Posture (AppArmor, least privilege) — A- [9]
- Containerization & Dockerfile — A- [9]
- Build & Release (Makefile, packaging) — B+ [8]
- Cross-Platform Support (macOS/Linux) — B [8]
- Documentation — A- [9]
- User Experience (CLI, wrapper) — B+ [8]
- Performance & Footprint — B+ [8]
- Testing & CI — C [6]

What improved since last score
- Runtime images are now minimized via multi-stage builds; Rust toolchain and build-essential have been removed from final runtime layers. Aider uses a builder stage to assemble a venv and the runtime stage installs only python3 plus the venv from the builder. This reduces footprint and attack surface.

Key strengths
- Clear separation of concerns: Rust launcher orchestrates container runtime; per-agent images keep runtimes reproducible.
- Sensible defaults: HOME, GNUPGHOME, XDG_RUNTIME_DIR, UID:GID mapping, minimal host mounts, and pass-through of curated env vars.
- Security-aware: AppArmor enforced when supported; profile template provided; macOS/Colima falls back to docker-default to avoid host-profile mismatch.
- Robust UX: TTY detection, exit code propagation, conservative shell escaping, workspace lock to prevent concurrent runs.
- Packaging: macOS .app and .dmg targets; release-for-target/-for-mac/-for-linux; aggregate release now builds launcher, .app, and .dmg first.
- Documentation is modernized and aligned with the Rust-based launcher.

Current gaps and risks
- AppArmor override env (AIFO_CODER_APPARMOR_PROFILE) not documented in README; users cannot easily select custom profiles.
- No automated tests or CI; regressions possible across platforms and packaging steps.
- Wrapper builds with Docker but lacks mounted cargo caches; slower repeat builds.
- macOS packaging: no code signing/notarization; DMG is minimal.
- Cross-compile ergonomics: examples for .cargo/config.toml linkers are not committed; guidance exists in README but could include copy-pastable snippets.
- Diagnostics: no --verbose/--dry-run flags to print the final docker command.

Detailed assessment

1) Architecture & Design — A- [9/10]
- Launcher is cohesive; docker run assembly is correct and minimal, with safe defaults.

2) Rust Code Quality — B+ [8/10]
- Solid use of clap/atty/which/once_cell. Error paths are surfaced with good messages.
- Opportunities: add unit tests, consider richer context on io::Error, optionally add fd-lock for portability.

3) Security Posture — A- [9/10]
- Good AppArmor posture and least-privilege mounts. Custom template restricts sensitive kernel interfaces.
- Improvement: document AIFO_CODER_APPARMOR_PROFILE and log effective profile on startup when verbose.

4) Containerization & Dockerfile — A- [9/10]
- Multi-stage minimized images; Aider’s venv is built separately and copied into runtime. No compilers in final runtime layers.
- Consider providing “-slim” variants without editors to further shrink size.

5) Build & Release — B+ [8/10]
- Makefile has robust targets; aggregate release now sequences build-launcher → build-app → build-dmg → releases.
- Add checksums and optional SBOM generation for dist artifacts.

6) Cross-Platform — B [8/10]
- Works on macOS and Linux; rustup-based cross builds supported. Provide .cargo/config.toml samples for Linux linkers on macOS.

7) Documentation — A- [9/10]
- Clear and up to date, but missing AIFO_CODER_APPARMOR_PROFILE mention and examples.

8) User Experience — B+ [8/10]
- Good defaults; add --verbose/--dry-run for transparency, especially for debugging mount/env behavior.

9) Performance & Footprint — B+ [8/10]
- Significant image reduction after removing build tools from runtime; further gains possible with slimmer editor set and cache-friendly wrapper build.

10) Testing & CI — C [6/10]
- Add minimal unit tests and CI (GitHub Actions) to stabilize behaviors across hosts.

Actionable next steps (prioritized)

1) README: AppArmor override docs
- Add AIFO_CODER_APPARMOR_PROFILE to “Launcher control variables” and describe defaults on Linux vs Docker-in-VM (macOS/Windows).

2) Wrapper: accelerate Docker-based build
- Mount cargo caches when building inside rust:bookworm:
  - -v "$HOME/.cargo/registry:/root/.cargo/registry"
  - -v "$HOME/.cargo/git:/root/.cargo/git"
  - -v "$PWD/target:/workspace/target"

3) Diagnostics
- Add --verbose and --dry-run flags to print the assembled docker run command; on --dry-run, exit before exec.

4) Tests and CI
- Unit tests: shell_escape/join, path_pair, candidate_lock_paths, desired_apparmor_profile.
- Integration smoke (Linux CI): run a simple echo via launcher in Docker.
- GitHub Actions: matrix macOS + Ubuntu; cache cargo; upload dist artifacts.

5) Packaging polish
- Generate SHA256 checksums for dist/*.tar.gz and .dmg.
- Optional: code sign and notarize macOS app; add DMG background and Applications symlink.

6) Cross-compile ergonomics
- Add optional .cargo/config.toml examples for cross linkers in-repo and reference from README.

7) Optional image slimming
- Offer “-slim” variants removing editors; keep a “-full” tag for convenience.

Notes carried forward (from previous SCORE.md)
- Most prior gaps remain except minimized runtime images, which are now addressed.

Proposed implementation tasks (next commits)
- README: document AIFO_CODER_APPARMOR_PROFILE and defaults.
- aifo-coder wrapper: add cargo cache mounts during Docker build.
- src/main.rs: add --verbose/--dry-run and print effective AppArmor profile when verbose.
- CI: add GitHub Actions workflow for macOS/Ubuntu, build + package + artifact upload.
- Makefile: add checksum generation target and include in release(-for-*) flow.
