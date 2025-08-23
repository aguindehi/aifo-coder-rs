# aifo-coder Source Code Scorecard

Date: 2025-08-23
Scope: Rust CLI launcher, Makefile, Dockerfile, AppArmor template, wrapper script, README, packaging targets.

Overall grade: B+ (85/100)

Grade summary (category — grade [score/10]):
- Architecture & Design — A- [9]
- Rust Code Quality — B+ [8]
- Security Posture (AppArmor, least privilege) — A- [9]
- Containerization & Dockerfile — B [8]
- Build & Release (Makefile, packaging) — B [8]
- Cross-Platform Support (macOS/Linux) — B [8]
- Documentation — A- [9]
- User Experience (CLI, wrapper) — B+ [8]
- Performance & Footprint — B [8]
- Testing & CI — C [6]

Key strengths
- Clear separation of concerns: Rust launcher orchestrates container runtime; images encapsulate agents.
- Sensible defaults: HOME, GNUPGHOME, XDG_RUNTIME_DIR, UID:GID mapping, minimal host mounts, and pass-through of relevant env vars.
- Security-aware: AppArmor enforced when supported; profile template provided; macOS/Colima helper targets for loading and observing AppArmor.
- Robust UX: TTY detection, exit code propagation, conservative shell escaping, lock to prevent concurrent runs.
- Packaging: macOS .app and .dmg targets in Makefile; multi-target release workflow (release-for-target, -for-mac, -for-linux).
- Documentation is modernized, consistent with Rust-based launcher, and pragmatic.

Gaps and risks
- AppArmor profile activation across environments: macOS/Colima defaults to docker-default (good), but docs don’t advertise AIFO_CODER_APPARMOR_PROFILE override; custom profiles require explicit load into Colima VM.
- Dockerfile includes full Rust toolchain and build-essential in the runtime images; increases footprint and attack surface unnecessarily.
- No automated tests (unit/integration) and no CI for building/packaging, which risks regressions.
- Wrapper script builds in a container without caching cargo registry/target; slow on repeated runs.
- macOS packaging lacks code signing/notarization; DMG creation is minimal (no backgrounds/links).
- Cross-compilation guidance is present, but there’s no .cargo/config.toml sample or linker hints in-repo for Linux-from-mac builds.

Detailed assessment

1) Architecture & Design — A- [9/10]
- The launcher is cohesive and comprehensible. It assembles docker run arguments with careful handling of environment, mounts, users, and TTY.
- The container images are split per agent with a shared base; promotes cache reuse.
- AppArmor support is integrated in a non-intrusive way with desired_apparmor_profile().

2) Rust Code Quality — B+ [8/10]
- Code is readable, uses clap/atty/which/once_cell appropriately.
- shell_escape and shell_join are conservative and safe for /bin/sh -lc.
- acquire_lock uses flock(EX|NB) with clear user-facing errors.
- Minor opportunities: more unit tests, better error contexts, and consider avoiding unsafe by using a crate like fd-lock.

3) Security Posture — A- [9/10]
- Least-privilege mounts; host ~/.gnupg mounted read-only and copied in; sensitive sysfs/proc areas denied by AppArmor template.
- AppArmor enabled when supported; macOS/Colima defaults to docker-default to avoid host-profile mismatch.
- Suggestions: add explicit mention of AIFO_CODER_APPARMOR_PROFILE in README and surface when a custom profile is detected/used.

4) Containerization & Dockerfile — B [8/10]
- Good base with dumb-init, GPG setup entrypoint, and per-agent layers.
- Issue: Rust toolchain and build-essential installed into runtime images; these should be moved to a builder stage and not shipped in final images.
- Consider slim variants: remove editors or make them optional to reduce size.

5) Build & Release — B [8/10]
- Make targets cover build, rebuild, release-for-target, mac app/dmg, AppArmor utils.
- Recent fixes group command subshells for mac packaging to maintain env propagation.
- Suggestions: Provide checksum generation for archives; produce SBOMs; ensure reproducible tarball ordering/mtimes; add a “dist-clean”.

6) Cross-Platform — B [8/10]
- macOS and Linux supported; targets for aarch64/x86_64 logic are in place.
- Guidance to install rustup targets exists, but no example .cargo/config.toml in-repo for cross-linkers.

7) Documentation — A- [9/10]
- README is thorough, aligned with Rust launcher.
- Missing mention of AIFO_CODER_APPARMOR_PROFILE env var; also note default behavior on macOS/Colima.

8) User Experience — B+ [8/10]
- Helpful error messages, auto TTY flags, host mapping, and clear Makefile help.
- Could add a --verbose flag to print the assembled docker run command.

9) Performance & Footprint — B [8/10]
- Docker-layer reuse is good; however, shipping Rust toolchains bloats the images.
- Wrapper’s in-container build path lacks cargo cache mounts; can be slow.

10) Testing & CI — C [6/10]
- No tests or CI. High risk of regressions (e.g., earlier mac packaging issues; AppArmor behavior across hosts).

Actionable next steps (prioritized)

1) Minimize runtime images
- Remove rustc, cargo, build-essential, libssl-dev from runtime images. If any agent needs build tools at runtime, make that variant opt-in or use a dedicated stage.
- Consider a multistage builder for any compiled extras; final stages contain only runtime deps.

2) Expose AppArmor override explicitly
- Document AIFO_CODER_APPARMOR_PROFILE in README under “Launcher control variables”.
- Optionally log which profile is used (docker-default vs custom) when starting a container.

3) Improve wrapper build speed
- When building in Docker, mount cargo registry and target caches:
  - -v "$HOME/.cargo/registry:/root/.cargo/registry"
  - -v "$HOME/.cargo/git:/root/.cargo/git"
  - -v "$PWD/target:/workspace/target"
- Consider using a slimmer Rust image (or a builder container) to reduce download time.

4) Add basic tests and CI
- Unit tests for: shell_escape/join, path_pair, candidate_lock_paths, desired_apparmor_profile.
- Integration smoke test (Linux CI): build launcher, run docker echo via the launcher.
- GitHub Actions (macOS + Ubuntu): build, package (release-for-target), archive artifacts.

5) Packaging polish
- Add SHA256 checksums for tar.gz files.
- For macOS: optional code signing/notarization steps; optionally add DMG background and Applications symlink.

6) Cross-compile ergonomics
- Include optional .cargo/config.toml examples for linkers:
  - [target.x86_64-unknown-linux-gnu] linker = "x86_64-unknown-linux-gnu-gcc"
- Provide a Makefile hint target to print recommended toolchain installation steps.

7) Enhanced diagnostics
- Add --verbose/--dry-run flags to print the docker run command prior to exec.
- Add a short “doctor” target to validate environment: docker presence, AppArmor support, colima status on macOS.

Notes carried forward (from prior SCORE.md): None (previous file was empty).

Proposed implementation tasks (next commits)
- Dockerfile: remove Rust-related build tools from final images; keep only in a builder stage.
- README: add AIFO_CODER_APPARMOR_PROFILE; clarify AppArmor behavior on macOS/Colima.
- aifo-coder wrapper: add cache volumes for cargo when building in Docker.
- src/main.rs: add --verbose flag and print the generated docker command when set.
- Tests: create tests for shell_escape/join and lock path selection; add minimal CI.
- Makefile: add checksums generation for packaged archives; add dist-clean.
