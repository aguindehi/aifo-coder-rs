# Deterministic registry tests plan (2025-11-11)

- registry_env_empty_cache.rs → env-empty branch, cache write/remove path.
- registry_env_non_empty_normalization.rs → env non-empty normalization to single '/'.
- registry_cache_invalidate.rs → cache creation and invalidate removes cache file.
- Existing override tests (tcp/curl ok/fail) verify override early-return and no cache.
- registry_quiet_env_empty.rs → quiet env-empty path; source "env-empty"; cache write.
- registry_probe_env_tcp_ok_prefix.rs → env-probe tcp-ok; source "tcp"; no cache.
- registry_invalidate_no_file.rs → invalidation is safe when cache file is missing.
- registry_source_unknown_override.rs → override set → source "unknown"; no cache.
- registry_source_unknown_pristine.rs → no prior resolution → source "unknown".
- registry_quiet_env_probe_curl_ok.rs → env-probe curl-ok; source "curl"; no cache.
- registry_quiet_env_probe_curl_fail.rs → env-probe curl-fail; source "curl"; no cache.

Each test uses a unique XDG_RUNTIME_DIR temp dir and only public APIs:
preferred_registry_prefix[_quiet](), preferred_registry_source(),
invalidate_registry_cache(), registry_probe_set_override_for_tests().
