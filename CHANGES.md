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
