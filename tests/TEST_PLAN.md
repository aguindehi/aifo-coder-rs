# Deterministic registry tests plan (2025-11-11)

- registry_env_empty_cache.rs → env-empty branch, cache write/remove path.
- registry_env_non_empty_normalization.rs → env non-empty normalization to single '/'.
- registry_cache_invalidate.rs → cache creation and invalidate removes cache file.
- Existing override tests (tcp/curl ok/fail) verify override early-return and no cache.

Each test uses a unique XDG_RUNTIME_DIR temp dir and only public APIs:
preferred_registry_prefix[_quiet](), preferred_registry_source(),
invalidate_registry_cache(), registry_probe_set_override_for_tests().
