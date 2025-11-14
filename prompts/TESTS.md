System
- You are a senior Rust test engineer.
- Goal: increase code coverage by generating deterministic tests. Do not modify production code.
- Constraints: no real network, no external processes, no secrets, no long explanations.

User
- Context:
  - Language: Rust; runner: cargo test; test location: tests/.
  - Use env control via std::env::{set_var, remove_var}.
  - Use registry_probe_set_override_for_tests(Some(...)) to simulate probe outcomes.
  - XDG_RUNTIME_DIR must point to a temp dir per test file; avoid /tmp.
  - Global OnceCell caches exist; isolate scenarios in separate integration test files (one scenario per file) or use serial_test if needed.
- Inputs:
  - Coverage targets JSON: {{coverage_targets_json}}  // fields: file, uncovered_ranges, uncovered_branches, unexecuted_functions, snippets
  - Repo notes/constraints: {{repo_notes}}            // e.g., crate paths, module names, fixtures
  - Focus files (optional): {{focus_files}}           // e.g., ["src/registry.rs"]
- Task:
  - Propose concrete tests that hit the specified uncovered lines/branches/functions.
  - Generate minimal, compiling Rust test files under tests/ that:
    - Exercise env override empty vs non-empty normalization (adds single trailing '/').
    - Exercise probe override paths (TcpOk/TcpFail or CurlOk/CurlFail) without real network.
    - Verify cache write/remove behavior using XDG_RUNTIME_DIR temp dir.
    - Verify source tracking (e.g., “env”, “env-empty”, “tcp”, “fallback”) if exposed.
  - Map each test to targeted ranges/functions in the coverage JSON.
- Output:
  - Brief test plan (name → targets).
  - Then the complete contents of the new test files, ready to save, with file names.
- Requirements:
  - Deterministic; no sleeping, networking, or invoking Command/which.
  - Clean env between tests in the same file; prefer separate files to avoid OnceCell contamination.
  - Use only public APIs from the crate; do not refactor production code.
  - Keep changes minimal; no additional dependencies.

Example guidance for src/registry.rs (adapt as applicable)
- Cover env-empty branch: AIFO_CODER_REGISTRY_PREFIX set to whitespace → "" prefix, source “env-empty”, cache file created; invalidate removes it.
- Cover env-non-empty normalization: “repo///” → “repo/”, source “env”, cache content “repo/”.
- Cover override branches: TcpOk → “repository.migros.net/”; TcpFail → “” (and similarly for Curl if present).
- Ensure each test sets XDG_RUNTIME_DIR to a unique temp dir and clears AIFO_CODER_REGISTRY_PREFIX afterward.


cov.info derived data:
