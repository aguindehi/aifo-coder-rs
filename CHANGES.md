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
