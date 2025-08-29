2025-08-29 16:20 Amir Guindehi <amir.guindehi@mgb.ch>

feat: finalize toolchain rollout; versioned specs, bootstrap, tests

- src/main.rs/src.lib.rs: add --toolchain-spec kind@version image mapping; optional
  --toolchain-bootstrap typescript=global; proxy bind per OS; Linux sidecars get
  host-gateway add-host; minor structured timing logs for proxy execs.
- tests: add route mapping unit test and negative proxy auth test; optional unix
  socket smoke; c-cpp dry-run test.
- docs: README and docs/TOOLCHAINS.md updated with new flags and examples.

2025-08-29 14:45 Amir Guindehi <amir.guindehi@mgb.ch>

test: add ignored live toolchain sidecar tests and docs

- tests/toolchain_live.rs: new ignored tests for rust and node; exercise real
  sidecars when run with --ignored.
- README.md: document how to run the live tests.

2025-08-29 14:10 Amir Guindehi <amir.guindehi@mgb.ch>

feat: Rollout Phase 1 toolchain sidecars, CLI, docs, tests

- src/lib.rs: implement toolchain_run; add sidecar helpers (run/exec/network),
  per-language caches, and AppArmor application; suppress docker output when not
  verbose; respect --dry-run for previews without creating networks.
- src/main.rs: add Toolchain subcommand with --toolchain-image and
  --no-toolchain-cache flags; bypass application lock for toolchain runs; print
  effective docker previews on --verbose/--dry-run; return 127 when docker is
  missing.
- README.md, man/aifo-coder.1: document Toolchain Phase 1 usage, options, and
  examples; add notes on running the new tests.
- tests/toolchain_phase1.rs: add dry-run integration tests for rust and node.
- Minor: tighten verbosity for docker run/stop/network output.

2025-08-27 12:35 Amir Guindehi <amir.guindehi@mgb.ch>

score: refresh scorecard per AGENT.md

- SCORE-before.md: overwritten with previous SCORE.md contents
- SCORE.md: refreshed comprehensive scoring per AGENT.md workflow
- CHANGES.md: recorded scoring update

2025-08-25 12:20 Amir Guindehi <amir.guindehi@mgb.ch>

score: refresh scorecard after CI smokes and doctor updates

- SCORE-before.md: overwritten with previous SCORE.md contents
- SCORE.md: updated grading reflecting CI help smokes, caching, and doctor AppArmor/security options
- CHANGES.md: record this scoring update

2025-08-25 10:55 Amir Guindehi <amir.guindehi@mgb.ch>

score: archive previous SCORE to SCORE-before and update scorecard

- SCORE-before.md: updated to contain previous SCORE.md contents
- SCORE.md: refreshed comprehensive scoring per AGENT.md workflow
- CHANGES.md: recorded scoring update

2025-08-24 20:45 Amir Guindehi <amir.guindehi@mgb.ch>

assets: add new images/ folder

- images/: add repository image assets
- CHANGES.md: record this non-code repository content change
- No source code modifications

2025-08-24 19:25 Amir Guindehi <amir.guindehi@mgb.ch>

docs: re-commit AGENT.md checklist

- AGENT.md: commit contributor checklist and workflow file to the repo
- CHANGES.md: record this documentation-only change
- Documentation-only change; no source code modifications

2025-08-24 19:05 Amir Guindehi <amir.guindehi@mgb.ch>

score: archive previous SCORE to SCORE-before and update scorecard

- SCORE-before.md: updated to contain previous SCORE.md contents
- SCORE.md: refreshed comprehensive scoring per AGENT.md workflow
- CHANGES.md: recorded scoring update

2025-08-24 18:55 Amir Guindehi <amir.guindehi@mgb.ch>

docs: commit AGENT.md contributor checklist and workflow

- AGENT.md: add explicit post-commit steps, CHANGES.md update, and scoring workflow
- Documentation-only change; no source code modifications

2025-08-24 11:15 Amir Guindehi <amir.guindehi@mgb.ch>

test: relax preview assert for single-quote escaping

- tests/docker_cmd_edges.rs: Adjust assertion to look for the POSIX single-quote escape sequence ('\"'\"') within the sh -lc script, making the test robust to nested quoting in the preview.

2025-08-24 10:22 Amir Guindehi <amir.guindehi@mgb.ch>

score: update scorecard; reflect Linux smoke and new tests

- Moved previous SCORE.md to SCORE-before.md (overwrite)
- Re-scored and rewrote SCORE.md (A 95/100) reflecting Linux CI smoke, edge tests, and cross-compiling examples
- No functional source changes; documentation/scorecard updates only

2025-08-23 13:55 Amir Guindehi <amir.guindehi@mgb.ch>

chore: score per AGENT.md; archive previous score

- Archived previous SCORE.md content to SCORE-before.md
- Wrote an updated comprehensive SCORE.md reflecting current repo state
- Proposed next steps per the new scoring recommendations

2025-08-23 13:15 Amir Guindehi <amir.guindehi@mgb.ch>

feat: tests; docker command preview; SBOM target

- src/main.rs: implement docker command preview and print on --verbose/--dry-run
- tests: add helpers tests for shell escaping, path pairing, file creation and lock paths
- CI: run cargo test on macOS and Linux
- Makefile: add sbom target; generate SBOM during release when cargo-cyclonedx is available

2025-08-23 12:30 Amir Guindehi <amir.guindehi@mgb.ch>

feat: verbose/dry-run; checksums; wrapper cache; docs; CI

- README: document AIFO_CODER_APPARMOR_PROFILE and default behaviors
- aifo-coder: mount cargo registry/git and target caches when building via Docker
- src/main.rs: add --verbose and --dry-run; print effective AppArmor profile in verbose mode
- Makefile: generate SHA256SUMS.txt in release-for-target; add checksums target
- CI: add GitHub Actions workflow for macOS and Ubuntu to build and upload dist artifacts

2025-08-23 12:00 Amir Guindehi <amir.guindehi@mgb.ch>

Scorecard updated; archived previous SCORE.md to SCORE-before.md

- Archived previous SCORE.md content into SCORE-before.md (overwrite).
- Wrote a new comprehensive SCORE.md with updated grades and next steps.
- No functional code changes in this commit; documentation-only update to score.
