2025-11-11

Add quiet env-probe tcp-ok registry test

- tests/: add registry_quiet_env_probe_tcp_ok.rs (tcp-ok → "repository.migros.net/", source "tcp").
- Update TEST_PLAN.md to list quiet tcp-ok scenario; no cache writes for env-probe.

2025-11-11

Add precedence and nested normalization tests

- tests/: quiet override vs env override (quiet prefers override; no cache).
- tests/: non-quiet env override vs override (env wins; cache written).
- tests/: nested path normalization (acme/registry/// → acme/registry/).
- Update TEST_PLAN.md to include new scenarios.

2025-11-11

Add more deterministic registry tests (env-empty exact, override vs probe)

- tests/: add quiet env-empty exact override; source env-empty; cache write.
- tests/: add override vs env-probe conflict; override wins; no cache; source unknown.
- Update TEST_PLAN.md to include new scenarios.

2025-11-11

Add non-quiet env-probe tests for curl-ok/curl-fail/tcp-fail

- tests/: add env-probe curl-ok/curl-fail/tcp-fail (non-quiet) paths; no cache writes.
- Update TEST_PLAN.md to list new scenarios; unique XDG_RUNTIME_DIR per file.

2025-11-11

More deterministic registry tests (cache, spaces, unknown)

- tests/: add cache persistence, trailing spaces normalization, and env-probe unknown.
- Update TEST_PLAN.md to include new scenarios; no external IO added.

2025-11-11

Expand deterministic registry tests (curl env-probe, source unknown)

- tests/: add quiet env-probe curl-ok/curl-fail and source unknown tests.
- Each test uses unique XDG_RUNTIME_DIR; public APIs only; no external IO.

2025-11-11

Add deterministic registry tests to increase coverage

- tests/: add env-empty, env normalization, and cache invalidate tests.
- Each test uses unique XDG_RUNTIME_DIR and only public APIs; no external IO.

2025-11-11

Print prompt before JSON unless --raw

- scripts/cov2ai.py: add --raw flag; print prompts/TESTS.md before JSON by default.
- Keeps existing behavior when --raw is used; default shows prompt then JSON.

2025-11-10

Add CLI argument to control JSON preview size

- scripts/cov2ai.py: add --size argument (default 20000) for print truncation.
- Keeps default behavior; allows users to adjust preview bytes.

2025-11-10

Harden lcov parser to handle commas in function names

- Update scripts/cov2ai.py to split FN/FNDA at the first comma only.
- Accept optional DA checksum by allowing a third field; use first two parts.
- Prevent crashes when function names include commas; improves resilience.

2025-11-09

Finalize registry coverage tests; all passing

- Confirmed suite green: 307 passed, 34 skipped
- Registry coverage complete except external process/network paths by design
- No production code changes required

2025-11-09

Add quiet tests for env-probe tcp-ok and override curl-ok

- Add tests: registry_quiet_env_probe_tcp_ok.rs (tcp-ok → "repository.migros.net/" and source "tcp")
- Add tests: registry_quiet_override_curl_ok.rs (override CurlOk → "repository.migros.net/", source "unknown")
- Both verify no cache write and use per-file XDG_RUNTIME_DIR

2025-11-09

Add tests: override wins over env-probe; quiet override tcp-fail

- Add registry_override_vs_env_probe.rs (override beats env-probe; source unknown)
- Add registry_quiet_override_tcp_fail.rs (quiet override tcp-fail → "" and no cache)

2025-11-09

Fix registry precedence test expectation

- Update registry_env_override_wins.rs: expect source "tcp" after setting env probe
- Prefix remains "zeta/" from env override; validates precedence and source reporting

2025-11-09

Add tests for quiet env-empty and cache retrieval path

- Add test: registry_quiet_env_empty.rs (env-empty → "" with cache write, invalidate removes)
- Add test: registry_cache_retrieval_path.rs (non-quiet returns cached value when env cleared)

2025-11-09

Add tests for override curl modes and env-probe unknown

- Add tests: registry_override_curl_ok.rs, registry_override_curl_fail.rs
- Add test: registry_env_probe_unknown.rs for unknown env probe value
- Add test: registry_invalidate_no_file.rs ensures safe no-op when cache missing

2025-11-09

Add quiet env-probe tests and unknown-source coverage

- Add tests: registry_quiet_env_probe_curl.rs and registry_quiet_env_probe_tcp_fail.rs
- Add test: registry_source_unknown.rs covering source fallback

2025-11-09

Fix tests to use exported enum and add precedence test

- Fix tests: use exported RegistryProbeTestMode in override tests
- Add test: env override wins over AIFO_CODER_TEST_REGISTRY_PROBE (cache precedence)

2025-11-07 AIFO User <aifo@example.com>

Add v3 support: fast randomized support matrix

- Add "support" CLI subcommand and module scaffolding.
- Implement randomized worklist and worker/painter split (TTY-only animation).
- Cache agent --version checks; repaint rows immediately on cell completion.
- Add docs and tests: docker missing, deterministic shuffle, agent check count.

2025-09-29 AIFO User <aifo@example.com>

Add v4 spec: real installs for openhands/opencode/plandex

- Add spec/aifo-coder-implement-openhands-opencode-plandex-v4.spec with comprehensive plan.
- Detail OpenHands (uv tool install), OpenCode (npm global), Plandex (Go build) recipes.
- Document CA handling, cleanup patterns, multi-arch, and reproducibility.
- Outline Makefile targets, Dockerfile stage changes, tests (preview-only), and docs updates.
2025-11-09

Add tests for env-probe curl-fail and tcp-ok branches

- Add tests: registry_probe_env_curl_fail_prefix.rs (curl-fail → "" and source "curl")
- Add tests: registry_probe_env_tcp_ok_prefix.rs (tcp-ok → "repository.migros.net/" and source "tcp")
- Both verify no cache write occurs for env-probe paths and use per-file XDG_RUNTIME_DIR
